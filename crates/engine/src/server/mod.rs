//! The engine's websocket server: remote frontends attach here (the
//! protocol is in [`protocol`], the Rust client is
//! [`crate::client::RemoteClient`]).
//!
//! - One listener thread accepts connections on `config.toml`'s `port`
//!   (`server_enabled = false` turns it off). A plain HTTP request there
//!   gets the web frontend's files ([`http`]); a websocket upgrade becomes a
//!   remote client. Each client gets its own
//!   thread, a [`ClientConnection`] (a fresh `ClientId::Remote`, a new
//!   operator session, a hub subscription, a bus connection and a view-state
//!   publisher) and the engine's state to read. Sync `tungstenite`, no async
//!   runtime.
//! - **The engine never waits for a client.** Client threads read engine
//!   state under short read locks, never while touching the socket, and the
//!   engine hands them messages through queues that drop their oldest entry
//!   when full: the hub subscription ([`ServerOptions::queue_capacity`]) and
//!   the bus connection (`event::CONNECTION_CAPACITY`). The show state comes
//!   from the plugin tick's bytes ([`snapshot`]).
//! - **A stalled client can't grow memory without bound.** The socket is
//!   non-blocking. Once it stops taking data, the client's thread stops
//!   producing frames (at most one frame waits in the websocket's buffer),
//!   skips show, session and stream updates (they are "latest only" and are
//!   sent again once it catches up), and leaves system messages in the capped
//!   hub queue. A client that takes nothing for
//!   [`ServerOptions::stall_timeout`] is disconnected.
//! - Closing a connection (either side, or a stall) drops the
//!   `ClientConnection`, which closes the session and drops the client's
//!   view-state entries.

pub mod http;
pub mod json;
pub mod protocol;
pub mod snapshot;
#[cfg(test)]
mod tests;

use std::{
    collections::{BTreeMap, BTreeSet, HashSet, VecDeque},
    io::{self, ErrorKind},
    net::{SocketAddr, TcpListener, TcpStream},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, MutexGuard, PoisonError, RwLock, RwLockReadGuard,
    },
    thread::{self, JoinHandle},
    time::Duration,
};
use web_time::Instant;

use blaulicht_shared::{ControlEventMessage, EventOriginator};
use tungstenite::{
    protocol::{frame::coding::CloseCode, CloseFrame, WebSocketConfig},
    Message, WebSocket,
};

use crate::{
    event::EventBusHandle,
    frontend_ui::FrontendUiSnapshot,
    hub::{BusBody, BusMessage, ClientConnection, ClientId, SessionId, SystemHub},
    msg::SystemMessage,
    state::{AppState, NUM_DMX_UNIVERSES},
};

use protocol::{
    fingerprint, ClientMessage, DmxUniverse, Encoding, EngineStatus, Frame, ServerMessage,
    SessionSnapshot, ShowSnapshot, ShowState, SpectrogramColumn, SpectrogramUpdate, StageSnapshot,
    Stream, WireSystemMessage, PROTOCOL_VERSION,
};
use snapshot::ShowBytes;

/// Most remote clients connected at once; more are rejected.
pub const MAX_CLIENTS: usize = 16;
/// Most system messages (log lines, results, ...) waiting for one remote
/// client before its oldest are dropped.
pub const REMOTE_QUEUE_CAPACITY: usize = 4096;
/// How long a client may take no data before it is disconnected.
pub const STALL_TIMEOUT: Duration = Duration::from_secs(30);
/// Most bytes in one websocket message, either way.
pub const MAX_MESSAGE_BYTES: usize = 64 << 20;

/// Time allowed for the websocket handshake and the client's `Hello`.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);
/// How long a client thread sleeps when there was nothing to do.
const IDLE_SLEEP: Duration = Duration::from_millis(4);
/// How often the listener checks for a stop while no one connects.
const ACCEPT_POLL: Duration = Duration::from_millis(50);
/// Most hub or bus messages one client thread forwards per round, so the
/// other updates get their turn.
const MESSAGES_PER_ROUND: usize = 256;

