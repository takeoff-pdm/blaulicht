//! [`RemoteClient`]: the [`EngineClient`] for an engine in another process,
//! over the websocket protocol ([`crate::server::protocol`], bincode).
//!
//! An I/O thread owns the socket. It applies what the engine sends to a copy
//! of the engine state (an [`EngineState`] built from the show, stage and
//! session snapshots) and to the latest value of each stream, and writes the
//! client's commands. Readers run their closures on that copy, under a read
//! lock that only the I/O thread ever writes, so a frontend reads a remote
//! engine exactly as it reads a local one; the state lags the engine by up
//! to a show snapshot interval.
//!
//! Commands get their results as with a local client. When the connection
//! drops, every command still waiting (and every command sent afterwards)
//! gets `Refused(DISCONNECTED)`, so [`super::ReplyHandlers`] never keeps a
//! closure forever.

use std::{
    collections::{BTreeMap, BTreeSet, HashMap, VecDeque},
    io::ErrorKind,
    net::{TcpStream, ToSocketAddrs},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex, MutexGuard, PoisonError, RwLock, RwLockReadGuard,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use anyhow::Context;
use blaulicht_audio_engine::{AudioSpectrogram, CollectorOutput, SignalCollectorParams};
use crossbeam_channel::{Receiver, Sender};
use tungstenite::{Message, WebSocket};

use super::EngineClient;
use crate::{
    audio::defs::AudioThreadControlSignal,
    dmx::EngineState,
    frontend_ui::FrontendUiSnapshot,
    hub::{
        BusBody, BusMessage, ClientId, CommandOutcome, CommandResult, CommandSeq, SessionId,
        SUBSCRIBER_CAPACITY,
    },
    msg::{SystemMessage, TERMINAL_ONLY_LOG_TARGET},
    server::protocol::{
        ClientMessage, Encoding, EngineStatus, Frame, ServerMessage, SpectrogramUpdate, Stream,
        PROTOCOL_VERSION,
    },
    state::{AppHealthState, ArtNetReceiver, PluginRuntimeKind, PluginState, NUM_DMX_UNIVERSES},
    ui_ops::WasmUiOp,
};

/// Why a command fails when the connection to the engine is gone.
pub const DISCONNECTED: &str = "Disconnected from the engine";

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);
/// How long the I/O thread sleeps when there was nothing to do.
const IDLE_SLEEP: Duration = Duration::from_millis(2);

/// What a [`RemoteClient`] received so far, for diagnostics.
#[derive(Clone, Debug, Default)]
pub struct RemoteStats {
    pub frames: u64,
    pub bytes: u64,
    /// Frames and bytes by message kind (`"Show"`, `"Session"`, `"Dmx"`, ...).
    pub by_kind: BTreeMap<&'static str, (u64, u64)>,
    /// The size of the last show snapshot.
    pub last_show_bytes: usize,
}

/// Messages and results waiting for the frontend.
#[derive(Default)]
struct Inbox {
    system: VecDeque<SystemMessage>,
    results: Vec<CommandResult>,
    bus: VecDeque<BusMessage>,
    /// Commands sent whose result hasn't arrived.
    pending: BTreeSet<CommandSeq>,
    overflowed: bool,
}

impl Inbox {
    /// Keeps `message` in `queue`, dropping the oldest beyond
    /// [`SUBSCRIBER_CAPACITY`] (a frontend that never reads a stream would
    /// grow it without bound otherwise).
    fn keep<T>(queue: &mut VecDeque<T>, overflowed: &mut bool, message: T) {
        if queue.len() >= SUBSCRIBER_CAPACITY {
            queue.pop_front();
            if !std::mem::replace(overflowed, true) {
                tracing::warn!(
                    target: TERMINAL_ONLY_LOG_TARGET,
                    "Remote client: messages are not read; dropping the oldest (logged once)"
                );
            }
        }
        queue.push_back(message);
    }
}

