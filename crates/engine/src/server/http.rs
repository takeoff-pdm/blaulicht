//! Plain HTTP on the remote-client port: the web frontend's static bundle
//! (`index.html`, `blaulicht_web.js`, `blaulicht_web_bg.wasm`, built by
//! `crates/web/build.sh`), so a browser opening `http://<device>:<port>/`
//! gets the UI and connects back to the same port.
//!
//! [`peek_request`] looks at a new connection's request head without
//! consuming it. A websocket upgrade goes on to the protocol (tungstenite
//! reads the head itself); any other request is answered by [`respond`] and
//! the connection closed. GET and HEAD only, files from one directory (the
//! bundle), a precompressed `<file>.gz` when the browser takes gzip, ETags
//! for revalidation. Without a bundle, `/` explains how to build one.

use std::{
    fs,
    io::{self, Read, Write},
    net::{Shutdown, TcpStream},
    path::{Component, Path, PathBuf},
    // std's epoch: `fs::Metadata::modified` is a std `SystemTime`.
    time::{Duration, UNIX_EPOCH},
};
use web_time::Instant;

/// Most bytes of a request head; longer ones are refused.
const MAX_HEAD_BYTES: usize = 8 << 10;
/// How long to wait between two looks at a request head still arriving.
const PEEK_POLL: Duration = Duration::from_millis(5);
/// How long to read (and drop) what a client still sends after the answer,
/// so closing doesn't reset the connection under the answer.
const LINGER: Duration = Duration::from_millis(200);
/// How long one send of an HTTP answer may make no progress before the
/// connection is given up.
const HTTP_WRITE_TIMEOUT: Duration = Duration::from_secs(30);

/// What a new connection asks for.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Peeked {
    /// A websocket upgrade: a remote client.
    Upgrade,
    /// A plain HTTP request.
    Http(Request),
    /// Not HTTP, too long, or the peer left before finishing the head.
    Invalid,
}

/// A plain HTTP request, as far as serving files needs it.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Request {
    pub method: String,
    pub path: String,
    accepts_gzip: bool,
    if_none_match: Option<String>,
    /// The length of the request head, which is still unread.
    head_len: usize,
}

/// Reads the request head of `stream` without consuming it (so tungstenite
/// can still read an upgrade), waiting until `timeout` for it to arrive.
pub(crate) fn peek_request(stream: &TcpStream, timeout: Duration) -> io::Result<Peeked> {
    let deadline = Instant::now() + timeout;
    let mut buffer = vec![0; MAX_HEAD_BYTES];
    loop {
        // Blocks until data arrives (up to the stream's read timeout).
        let len = stream.peek(&mut buffer)?;
        if len == 0 {
            return Ok(Peeked::Invalid);
        }
        let mut headers = [httparse::EMPTY_HEADER; 64];
        let mut request = httparse::Request::new(&mut headers);
        match request.parse(&buffer[..len]) {
            Ok(httparse::Status::Complete(head_len)) => return Ok(classify(&request, head_len)),
            Ok(httparse::Status::Partial) if len < buffer.len() && Instant::now() < deadline => {
                std::thread::sleep(PEEK_POLL);
            }
            Ok(httparse::Status::Partial) | Err(_) => return Ok(Peeked::Invalid),
        }
    }
}

fn classify(request: &httparse::Request<'_, '_>, head_len: usize) -> Peeked {
    let header = |name: &str| {
        request
            .headers
            .iter()
            .find(|header| header.name.eq_ignore_ascii_case(name))
            .map(|header| String::from_utf8_lossy(header.value).into_owned())
    };
    let upgrade = header("upgrade").is_some_and(|value| value.eq_ignore_ascii_case("websocket"));
    if upgrade {
        return Peeked::Upgrade;
    }
    let (Some(method), Some(path)) = (request.method, request.path) else {
        return Peeked::Invalid;
    };
    Peeked::Http(Request {
        method: method.to_string(),
        path: path.to_string(),
        accepts_gzip: header("accept-encoding").is_some_and(|value| {
            value
                .split(',')
                .any(|coding| coding.trim().split(';').next() == Some("gzip"))
        }),
        if_none_match: header("if-none-match"),
        head_len,
    })
}