const SHOW_INTERVAL: Duration = snapshot::OFFER_INTERVAL;
const SESSION_INTERVAL: Duration = Duration::from_millis(20);
const STAGE_INTERVAL: Duration = Duration::from_millis(100);
const DMX_INTERVAL: Duration = Duration::from_millis(33);
const HEALTH_INTERVAL: Duration = Duration::from_millis(500);
const SPECTROGRAM_INTERVAL: Duration = Duration::from_millis(50);
const PLUGIN_UI_INTERVAL: Duration = Duration::from_millis(50);
const STATUS_INTERVAL: Duration = Duration::from_millis(500);
/// How often a client's reported view state reaches the engine's mirror at
/// most (its updates queue until a plugin asks).
const PUBLISH_UI_INTERVAL: Duration = Duration::from_millis(250);
/// How often a client thread updates its [`ClientInfo`].
const INFO_INTERVAL: Duration = Duration::from_millis(100);

/// How to run the server.
#[derive(Clone, Debug)]
pub struct ServerOptions {
    pub bind: SocketAddr,
    pub max_clients: usize,
    /// Most system messages queued for one client.
    pub queue_capacity: usize,
    /// How long a client may take no data before it is disconnected.
    pub stall_timeout: Duration,
    /// The web frontend's bundle, served to plain HTTP requests
    /// ([`http::web_root`]). Without one, `/` explains how to build it.
    pub web_root: Option<PathBuf>,
}

impl ServerOptions {
    pub fn new(bind: SocketAddr) -> Self {
        Self {
            bind,
            max_clients: MAX_CLIENTS,
            queue_capacity: REMOTE_QUEUE_CAPACITY,
            stall_timeout: STALL_TIMEOUT,
            web_root: None,
        }
    }
}

/// One connected client, as the server sees it.
#[derive(Clone, Debug)]
pub struct ClientInfo {
    pub client: ClientId,
    pub session: SessionId,
    pub peer: SocketAddr,
    pub encoding: Encoding,
    /// The socket takes no more data right now; updates wait.
    pub backlogged: bool,
    /// System messages waiting in the client's hub queue.
    pub queued_system: usize,
    pub sent_frames: u64,
    pub sent_bytes: u64,
    /// Show snapshots sent, and their bytes.
    pub show_frames: u64,
    pub show_bytes: u64,
    /// Frames that didn't fit the websocket's write buffer and were dropped.
    pub dropped_frames: u64,
}

struct Shared {
    options: ServerOptions,
    app: Arc<AppState>,
    hub: SystemHub,
    bus: EventBusHandle<BusMessage>,
    stop: AtomicBool,
    clients: Mutex<BTreeMap<u64, ClientSlot>>,
    /// The last show frame per encoding, shared by every client of that
    /// encoding.
    show_frames: [CachedFrame; 2],
}

/// An encoded show snapshot and its digest.
type CachedFrame = Mutex<Option<(u64, Arc<Frame>)>>;

enum ClientSlot {
    /// Counted against `max_clients` while its `Hello` is checked.
    Joining,
    Connected(ClientInfo),
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn read<T>(lock: &RwLock<T>) -> RwLockReadGuard<'_, T> {
    lock.read().unwrap_or_else(PoisonError::into_inner)
}

impl Shared {
    /// The show frame for `show` in `encoding`, encoded once per digest.
    fn show_frame(&self, encoding: Encoding, show: &ShowBytes) -> anyhow::Result<Arc<Frame>> {
        let mut cached = lock(&self.show_frames[encoding as usize]);
        if let Some((digest, frame)) = cached.as_ref() {
            if *digest == show.digest {
                return Ok(Arc::clone(frame));
            }
        }
        let frame = Arc::new(encoding.encode(&ServerMessage::Show(ShowSnapshot {
            digest: show.digest,
            state: ShowState::Encoded(Arc::clone(&show.bytes)),
        }))?);
        *cached = Some((show.digest, Arc::clone(&frame)));
        Ok(frame)
    }
}

/// The websocket server.
pub struct Server;

impl Server {
    /// Listens on `options.bind` and serves remote clients on their own
    /// threads until the returned handle is stopped or dropped.
    pub(crate) fn start(
        options: ServerOptions,
        app: Arc<AppState>,
        hub: SystemHub,
        bus: EventBusHandle<BusMessage>,
    ) -> io::Result<ServerHandle> {
        let listener = TcpListener::bind(options.bind)?;
        listener.set_nonblocking(true)?;
        let addr = listener.local_addr()?;
        let shared = Arc::new(Shared {
            options,
            app,
            hub,
            bus,
            stop: AtomicBool::new(false),
            clients: Mutex::new(BTreeMap::new()),
            show_frames: [Mutex::new(None), Mutex::new(None)],
        });
        let listener = {
            let shared = Arc::clone(&shared);
            thread::Builder::new()
                .name("server".into())
                .spawn(move || accept_loop(listener, shared))?
        };
        Ok(ServerHandle {
            addr,
            shared,
            listener: Some(listener),
        })
    }
}