/// What the I/O thread writes and the frontend reads.
struct RemoteState {
    client: ClientId,
    show: RwLock<EngineState>,
    show_digest: Mutex<Option<u64>>,
    session_received: AtomicBool,
    inbox: Mutex<Inbox>,
    dmx: RwLock<Vec<[u8; 513]>>,
    health: RwLock<AppHealthState>,
    spectrogram: RwLock<AudioSpectrogram>,
    plugin_ui_ops: RwLock<HashMap<u8, Vec<WasmUiOp>>>,
    editor_ui_ops: RwLock<HashMap<u64, Vec<WasmUiOp>>>,
    status: RwLock<EngineStatus>,
    connected: AtomicBool,
    stop: AtomicBool,
    stats: Mutex<RemoteStats>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn read<T>(lock: &RwLock<T>) -> RwLockReadGuard<'_, T> {
    lock.read().unwrap_or_else(PoisonError::into_inner)
}

fn write<T>(lock: &RwLock<T>) -> std::sync::RwLockWriteGuard<'_, T> {
    lock.write().unwrap_or_else(PoisonError::into_inner)
}

impl RemoteState {
    fn new(client: ClientId) -> Self {
        Self {
            client,
            show: RwLock::new(EngineState::default()),
            show_digest: Mutex::new(None),
            session_received: AtomicBool::new(false),
            inbox: Mutex::default(),
            dmx: RwLock::new(vec![[0; 513]; NUM_DMX_UNIVERSES]),
            health: RwLock::new(AppHealthState::default()),
            spectrogram: RwLock::new(AudioSpectrogram::new(40, 128, Duration::from_millis(20))),
            plugin_ui_ops: RwLock::default(),
            editor_ui_ops: RwLock::default(),
            status: RwLock::default(),
            connected: AtomicBool::new(true),
            stop: AtomicBool::new(false),
            stats: Mutex::default(),
        }
    }

    fn refused(&self, seq: CommandSeq) -> CommandResult {
        CommandResult {
            client: self.client,
            seq,
            outcome: CommandOutcome::Refused(DISCONNECTED.to_string()),
        }
    }

    /// The connection is gone: every waiting command fails.
    fn disconnect(&self, reason: &str) {
        self.connected.store(false, Ordering::SeqCst);
        let mut inbox = lock(&self.inbox);
        let pending = std::mem::take(&mut inbox.pending);
        let results: Vec<_> = pending.into_iter().map(|seq| self.refused(seq)).collect();
        inbox.results.extend(results);
        drop(inbox);
        tracing::info!("Remote client {}: disconnected ({reason})", self.client);
    }

    fn count(&self, kind: &'static str, bytes: usize) {
        let mut stats = lock(&self.stats);
        stats.frames += 1;
        stats.bytes += bytes as u64;
        let entry = stats.by_kind.entry(kind).or_default();
        entry.0 += 1;
        entry.1 += bytes as u64;
        if kind == "Show" {
            stats.last_show_bytes = bytes;
        }
    }

    fn apply(&self, message: ServerMessage) {
        match message {
            ServerMessage::Welcome { .. } | ServerMessage::Rejected { .. } => {}
            ServerMessage::Show(snapshot) => match snapshot.state.into_state() {
                Ok(state) => {
                    write(&self.show).set_mirrored_show(state);
                    *lock(&self.show_digest) = Some(snapshot.digest);
                }
                Err(err) => tracing::warn!("Remote client: bad show snapshot: {err:#}"),
            },
            ServerMessage::Stage(stage) => {
                let stage = *stage;
                write(&self.show).set_mirrored_stage(stage.scene, stage.revision, stage.showfile)
            }
            ServerMessage::Session(session) => {
                write(&self.show).set_mirrored_session(
                    session.session,
                    session.state,
                    session.live_owner,
                );
                self.session_received.store(true, Ordering::Relaxed);
            }
            ServerMessage::System(message) => {
                let mut inbox = lock(&self.inbox);
                let Inbox {
                    system, overflowed, ..
                } = &mut *inbox;
                Inbox::keep(system, overflowed, message.into_system());
            }
            ServerMessage::Result(result) => {
                let mut inbox = lock(&self.inbox);
                inbox.pending.remove(&result.seq);
                inbox.results.push(result);
            }
            ServerMessage::Bus(message) => {
                let mut inbox = lock(&self.inbox);
                let Inbox {
                    bus, overflowed, ..
                } = &mut *inbox;
                Inbox::keep(bus, overflowed, message);
            }
            ServerMessage::Dmx(universes) => {
                let mut dmx = write(&self.dmx);
                for universe in universes {
                    if let Some(buffer) = dmx.get_mut(universe.universe as usize) {
                        let len = universe.channels.len().min(buffer.len());
                        buffer[..len].copy_from_slice(&universe.channels[..len]);
                    }
                }
            }
            ServerMessage::Health(health) => *write(&self.health) = *health,
            ServerMessage::Spectrogram(update) => apply_spectrogram(&self.spectrogram, update),
            ServerMessage::PluginUi(ops) => *write(&self.plugin_ui_ops) = ops,
            ServerMessage::AnimationEditorUi(ops) => *write(&self.editor_ui_ops) = ops,
            ServerMessage::Status(status) => *write(&self.status) = *status,
        }
    }
}