/// A response, before it is written.
#[derive(Debug)]
pub(crate) struct Response {
    pub status: u16,
    headers: Vec<(&'static str, String)>,
    pub body: Vec<u8>,
}

impl Response {
    fn new(status: u16, content_type: &str, body: Vec<u8>) -> Self {
        Self {
            status,
            headers: vec![("Content-Type", content_type.to_string())],
            body,
        }
    }

    fn text(status: u16, text: &str) -> Self {
        Self::new(
            status,
            "text/plain; charset=utf-8",
            text.as_bytes().to_vec(),
        )
    }

    fn reason(&self) -> &'static str {
        match self.status {
            200 => "OK",
            304 => "Not Modified",
            404 => "Not Found",
            405 => "Method Not Allowed",
            _ => "Internal Server Error",
        }
    }
}

/// What the bundle directory serves for `request` (`web_root`: the bundle
/// directory, if one is configured or was found).
pub(crate) fn response_for(request: &Request, web_root: Option<&Path>) -> Response {
    if request.method != "GET" && request.method != "HEAD" {
        let mut response = Response::text(405, "Only GET and HEAD are served here.\n");
        response.headers.push(("Allow", "GET, HEAD".to_string()));
        return response;
    }
    let path = request.path.split(['?', '#']).next().unwrap_or("/");
    let Some(relative) = file_path(path) else {
        return Response::text(404, "Not found.\n");
    };
    let bundle = web_root.filter(|root| root.join("index.html").is_file());
    let Some(root) = bundle else {
        return if relative == Path::new("index.html") {
            Response::new(200, "text/html; charset=utf-8", no_bundle_page(web_root))
        } else {
            Response::text(404, "Not found (the web UI is not built).\n")
        };
    };

    let file = root.join(&relative);
    let gzipped = PathBuf::from(format!("{}.gz", file.display()));
    let (served, encoding) = if request.accepts_gzip && gzipped.is_file() {
        (gzipped, Some("gzip"))
    } else {
        (file.clone(), None)
    };
    let Some(metadata) = fs::metadata(&served)
        .ok()
        .filter(|metadata| metadata.is_file())
    else {
        return Response::text(404, "Not found.\n");
    };
    let modified = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |since| since.as_millis());
    let etag = format!("\"{:x}-{modified:x}\"", metadata.len());

    let mut response = if request.if_none_match.as_deref() == Some(etag.as_str()) {
        Response::new(304, content_type(&file), Vec::new())
    } else {
        match fs::read(&served) {
            Ok(body) => Response::new(200, content_type(&file), body),
            Err(err) => {
                tracing::warn!("Web UI: could not read {}: {err}", served.display());
                return Response::text(500, "Could not read the file.\n");
            }
        }
    };
    // Revalidate every time (cheap with the ETag), so a rebuilt bundle is
    // picked up at once.
    response
        .headers
        .push(("Cache-Control", "no-cache".to_string()));
    response.headers.push(("ETag", etag));
    response
        .headers
        .push(("Vary", "Accept-Encoding".to_string()));
    if let Some(encoding) = encoding {
        response
            .headers
            .push(("Content-Encoding", encoding.to_string()));
    }
    response
}