/// A running server. Dropping it stops the server and disconnects every
/// client.
pub struct ServerHandle {
    addr: SocketAddr,
    shared: Arc<Shared>,
    listener: Option<JoinHandle<()>>,
}

impl ServerHandle {
    /// The address it listens on (with the port the OS picked for port 0).
    pub fn local_addr(&self) -> SocketAddr {
        self.addr
    }

    /// The connected clients.
    pub fn clients(&self) -> Vec<ClientInfo> {
        lock(&self.shared.clients)
            .values()
            .filter_map(|slot| match slot {
                ClientSlot::Connected(info) => Some(info.clone()),
                ClientSlot::Joining => None,
            })
            .collect()
    }

    /// Stops accepting, disconnects every client and waits for their
    /// threads.
    pub fn stop(mut self) {
        self.stop_and_join();
    }

    fn stop_and_join(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
        if let Some(listener) = self.listener.take() {
            let _ = listener.join();
        }
    }
}

impl Drop for ServerHandle {
    fn drop(&mut self) {
        self.stop_and_join();
    }
}

fn accept_loop(listener: TcpListener, shared: Arc<Shared>) {
    let mut clients: Vec<JoinHandle<()>> = Vec::new();
    let mut next_id = 0u64;
    while !shared.stop.load(Ordering::Relaxed) {
        clients.retain(|client| !client.is_finished());
        match listener.accept() {
            Ok((stream, peer)) => {
                let shared = Arc::clone(&shared);
                let id = next_id;
                next_id += 1;
                let spawned = thread::Builder::new()
                    .name(format!("remote-{id}"))
                    .spawn(move || serve(shared, id, stream, peer));
                match spawned {
                    Ok(client) => clients.push(client),
                    Err(err) => tracing::warn!("Could not start a thread for {peer}: {err}"),
                }
            }
            Err(err) if err.kind() == ErrorKind::WouldBlock => thread::sleep(ACCEPT_POLL),
            Err(err) => {
                tracing::warn!("Remote client server: accept failed: {err}");
                thread::sleep(ACCEPT_POLL);
            }
        }
    }
    for client in clients {
        let _ = client.join();
    }
}

/// The tungstenite settings both ends use.
pub(crate) fn websocket_config() -> WebSocketConfig {
    WebSocketConfig {
        max_message_size: Some(MAX_MESSAGE_BYTES),
        max_frame_size: Some(MAX_MESSAGE_BYTES),
        // Large enough for any one frame (a show snapshot); the client
        // thread stops writing as soon as the socket backs up, so it never
        // fills.
        max_write_buffer_size: 2 * MAX_MESSAGE_BYTES,
        ..WebSocketConfig::default()
    }
}

fn serve(shared: Arc<Shared>, id: u64, stream: TcpStream, peer: SocketAddr) {
    match connect(&shared, id, stream, peer) {
        Ok(Some((client, reason))) => {
            tracing::info!("Remote client {client} ({peer}) disconnected: {reason}");
        }
        Ok(None) => {}
        Err(err) => tracing::debug!("Remote connection from {peer} failed: {err:#}"),
    }
    lock(&shared.clients).remove(&id);
}