fn apply_spectrogram(spectrogram: &RwLock<AudioSpectrogram>, update: SpectrogramUpdate) {
    let mut spectrogram = write(spectrogram);
    if update.reset {
        spectrogram.columns.clear();
    }
    spectrogram.max_columns = update.max_columns as usize;
    spectrogram.bin_count = update.bin_count as usize;
    spectrogram.tick_period = Duration::from_millis(update.tick_period_ms.into());
    for column in update.columns {
        spectrogram.push_data(CollectorOutput {
            snapshot: column.snapshot,
            current_audio_colunn: column.buckets,
            ..CollectorOutput::default()
        });
    }
}

fn kind(message: &ServerMessage) -> &'static str {
    match message {
        ServerMessage::Welcome { .. } => "Welcome",
        ServerMessage::Rejected { .. } => "Rejected",
        ServerMessage::Show(_) => "Show",
        ServerMessage::Stage(_) => "Stage",
        ServerMessage::Session(_) => "Session",
        ServerMessage::System(_) => "System",
        ServerMessage::Result(_) => "Result",
        ServerMessage::Bus(_) => "Bus",
        ServerMessage::Dmx(_) => "Dmx",
        ServerMessage::Health(_) => "Health",
        ServerMessage::Spectrogram(_) => "Spectrogram",
        ServerMessage::PluginUi(_) => "PluginUi",
        ServerMessage::AnimationEditorUi(_) => "AnimationEditorUi",
        ServerMessage::Status(_) => "Status",
    }
}

fn into_message(frame: Frame) -> Message {
    match frame {
        Frame::Binary(bytes) => Message::Binary(bytes),
        Frame::Text(text) => Message::Text(text),
    }
}

/// An [`EngineClient`] for an engine in another process (or this one, over
/// loopback), connected with [`RemoteClient::connect`].
///
/// Before the first snapshots arrive, the show is empty and
/// [`EngineClient::with_session`] is `None`; [`Self::wait_for_snapshot`]
/// waits for them. Dropping the client closes the connection, which closes
/// its session on the engine.
pub struct RemoteClient {
    client: ClientId,
    session: SessionId,
    engine_version: String,
    next_seq: AtomicU64,
    outgoing: Sender<ClientMessage>,
    state: Arc<RemoteState>,
    last_ui: Option<BTreeSet<u8>>,
    io: Option<JoinHandle<()>>,
}

impl RemoteClient {
    /// Connects to the engine's server at `addr` (`"host:port"`), in a new
    /// operator session, receiving `streams` (see [`Stream`]). Fails if the
    /// engine can't be reached or rejects the client (for example another
    /// [`PROTOCOL_VERSION`]).
    pub fn connect(addr: impl ToSocketAddrs, streams: &[Stream]) -> anyhow::Result<Self> {
        let addr = addr
            .to_socket_addrs()?
            .next()
            .context("No address to connect to")?;
        let stream = TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT)
            .with_context(|| format!("Could not connect to {addr}"))?;
        stream.set_nodelay(true)?;
        stream.set_read_timeout(Some(HANDSHAKE_TIMEOUT))?;
        stream.set_write_timeout(Some(HANDSHAKE_TIMEOUT))?;
        let url = format!("ws://{addr}/");
        let (mut ws, _) = tungstenite::client::client_with_config(
            url.as_str(),
            stream,
            Some(crate::server::websocket_config()),
        )
        .map_err(|err| anyhow::anyhow!("Websocket handshake with {addr} failed: {err}"))?;

