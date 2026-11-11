//! Minimal non-blocking HTTP GET for plugins.
//!
//! A plugin starts a request and polls it on later ticks; the request runs on
//! its own thread, so an unreachable host never stalls the plugin tick.
//! Only plain `http://` is supported (local-network APIs such as forgeDMX).

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const CONNECT_TIMEOUT: Duration = Duration::from_millis(1500);
const IO_TIMEOUT: Duration = Duration::from_secs(3);
const MAX_RESPONSE: u64 = 256 * 1024;
/// Upper bound on outstanding requests, so a plugin that never polls cannot
/// pile up threads.
const MAX_PENDING: usize = 32;

/// Outcome of a request, serialized as JSON for the plugin.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct HttpOutcome {
    pub ok: bool,
    pub status: u16,
    pub body: String,
    pub error: String,
}

impl HttpOutcome {
    fn failed(error: impl Into<String>) -> Self {
        Self { ok: false, status: 0, body: String::new(), error: error.into() }
    }
}

type Slot = Arc<Mutex<Option<HttpOutcome>>>;

#[derive(Default)]
pub struct HttpRequests {
    next_id: u32,
    pending: HashMap<u32, Slot>,
}

pub enum Poll {
    Pending,
    Unknown,
    Done(HttpOutcome),
}

impl HttpRequests {
    /// Starts a GET request; returns None when too many are outstanding.
    pub fn start(&mut self, url: String) -> Option<u32> {
        if self.pending.len() >= MAX_PENDING {
            return None;
        }
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1) % u32::MAX;
        let slot: Slot = Arc::default();
        let worker = Arc::clone(&slot);
        std::thread::Builder::new()
            .name("plugin-http".into())
            .spawn(move || {
                let outcome = get_blocking(&url);
                *worker.lock().unwrap() = Some(outcome);
            })
            .ok()?;
        self.pending.insert(id, slot);
        Some(id)
    }

    /// Returns the outcome once and forgets the request.
    pub fn poll(&mut self, id: u32) -> Poll {
        let Some(slot) = self.pending.get(&id) else {
            return Poll::Unknown;
        };
        let done = slot.lock().unwrap().take();
        match done {
            Some(outcome) => {
                self.pending.remove(&id);
                Poll::Done(outcome)
            }
            None => Poll::Pending,
        }
    }
}

/// Splits `http://host[:port][/path]` into (host:port, path).
fn parse_url(url: &str) -> Result<(String, String, String), String> {
    let rest = url
        .strip_prefix("http://")
        .ok_or_else(|| "only http:// URLs are supported".to_string())?;
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    if authority.is_empty() {
        return Err("missing host".into());
    }
    let host = authority.rsplit_once(':').map_or(authority, |(h, _)| h);
    let addr = if authority.contains(':') {
        authority.to_string()
    } else {
        format!("{authority}:80")
    };
    Ok((addr, host.to_string(), path.to_string()))
}

pub fn get_blocking(url: &str) -> HttpOutcome {
    let (addr, host, path) = match parse_url(url) {
        Ok(parts) => parts,
        Err(e) => return HttpOutcome::failed(e),
    };
    let sock_addr = match addr.to_socket_addrs().map(|mut a| a.next()) {
        Ok(Some(a)) => a,
        Ok(None) => return HttpOutcome::failed(format!("cannot resolve {addr}")),
        Err(e) => return HttpOutcome::failed(format!("cannot resolve {addr}: {e}")),
    };
    let mut stream = match TcpStream::connect_timeout(&sock_addr, CONNECT_TIMEOUT) {
        Ok(s) => s,
        Err(e) => return HttpOutcome::failed(format!("connect to {addr}: {e}")),
    };
    let _ = stream.set_read_timeout(Some(IO_TIMEOUT));
    let _ = stream.set_write_timeout(Some(IO_TIMEOUT));
    // HTTP/1.0 + Connection: close: the body ends at EOF, no chunked encoding.
    let request = format!(
        "GET {path} HTTP/1.0\r\nHost: {host}\r\nAccept: application/json\r\nConnection: close\r\n\r\n"
    );
    if let Err(e) = stream.write_all(request.as_bytes()) {
        return HttpOutcome::failed(format!("send: {e}"));
    }
    let mut raw = Vec::new();
    if let Err(e) = stream.take(MAX_RESPONSE).read_to_end(&mut raw) {
        if raw.is_empty() {
            return HttpOutcome::failed(format!("read: {e}"));
        }
    }
    parse_response(&raw)
}

fn parse_response(raw: &[u8]) -> HttpOutcome {
    let text = String::from_utf8_lossy(raw);
    let Some((head, body)) = text.split_once("\r\n\r\n") else {
        return HttpOutcome::failed("malformed HTTP response");
    };
    let status = head
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse::<u16>().ok());
    match status {
        Some(status) => HttpOutcome {
            ok: (200..300).contains(&status),
            status,
            body: body.to_string(),
            error: String::new(),
        },
        None => HttpOutcome::failed("malformed HTTP status line"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    fn serve_once(response: &'static str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let mut buf = [0u8; 1024];
            let _ = s.read(&mut buf);
            s.write_all(response.as_bytes()).unwrap();
        });
        format!("http://{addr}/api/state")
    }

    #[test]
    fn parses_urls() {
        assert_eq!(
            parse_url("http://10.0.0.5:3000/api/state").unwrap(),
            ("10.0.0.5:3000".into(), "10.0.0.5".into(), "/api/state".into())
        );
        assert_eq!(
            parse_url("http://host").unwrap(),
            ("host:80".into(), "host".into(), "/".into())
        );
        assert!(parse_url("https://host/").is_err());
    }

    #[test]
    fn gets_body_and_status() {
        let url = serve_once("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n{\"messages\":3}");
        let out = get_blocking(&url);
        assert_eq!(out.status, 200);
        assert!(out.ok);
        assert_eq!(out.body, "{\"messages\":3}");
    }

    #[test]
    fn non_2xx_is_not_ok() {
        let url = serve_once("HTTP/1.1 404 Not Found\r\n\r\nnope");
        let out = get_blocking(&url);
        assert_eq!((out.ok, out.status), (false, 404));
    }

    #[test]
    fn refused_connection_is_an_error() {
        let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let out = get_blocking(&format!("http://127.0.0.1:{port}/"));
        assert!(!out.ok);
        assert!(out.error.contains("connect"), "{}", out.error);
    }

    #[test]
    fn registry_polls_until_done() {
        let url = serve_once("HTTP/1.0 200 OK\r\n\r\nhi");
        let mut reqs = HttpRequests::default();
        let id = reqs.start(url).unwrap();
        let outcome = loop {
            match reqs.poll(id) {
                Poll::Done(o) => break o,
                Poll::Pending => std::thread::sleep(Duration::from_millis(5)),
                Poll::Unknown => panic!("lost request"),
            }
        };
        assert_eq!(outcome.body, "hi");
        assert!(matches!(reqs.poll(id), Poll::Unknown));
    }
}