/// Handshake, `Hello`, then the client's session. Returns the client and why
/// it ended, or `None` if it never became a client.
fn connect(
    shared: &Arc<Shared>,
    id: u64,
    stream: TcpStream,
    peer: SocketAddr,
) -> anyhow::Result<Option<(ClientId, String)>> {
    stream.set_nodelay(true)?;
    stream.set_read_timeout(Some(HANDSHAKE_TIMEOUT))?;
    stream.set_write_timeout(Some(HANDSHAKE_TIMEOUT))?;
    match http::peek_request(&stream, HANDSHAKE_TIMEOUT)? {
        http::Peeked::Upgrade => {}
        http::Peeked::Http(request) => {
            http::respond(stream, &request, shared.options.web_root.as_deref());
            return Ok(None);
        }
        http::Peeked::Invalid => anyhow::bail!("not an HTTP request"),
    }
    let mut ws = tungstenite::accept_with_config(stream, Some(websocket_config()))
        .map_err(|err| anyhow::anyhow!("websocket handshake failed: {err}"))?;

    let (frame, hello) = loop {
        let frame = match ws.read()? {
            Message::Binary(bytes) => Frame::Binary(bytes),
            Message::Text(text) => Frame::Text(text),
            Message::Close(_) => return Ok(None),
            _ => continue,
        };
        let hello = Encoding::decode::<ClientMessage>(&frame);
        break (frame, hello);
    };
    let encoding = frame.encoding();
    let reject = |ws: &mut WebSocket<TcpStream>, reason: String| -> anyhow::Result<()> {
        tracing::info!("Rejected remote client {peer}: {reason}");
        let rejected = encoding.encode(&ServerMessage::Rejected {
            protocol_version: PROTOCOL_VERSION,
            reason,
        })?;
        ws.send(into_message(rejected))?;
        ws.close(None)?;
        let _ = ws.flush();
        Ok(())
    };
    let streams = match hello {
        Ok(ClientMessage::Hello {
            protocol_version,
            streams,
        }) => {
            if protocol_version != PROTOCOL_VERSION {
                reject(
                    &mut ws,
                    format!(
                        "Protocol version {protocol_version} is not supported (the engine speaks {PROTOCOL_VERSION})"
                    ),
                )?;
                return Ok(None);
            }
            streams
        }
        Ok(_) => {
            reject(&mut ws, "The first message must be Hello".to_string())?;
            return Ok(None);
        }
        Err(err) => {
            reject(&mut ws, format!("Could not read Hello: {err}"))?;
            return Ok(None);
        }
    };
    {
        let mut clients = lock(&shared.clients);
        if clients.len() >= shared.options.max_clients {
            drop(clients);
            reject(
                &mut ws,
                format!("Too many clients (at most {})", shared.options.max_clients),
            )?;
            return Ok(None);
        }
        clients.insert(id, ClientSlot::Joining);
    }

    let connection = ClientConnection::connect_remote(
        &shared.hub,
        &shared.bus,
        &shared.app.frontend_ui,
        shared.options.queue_capacity,
    );
    let client = connection.id();
    let welcome = encoding.encode(&ServerMessage::Welcome {
        protocol_version: PROTOCOL_VERSION,
        client,
        session: connection.session(),
        engine_version: env!("CARGO_PKG_VERSION").to_string(),
    })?;
    ws.send(into_message(welcome))?;
    ws.get_mut().set_read_timeout(None)?;
    ws.get_mut().set_write_timeout(None)?;
    ws.get_mut().set_nonblocking(true)?;
    tracing::info!(
        "Remote client {client} connected from {peer} ({encoding}, {})",
        connection.session()
    );

    // The plugin tick hands its show bytes over while this lives.
    let _interest = shared.app.show_snapshots.want();
    let mut session = ClientSession::new(shared, id, ws, encoding, connection, peer, streams);
    let reason = match session.run() {
        Ok(reason) => reason,
        Err(err) => format!("{err:#}"),
    };
    Ok(Some((client, reason)))
}

fn into_message(frame: Frame) -> Message {
    match frame {
        Frame::Binary(bytes) => Message::Binary(bytes),
        Frame::Text(text) => Message::Text(text),
    }
}

/// What happened to a frame handed to the websocket.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Written {
    /// Buffered and the socket keeps up.
    Sent,
    /// Buffered, but the socket is full: stop producing.
    Backlogged,
    /// Not buffered (the write buffer is full): stop producing.
    Dropped,
}

/// One connected client: its socket, its engine connection and what it was
/// last sent.
struct ClientSession<'a> {
    shared: &'a Shared,
    id: u64,
    ws: WebSocket<TcpStream>,
    encoding: Encoding,
    connection: ClientConnection,
    streams: HashSet<Stream>,
    info: ClientInfo,
    /// Since when the socket takes no data, if it doesn't.
    backlogged_since: Option<Instant>,
    /// Messages taken from the hub but not yet written.
    carry: VecDeque<ServerMessage>,
    pending_ui: Option<BTreeSet<u8>>,

    show_digest: Option<u64>,
    session_fingerprint: Vec<u8>,
    stage: Option<(u64, Option<PathBuf>)>,
    dmx: Vec<Option<Vec<u8>>>,
    health: Vec<u8>,
    spectrogram_time: Option<u64>,
    plugin_ui: Vec<u8>,
    editor_ui: Vec<u8>,
    status: Vec<u8>,

    next_show: Instant,
    next_session: Instant,
    next_stage: Instant,
    next_dmx: Instant,
    next_health: Instant,
    next_spectrogram: Instant,
    next_plugin_ui: Instant,
    next_status: Instant,
    next_publish_ui: Instant,
    next_info: Instant,
}