        let hello = Encoding::Bincode.encode(&ClientMessage::Hello {
            protocol_version: PROTOCOL_VERSION,
            streams: streams.to_vec(),
        })?;
        ws.send(into_message(hello))?;
        let welcome = loop {
            match ws.read()? {
                Message::Binary(bytes) => {
                    break Encoding::decode::<ServerMessage>(&Frame::Binary(bytes))?
                }
                Message::Close(_) => anyhow::bail!("The engine closed the connection"),
                _ => continue,
            }
        };
        let (client, session, engine_version) = match welcome {
            ServerMessage::Welcome {
                client,
                session,
                engine_version,
                ..
            } => (client, session, engine_version),
            ServerMessage::Rejected { reason, .. } => {
                anyhow::bail!("The engine rejected the connection: {reason}")
            }
            _ => anyhow::bail!("The engine did not answer Hello"),
        };
        ws.get_mut().set_read_timeout(None)?;
        ws.get_mut().set_write_timeout(None)?;
        ws.get_mut().set_nonblocking(true)?;

        let state = Arc::new(RemoteState::new(client));
        let (outgoing, receiver) = crossbeam_channel::unbounded();
        let io = {
            let state = Arc::clone(&state);
            thread::Builder::new()
                .name(format!("remote-client-{client}"))
                .spawn(move || io_loop(ws, &state, &receiver))?
        };
        Ok(Self {
            client,
            session,
            engine_version,
            next_seq: AtomicU64::new(1),
            outgoing,
            state,
            last_ui: None,
            io: Some(io),
        })
    }

    /// The engine's version (`CARGO_PKG_VERSION` of its build).
    pub fn engine_version(&self) -> &str {
        &self.engine_version
    }

    /// `false` once the connection is gone (it is not reopened).
    pub fn is_connected(&self) -> bool {
        self.state.connected.load(Ordering::SeqCst)
    }

    /// Replaces the streams this client receives.
    pub fn subscribe(&self, streams: &[Stream]) {
        let _ = self.outgoing.send(ClientMessage::Subscribe {
            streams: streams.to_vec(),
        });
    }

    /// The digest of the last show snapshot, if one arrived.
    pub fn show_digest(&self) -> Option<u64> {
        *lock(&self.state.show_digest)
    }

    /// Waits until the first show and session snapshots arrived (`true`), or
    /// `timeout` passed or the connection dropped (`false`).
    pub fn wait_for_snapshot(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        loop {
            if self.show_digest().is_some() && self.state.session_received.load(Ordering::Relaxed) {
                return true;
            }
            if Instant::now() >= deadline || !self.is_connected() {
                return false;
            }
            thread::sleep(Duration::from_millis(2));
        }
    }

    /// What arrived so far.
    pub fn stats(&self) -> RemoteStats {
        lock(&self.state.stats).clone()
    }
}

impl Drop for RemoteClient {
    fn drop(&mut self) {
        self.state.stop.store(true, Ordering::SeqCst);
        if let Some(io) = self.io.take() {
            let _ = io.join();
        }
    }
}

fn io_loop(mut ws: WebSocket<TcpStream>, state: &RemoteState, outgoing: &Receiver<ClientMessage>) {
    let reason = loop {
        if state.stop.load(Ordering::SeqCst) {
            let _ = ws.close(None);
            let _ = ws.flush();
            break "closed by the client".to_string();
        }
        let mut busy = false;
        let mut ended = None;
        loop {
            match ws.read() {
                Ok(Message::Binary(bytes)) => {
                    busy = true;
                    let len = bytes.len();
                    match Encoding::decode::<ServerMessage>(&Frame::Binary(bytes)) {
                        Ok(message) => {
                            state.count(kind(&message), len);
                            state.apply(message);
                        }
                        Err(err) => tracing::warn!("Remote client: bad message: {err:#}"),
                    }
                }
                Ok(Message::Close(_)) => {
                    let _ = ws.flush();
                    ended = Some("closed by the engine".to_string());
                    break;
                }
                Ok(_) => {}
                Err(tungstenite::Error::Io(err)) if err.kind() == ErrorKind::WouldBlock => break,
                Err(err) => {
                    ended = Some(format!("connection lost ({err})"));
                    break;
                }
            }
        }
        if let Some(reason) = ended {
            break reason;
        }
        while let Ok(message) = outgoing.try_recv() {
            busy = true;
            let frame = match Encoding::Bincode.encode(&message) {
                Ok(frame) => frame,
                Err(err) => {
                    tracing::warn!("Remote client: could not encode a message: {err:#}");
                    continue;
                }
            };
            match ws.write(into_message(frame)) {
                Ok(()) => {}
                Err(tungstenite::Error::Io(err)) if err.kind() == ErrorKind::WouldBlock => {}
                Err(err) => {
                    ended = Some(format!("connection lost ({err})"));
                    break;
                }
            }
        }
        if let Some(reason) = ended {
            break reason;
        }
        match ws.flush() {
            Ok(()) => {}
            Err(tungstenite::Error::Io(err)) if err.kind() == ErrorKind::WouldBlock => {}
            Err(err) => break format!("connection lost ({err})"),
        }
        if !busy {
            thread::sleep(IDLE_SLEEP);
        }
    };
    state.disconnect(&reason);
}