/// Answers `request` on `stream` and closes it.
pub(crate) fn respond(mut stream: TcpStream, request: &Request, web_root: Option<&Path>) {
    // Take the peeked head off the socket: closing a socket with unread
    // input resets the connection, and the client may lose the answer.
    let mut head = vec![0; request.head_len];
    if stream.read_exact(&mut head).is_err() {
        return;
    }
    let response = response_for(request, web_root);
    // The handshake's short write timeout is too tight for the wasm bundle
    // over slow Wi-Fi; it only fires when a send makes no progress at all.
    let _ = stream.set_write_timeout(Some(HTTP_WRITE_TIMEOUT));
    tracing::debug!(
        "Web UI: {} {} -> {}",
        request.method,
        request.path,
        response.status
    );
    let mut head = format!("HTTP/1.1 {} {}\r\n", response.status, response.reason());
    for (name, value) in &response.headers {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    head.push_str(&format!(
        "Content-Length: {}\r\nConnection: close\r\n\r\n",
        response.body.len()
    ));
    let mut written = stream.write_all(head.as_bytes());
    if written.is_ok() && request.method != "HEAD" {
        written = stream.write_all(&response.body);
    }
    if let Err(err) = written.and_then(|()| stream.flush()) {
        tracing::debug!("Web UI: could not answer {}: {err}", request.path);
        return;
    }
    // Let the client read the answer before the socket goes: signal the end,
    // then drop whatever it still sends (a request body) for a moment.
    let _ = stream.shutdown(Shutdown::Write);
    let _ = stream.set_read_timeout(Some(LINGER));
    let deadline = Instant::now() + LINGER;
    let mut sink = [0; 1024];
    while Instant::now() < deadline {
        match stream.read(&mut sink) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
    }
}

/// The file a URL path names, relative to the bundle (`/` is `index.html`),
/// or `None` if it would leave the bundle directory.
fn file_path(url_path: &str) -> Option<PathBuf> {
    let trimmed = url_path.trim_start_matches('/');
    if trimmed.is_empty() {
        return Some(PathBuf::from("index.html"));
    }
    if trimmed.contains('\\') || trimmed.contains('%') {
        return None;
    }
    let path = PathBuf::from(trimmed);
    path.components()
        .all(|component| matches!(component, Component::Normal(_)))
        .then_some(path)
}

fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("wasm") => "application/wasm",
        Some("css") => "text/css; charset=utf-8",
        Some("json") => "application/json",
        Some("png") => "image/png",
        Some("svg") => "image/svg+xml",
        Some("ico") => "image/x-icon",
        _ => "application/octet-stream",
    }
}

/// `/` when there is no bundle: what to do about it.
fn no_bundle_page(web_root: Option<&Path>) -> Vec<u8> {
    let looked = web_root.map_or_else(
        || "(no directory configured or found)".to_string(),
        |root| html_escape(&root.display().to_string()),
    );
    format!(
        "<!DOCTYPE html>\n<html><head><meta charset=\"utf-8\"><title>Blaulicht</title></head>\
         <body style=\"font-family: system-ui, sans-serif; max-width: 40em; margin: 3em auto\">\
         <h1>Blaulicht {version}</h1>\
         <p>The engine is running, but the web UI is not built.</p>\
         <p>Build it with <code>crates/web/build.sh</code> (or <code>scripts/blenv build web</code>), \
         or point <code>web_ui_dir</code> in <code>config.toml</code> at a built bundle.</p>\
         <p>Looked in: <code>{looked}</code></p></body></html>\n",
        version = env!("CARGO_PKG_VERSION"),
    )
    .into_bytes()
}

fn html_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Where the web bundle is: `configured` (`config.toml`'s `web_ui_dir`,
/// relative to the config file's folder) if set, else `web/` next to the
/// running binary or one folder up (so `target/web` serves the binaries in
/// `target/debug` and `target/release`).
pub fn web_root(configured: Option<&Path>, config_dir: Option<&Path>) -> Option<PathBuf> {
    if let Some(configured) = configured {
        return Some(match config_dir {
            Some(dir) if configured.is_relative() => dir.join(configured),
            _ => configured.to_path_buf(),
        });
    }
    let exe = std::env::current_exe().ok()?;
    let exe_dir = exe.parent()?;
    let candidates = [
        Some(exe_dir.join("web")),
        exe_dir.parent().map(|up| up.join("web")),
    ];
    candidates
        .iter()
        .flatten()
        .find(|dir| dir.join("index.html").is_file())
        .cloned()
        .or_else(|| candidates[0].clone())
}