impl<'a> ClientSession<'a> {
    fn new(
        shared: &'a Shared,
        id: u64,
        ws: WebSocket<TcpStream>,
        encoding: Encoding,
        connection: ClientConnection,
        peer: SocketAddr,
        streams: Vec<Stream>,
    ) -> Self {
        let now = Instant::now();
        let info = ClientInfo {
            client: connection.id(),
            session: connection.session(),
            peer,
            encoding,
            backlogged: false,
            queued_system: 0,
            sent_frames: 0,
            sent_bytes: 0,
            show_frames: 0,
            show_bytes: 0,
            dropped_frames: 0,
        };
        lock(&shared.clients).insert(id, ClientSlot::Connected(info.clone()));
        Self {
            shared,
            id,
            ws,
            encoding,
            connection,
            streams: streams.into_iter().collect(),
            info,
            backlogged_since: None,
            carry: VecDeque::new(),
            pending_ui: None,
            show_digest: None,
            session_fingerprint: Vec::new(),
            stage: None,
            dmx: vec![None; NUM_DMX_UNIVERSES],
            health: Vec::new(),
            spectrogram_time: None,
            plugin_ui: Vec::new(),
            editor_ui: Vec::new(),
            status: Vec::new(),
            next_show: now,
            next_session: now,
            next_stage: now,
            next_dmx: now,
            next_health: now,
            next_spectrogram: now,
            next_plugin_ui: now,
            next_status: now,
            next_publish_ui: now,
            next_info: now,
        }
    }

    /// Serves the client until it disconnects, stalls or the server stops.
    /// Returns why it ended.
    fn run(&mut self) -> anyhow::Result<String> {
        loop {
            if self.shared.stop.load(Ordering::Relaxed) {
                let _ = self.ws.close(Some(CloseFrame {
                    code: CloseCode::Away,
                    reason: "The engine is stopping".into(),
                }));
                let _ = self.ws.flush();
                return Ok("the server stopped".to_string());
            }
            let now = Instant::now();
            let mut busy = match self.receive()? {
                Some(received) => received,
                None => return Ok("closed by the client".to_string()),
            };
            self.publish_ui(now);

            if self.backlogged_since.is_none() {
                busy |= self.produce(now)?;
            } else if !self.streams.contains(&Stream::Bus) {
                // Nobody reads it; keep the bus queue empty.
                while self.connection.try_recv_bus().is_some() {}
            }

            match self.ws.flush() {
                Ok(()) => self.backlogged_since = None,
                Err(tungstenite::Error::Io(err)) if err.kind() == ErrorKind::WouldBlock => {
                    self.backlogged_since.get_or_insert(now);
                }
                Err(err) => return Ok(format!("connection lost ({err})")),
            }
            if let Some(since) = self.backlogged_since {
                if now.duration_since(since) >= self.shared.options.stall_timeout {
                    return Ok(format!(
                        "took no data for {:.0?} (stalled)",
                        self.shared.options.stall_timeout
                    ));
                }
            }
            if now >= self.next_info {
                self.next_info = now + INFO_INTERVAL;
                self.update_info();
            }
            if !busy {
                thread::sleep(IDLE_SLEEP);
            }
        }
    }

    fn update_info(&mut self) {
        self.info.backlogged = self.backlogged_since.is_some();
        self.info.queued_system = self.connection.queued_system();
        if let Some(slot) = lock(&self.shared.clients).get_mut(&self.id) {
            *slot = ClientSlot::Connected(self.info.clone());
        }
    }

    /// Reads and applies every message the client sent. `Some(true)` if
    /// there was one, `None` once the client closed the connection.
    fn receive(&mut self) -> anyhow::Result<Option<bool>> {
        let mut received = false;
        loop {
            let frame = match self.ws.read() {
                Ok(Message::Binary(bytes)) => Frame::Binary(bytes),
                Ok(Message::Text(text)) => Frame::Text(text),
                Ok(Message::Close(_)) => {
                    let _ = self.ws.flush();
                    return Ok(None);
                }
                Ok(_) => continue,
                Err(tungstenite::Error::Io(err)) if err.kind() == ErrorKind::WouldBlock => {
                    return Ok(Some(received));
                }
                Err(
                    tungstenite::Error::ConnectionClosed
                    | tungstenite::Error::AlreadyClosed
                    | tungstenite::Error::Protocol(_),
                ) => return Ok(None),
                Err(err) => return Err(err.into()),
            };
            received = true;
            match Encoding::decode::<ClientMessage>(&frame) {
                Ok(message) => self.apply(message),
                Err(err) => tracing::debug!(
                    "Ignoring a message from {} that could not be decoded: {err:#}",
                    self.info.client
                ),
            }
        }
    }