impl EngineClient for RemoteClient {
    fn client(&self) -> ClientId {
        self.client
    }

    fn session(&self) -> SessionId {
        self.session
    }

    fn with_show<R>(&self, f: impl FnOnce(&EngineState) -> R) -> R {
        f(&read(&self.state.show))
    }

    fn send(&self, body: impl Into<BusBody>) -> CommandSeq {
        let seq = self.next_seq.fetch_add(1, Ordering::Relaxed);
        let body = body.into();
        // Under the inbox lock, so a disconnect either sees this command as
        // pending or this sees the disconnect.
        let mut inbox = lock(&self.state.inbox);
        let sent = self.state.connected.load(Ordering::SeqCst)
            && self
                .outgoing
                .send(ClientMessage::Command { seq, body })
                .is_ok();
        if sent {
            inbox.pending.insert(seq);
        } else {
            let refused = self.state.refused(seq);
            inbox.results.push(refused);
        }
        seq
    }

    fn take_results(&self) -> Vec<CommandResult> {
        std::mem::take(&mut lock(&self.state.inbox).results)
    }

    fn try_recv_system(&self) -> Option<SystemMessage> {
        lock(&self.state.inbox).system.pop_front()
    }

    fn try_recv_bus(&self) -> Option<BusMessage> {
        lock(&self.state.inbox).bus.pop_front()
    }

    fn with_dmx_universe<R>(&self, universe: usize, f: impl FnOnce(&[u8; 513]) -> R) -> Option<R> {
        read(&self.state.dmx).get(universe).map(f)
    }

    fn with_health<R>(&self, f: impl FnOnce(&AppHealthState) -> R) -> R {
        f(&read(&self.state.health))
    }

    fn with_spectrogram<R>(&self, f: impl FnOnce(&AudioSpectrogram) -> R) -> R {
        f(&read(&self.state.spectrogram))
    }

    fn with_plugin_ui_ops<R>(&self, f: impl FnOnce(&HashMap<u8, Vec<WasmUiOp>>) -> R) -> R {
        f(&read(&self.state.plugin_ui_ops))
    }

    fn with_audio_params<R>(&self, f: impl FnOnce(&SignalCollectorParams) -> R) -> R {
        f(&read(&self.state.status).audio_params)
    }

    fn with_artnet_receivers<R>(&self, f: impl FnOnce(&[ArtNetReceiver]) -> R) -> R {
        f(&read(&self.state.status).artnet_receivers)
    }

    fn with_plugins<R>(&self, f: impl FnOnce(&HashMap<u8, PluginState>) -> R) -> R {
        f(&read(&self.state.status).plugins)
    }

    fn grand_master_percent(&self) -> u8 {
        read(&self.state.status).grand_master_percent
    }

    fn mainloop_state(&self) -> AudioThreadControlSignal {
        read(&self.state.status).mainloop_state
    }

    fn with_plugin_runtime_kinds<R>(
        &self,
        f: impl FnOnce(&HashMap<u8, PluginRuntimeKind>) -> R,
    ) -> R {
        f(&read(&self.state.status).plugin_runtime_kinds)
    }

    fn with_animation_editor_ui_ops<R>(
        &self,
        f: impl FnOnce(&HashMap<u64, Vec<WasmUiOp>>) -> R,
    ) -> R {
        f(&read(&self.state.editor_ui_ops))
    }

    /// Reports the open plugin UIs (external screens are the desk's alone).
    fn publish_ui(&mut self, snapshot: FrontendUiSnapshot) {
        if self.last_ui.as_ref() == Some(&snapshot.open_plugins) {
            return;
        }
        let _ = self.outgoing.send(ClientMessage::PublishUi {
            open_plugins: snapshot.open_plugins.clone(),
        });
        self.last_ui = Some(snapshot.open_plugins);
    }
}