    fn apply(&mut self, message: ClientMessage) {
        match message {
            ClientMessage::Hello { .. } => {}
            ClientMessage::Command { seq, body } => {
                let body = match body {
                    // A remote client is a frontend: its events are `Web`'s,
                    // whatever it claims (the engine skips `DmxEngine` events
                    // as rollbacks; the desk follows `Plugin` UI requests).
                    BusBody::Control(message) => BusBody::Control(ControlEventMessage::new(
                        EventOriginator::Web,
                        message.body(),
                    )),
                    body => body,
                };
                self.connection.send_with_seq(seq, body);
            }
            ClientMessage::Subscribe { streams } => {
                let streams: HashSet<Stream> = streams.into_iter().collect();
                let added: Vec<Stream> = streams.difference(&self.streams).copied().collect();
                self.streams = streams;
                for stream in added {
                    self.forget(stream);
                }
            }
            ClientMessage::PublishUi { open_plugins } => self.pending_ui = Some(open_plugins),
        }
    }

    /// Forgets what `stream` last sent, so the next update is complete.
    fn forget(&mut self, stream: Stream) {
        let now = Instant::now();
        match stream {
            Stream::Dmx => {
                self.dmx = vec![None; NUM_DMX_UNIVERSES];
                self.next_dmx = now;
            }
            Stream::Health => {
                self.health.clear();
                self.next_health = now;
            }
            Stream::Spectrogram => {
                self.spectrogram_time = None;
                self.next_spectrogram = now;
            }
            Stream::PluginUi => {
                self.plugin_ui.clear();
                self.next_plugin_ui = now;
            }
            Stream::AnimationEditorUi => {
                self.editor_ui.clear();
                self.next_plugin_ui = now;
            }
            Stream::Status => {
                self.status.clear();
                self.next_status = now;
            }
            Stream::Bus => {
                // Only what happens from now on.
                while self.connection.try_recv_bus().is_some() {}
            }
        }
    }

    /// Hands the client's reported view state to the engine's mirror, at
    /// most every [`PUBLISH_UI_INTERVAL`].
    fn publish_ui(&mut self, now: Instant) {
        if now < self.next_publish_ui {
            return;
        }
        if let Some(open_plugins) = self.pending_ui.take() {
            self.next_publish_ui = now + PUBLISH_UI_INTERVAL;
            // External screens are the desk's alone.
            self.connection.publish_ui(FrontendUiSnapshot {
                open_plugins,
                external_screens: Vec::new(),
            });
        }
    }

    /// Writes one frame. Every count is updated unless it was dropped.
    fn write(&mut self, frame: Frame) -> anyhow::Result<Written> {
        let len = frame.len() as u64;
        let written = match self.ws.write(into_message(frame)) {
            Ok(()) => Written::Sent,
            Err(tungstenite::Error::Io(err)) if err.kind() == ErrorKind::WouldBlock => {
                Written::Backlogged
            }
            Err(tungstenite::Error::WriteBufferFull(_)) => Written::Dropped,
            Err(err) => return Err(err.into()),
        };
        match written {
            Written::Dropped => {
                self.info.dropped_frames += 1;
                self.backlogged_since.get_or_insert_with(Instant::now);
            }
            Written::Backlogged => {
                self.info.sent_frames += 1;
                self.info.sent_bytes += len;
                self.backlogged_since.get_or_insert_with(Instant::now);
            }
            Written::Sent => {
                self.info.sent_frames += 1;
                self.info.sent_bytes += len;
            }
        }
        Ok(written)
    }

    fn send(&mut self, message: &ServerMessage) -> anyhow::Result<Written> {
        let frame = self.encoding.encode(message)?;
        self.write(frame)
    }

    /// Writes what changed, until the socket backs up. Returns whether
    /// anything was written.
    fn produce(&mut self, now: Instant) -> anyhow::Result<bool> {
        let sent_before = self.info.sent_frames;
        let _ = self.produce_until_backlogged(now)?;
        Ok(self.info.sent_frames != sent_before)
    }

    /// `Ok(false)` as soon as a write didn't go through.
    fn produce_until_backlogged(&mut self, now: Instant) -> anyhow::Result<bool> {
        // System messages and results. A result's effect on the client's
        // session is sent before the result itself.
        let mut had_result = false;
        while self.carry.len() < MESSAGES_PER_ROUND {
            let Some(message) = self.connection.try_recv_system() else {
                break;
            };
            match message {
                SystemMessage::CommandResult(result) => {
                    had_result = true;
                    self.carry.push_back(ServerMessage::Result(result));
                }
                message => {
                    if let Some(message) = WireSystemMessage::from_system(&message) {
                        self.carry.push_back(ServerMessage::System(message));
                    }
                }
            }
        }
        if had_result || now >= self.next_session {
            self.next_session = now + SESSION_INTERVAL;
            if !self.send_session()? {
                return Ok(false);
            }
        }
        while let Some(message) = self.carry.pop_front() {
            if self.send(&message)? != Written::Sent {
                return Ok(false);
            }
        }

        if self.streams.contains(&Stream::Bus) {
            for _ in 0..MESSAGES_PER_ROUND {
                let Some(message) = self.connection.try_recv_bus() else {
                    break;
                };
                if self.send(&ServerMessage::Bus(message))? != Written::Sent {
                    return Ok(false);
                }
            }
        } else {
            while self.connection.try_recv_bus().is_some() {}
        }

        if now >= self.next_show {
            self.next_show = now + SHOW_INTERVAL;
            if !self.send_show()? {
                return Ok(false);
            }
        }
        if now >= self.next_stage {
            self.next_stage = now + STAGE_INTERVAL;
            if !self.send_stage()? {
                return Ok(false);
            }
        }
        if self.streams.contains(&Stream::Dmx) && now >= self.next_dmx {
            self.next_dmx = now + DMX_INTERVAL;
            if !self.send_dmx()? {
                return Ok(false);
            }
        }
        if self.streams.contains(&Stream::Health) && now >= self.next_health {
            self.next_health = now + HEALTH_INTERVAL;
            let health = Box::new(read(&self.shared.app.health_data).clone());
            if !self.send_if_changed(Field::Health, ServerMessage::Health(health))? {
                return Ok(false);
            }
        }
        if self.streams.contains(&Stream::Spectrogram) && now >= self.next_spectrogram {
            self.next_spectrogram = now + SPECTROGRAM_INTERVAL;
            if !self.send_spectrogram()? {
                return Ok(false);
            }
        }
        if now >= self.next_plugin_ui {
            self.next_plugin_ui = now + PLUGIN_UI_INTERVAL;
            if self.streams.contains(&Stream::PluginUi) {
                let ops = read(&self.shared.app.plugin_ui_ops).clone();
                if !self.send_if_changed(Field::PluginUi, ServerMessage::PluginUi(ops))? {
                    return Ok(false);
                }
            }
            if self.streams.contains(&Stream::AnimationEditorUi) {
                let ops = read(&self.shared.app.wasm_animation_ui_ops).clone();
                let message = ServerMessage::AnimationEditorUi(ops);
                if !self.send_if_changed(Field::EditorUi, message)? {
                    return Ok(false);
                }
            }
        }
        if self.streams.contains(&Stream::Status) && now >= self.next_status {
            self.next_status = now + STATUS_INTERVAL;
            let status = Box::new(engine_status(&self.shared.app));
            if !self.send_if_changed(Field::Status, ServerMessage::Status(status))? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// The client's session, if it changed. `Ok(false)` if the write didn't
    /// go through.
    fn send_session(&mut self) -> anyhow::Result<bool> {
        let session = self.connection.session();
        let snapshot = {
            let show = read(&self.shared.app.dmx_engine);
            show.session(session).map(|me| SessionSnapshot {
                session,
                state: me.to_session(),
                live_owner: show.live_owner(),
            })
        };
        // Not open yet: the engine opens it on its next tick.
        let Some(snapshot) = snapshot else {
            return Ok(true);
        };
        let fingerprint = fingerprint(&snapshot);
        if fingerprint == self.session_fingerprint {
            return Ok(true);
        }
        let written = self.send(&ServerMessage::Session(snapshot))?;
        if written != Written::Dropped {
            self.session_fingerprint = fingerprint;
        }
        Ok(written == Written::Sent)
    }

    fn send_show(&mut self) -> anyhow::Result<bool> {
        let show = self
            .shared
            .app
            .show_snapshots
            .latest(&self.shared.app.dmx_engine);
        if self.show_digest == Some(show.digest) {
            return Ok(true);
        }
        let frame = self.shared.show_frame(self.encoding, &show)?;
        let len = frame.len() as u64;
        let written = self.write(Frame::clone(&frame))?;
        if written != Written::Dropped {
            self.show_digest = Some(show.digest);
            self.info.show_frames += 1;
            self.info.show_bytes += len;
        }
        Ok(written == Written::Sent)
    }

    fn send_stage(&mut self) -> anyhow::Result<bool> {
        let snapshot = {
            let show = read(&self.shared.app.dmx_engine);
            let key = (show.stage_revision(), show.showfile().map(PathBuf::from));
            if self.stage.as_ref() == Some(&key) {
                return Ok(true);
            }
            StageSnapshot {
                revision: key.0,
                showfile: key.1,
                scene: show.stage().clone(),
            }
        };
        let key = (snapshot.revision, snapshot.showfile.clone());
        let written = self.send(&ServerMessage::Stage(Box::new(snapshot)))?;
        if written != Written::Dropped {
            self.stage = Some(key);
        }
        Ok(written == Written::Sent)
    }

    fn send_dmx(&mut self) -> anyhow::Result<bool> {
        let mut changed = Vec::new();
        for (universe, buffer) in self.shared.app.dmx_universes.iter().enumerate() {
            let buffer = read(buffer);
            if self.dmx[universe].as_deref() != Some(&buffer.dmx_buffer[..]) {
                changed.push(DmxUniverse {
                    universe: universe as u8,
                    channels: buffer.dmx_buffer.to_vec(),
                });
            }
        }
        if changed.is_empty() {
            return Ok(true);
        }
        let written = self.send(&ServerMessage::Dmx(changed.clone()))?;
        if written != Written::Dropped {
            for universe in changed {
                self.dmx[universe.universe as usize] = Some(universe.channels);
            }
        }
        Ok(written == Written::Sent)
    }

    fn send_spectrogram(&mut self) -> anyhow::Result<bool> {
        let update = {
            let spectrogram = read(&self.shared.app.audio_spectrogram);
            let newest = spectrogram
                .columns
                .back()
                .map(|column| column.snapshot.time);
            let reset = match (self.spectrogram_time, newest) {
                (_, None) => return Ok(true),
                (None, Some(_)) => true,
                (Some(last), Some(newest)) if newest == last => return Ok(true),
                // The spectrogram started over.
                (Some(last), Some(newest)) => newest < last,
            };
            let columns = spectrogram
                .columns
                .iter()
                .filter(|column| reset || Some(column.snapshot.time) > self.spectrogram_time)
                .map(|column| SpectrogramColumn {
                    snapshot: column.snapshot.clone(),
                    buckets: column.current_audio_colunn.clone(),
                })
                .collect();
            SpectrogramUpdate {
                reset,
                max_columns: spectrogram.max_columns as u32,
                bin_count: spectrogram.bin_count as u32,
                tick_period_ms: spectrogram.tick_period.as_millis() as u32,
                columns,
            }
        };
        let newest = update.columns.last().map(|column| column.snapshot.time);
        let written = self.send(&ServerMessage::Spectrogram(update))?;
        if written != Written::Dropped {
            self.spectrogram_time = newest.or(self.spectrogram_time);
        }
        Ok(written == Written::Sent)
    }

    /// What `field` last sent.
    fn last_sent(&mut self, field: Field) -> &mut Vec<u8> {
        match field {
            Field::Health => &mut self.health,
            Field::PluginUi => &mut self.plugin_ui,
            Field::EditorUi => &mut self.editor_ui,
            Field::Status => &mut self.status,
        }
    }

    /// Sends `message` if its payload differs from what `field` last sent.
    fn send_if_changed(&mut self, field: Field, message: ServerMessage) -> anyhow::Result<bool> {
        let fingerprint = fingerprint(&message);
        if *self.last_sent(field) == fingerprint {
            return Ok(true);
        }
        let written = self.send(&message)?;
        if written != Written::Dropped {
            *self.last_sent(field) = fingerprint;
        }
        Ok(written == Written::Sent)
    }
}

/// The streams whose changes are found by comparing fingerprints.
#[derive(Clone, Copy)]
enum Field {
    Health,
    PluginUi,
    EditorUi,
    Status,
}

fn engine_status(app: &AppState) -> EngineStatus {
    EngineStatus {
        grand_master_percent: app.grand_master_percent(),
        mainloop_state: *read(&app.mainloop_state),
        audio_params: *read(&app.audio_params),
        artnet_receivers: read(&app.artnet_output).receivers.clone(),
        plugins: read(&app.plugins).clone(),
        plugin_runtime_kinds: read(&app.plugin_runtime_kinds).clone(),
    }
}
