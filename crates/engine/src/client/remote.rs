//! [`RemoteClient`]: the [`EngineClient`] for an engine in another process,
//! over the websocket protocol ([`crate::server::protocol`], bincode).
//!
//! What the engine sends is applied to a copy of the engine state (an
//! [`EngineState`] built from the show, stage and session snapshots) and to
//! the latest value of each stream. Readers run their closures on that copy,
//! under a read lock that only the transport ever writes, so a frontend reads
//! a remote engine exactly as it reads a local one; the state lags the engine
//! by up to a show snapshot interval.
//!
//! Two transports share that state (`RemoteState`):
//! - **native** ([`RemoteClient::connect`]): a TCP socket with `tungstenite`,
//!   owned by an I/O thread. Connecting blocks until the engine's `Welcome`.
//! - **browser** ([`RemoteClient::connect_web`], wasm32 only): the browser's
//!   `WebSocket`. It returns at once; the handshake runs in the socket's
//!   callbacks, and [`RemoteClient::status`] is `Connecting` until `Welcome`.
//!   Commands sent meanwhile wait and go out after `Hello`.
//!
//! Commands get their results as with a local client. When the connection
//! drops, every command still waiting (and every command sent afterwards)
//! gets `Refused(DISCONNECTED)`, so [`super::ReplyHandlers`] never keeps a
//! closure forever. A connection is never reopened:
//! [`RemoteClient::reconnect`] makes a new client (a new session).

use std::{
    collections::{BTreeMap, BTreeSet, HashMap, VecDeque},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex, MutexGuard, OnceLock, PoisonError, RwLock, RwLockReadGuard,
    },
    time::Duration,
};
use web_time::Instant;

use blaulicht_audio_engine::{AudioSpectrogram, CollectorOutput, SignalCollectorParams};

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
    server::protocol::{ClientMessage, EngineStatus, ServerMessage, SpectrogramUpdate, Stream},
    state::{AppHealthState, ArtNetReceiver, PluginRuntimeKind, PluginState, NUM_DMX_UNIVERSES},
    ui_ops::WasmUiOp,
};

#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(target_arch = "wasm32")]
mod web;

#[cfg(not(target_arch = "wasm32"))]
use native::Transport;
#[cfg(target_arch = "wasm32")]
use web::Transport;

/// Why a command fails when the connection to the engine is gone.
pub const DISCONNECTED: &str = "Disconnected from the engine";

/// The session a browser client reports before the engine's `Welcome`
/// named its real one. No engine session has this id, so
/// [`EngineClient::with_session`] is `None` until then.
const UNKNOWN_SESSION: SessionId = SessionId(u32::MAX);
/// The client id a browser client reports before `Welcome`.
const UNKNOWN_CLIENT: ClientId = ClientId::Remote(u32::MAX);
/// How many recent command round trips [`RemoteStats`] keeps.
const ROUND_TRIP_SAMPLES: usize = 128;

/// What a [`RemoteClient`] received so far, for diagnostics.
#[derive(Clone, Debug, Default)]
pub struct RemoteStats {
    pub frames: u64,
    pub bytes: u64,
    /// Frames and bytes by message kind (`"Show"`, `"Session"`, `"Dmx"`, ...).
    pub by_kind: BTreeMap<&'static str, (u64, u64)>,
    /// The size of the last show snapshot.
    pub last_show_bytes: usize,
    /// Results received, and the time from sending each command to its
    /// result for the most recent ones (oldest first).
    pub round_trips: u64,
    pub recent_round_trips: VecDeque<Duration>,
}

/// Where a [`RemoteClient`]'s connection stands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RemoteStatus {
    /// Waiting for the engine's `Welcome` (browser transport only).
    Connecting,
    Connected,
    /// The connection is gone, and why. It is not reopened.
    Disconnected(String),
}

/// What the engine said in its `Welcome`.
#[derive(Clone, Debug)]
struct Welcome {
    client: ClientId,
    session: SessionId,
    engine_version: String,
}

/// Messages and results waiting for the frontend.
#[derive(Default)]
struct Inbox {
    system: VecDeque<SystemMessage>,
    results: Vec<CommandResult>,
    bus: VecDeque<BusMessage>,
    /// Commands sent whose result hasn't arrived, and when they were sent.
    pending: BTreeMap<CommandSeq, Instant>,
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

/// What the transport writes and the frontend reads.
struct RemoteState {
    /// Set once, from the engine's `Welcome`.
    welcome: OnceLock<Welcome>,
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
    /// `false` once the connection is gone.
    connected: AtomicBool,
    disconnect_reason: Mutex<Option<String>>,
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
    fn new() -> Self {
        Self {
            welcome: OnceLock::new(),
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
            disconnect_reason: Mutex::new(None),
            stats: Mutex::default(),
        }
    }

    fn client(&self) -> ClientId {
        self.welcome.get().map_or(UNKNOWN_CLIENT, |w| w.client)
    }

    fn refused(&self, seq: CommandSeq) -> CommandResult {
        CommandResult {
            client: self.client(),
            seq,
            outcome: CommandOutcome::Refused(DISCONNECTED.to_string()),
        }
    }

    /// The connection is gone: every waiting command fails.
    fn disconnect(&self, reason: &str) {
        if !self.connected.swap(false, Ordering::SeqCst) {
            return;
        }
        lock(&self.disconnect_reason).get_or_insert_with(|| reason.to_string());
        let mut inbox = lock(&self.inbox);
        let pending = std::mem::take(&mut inbox.pending);
        let results: Vec<_> = pending.into_keys().map(|seq| self.refused(seq)).collect();
        inbox.results.extend(results);
        drop(inbox);
        match self.welcome.get() {
            Some(welcome) => {
                tracing::info!("Remote client {}: disconnected ({reason})", welcome.client)
            }
            None => tracing::info!("Remote client: could not connect ({reason})"),
        }
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
                let sent = inbox.pending.remove(&result.seq);
                inbox.results.push(result);
                drop(inbox);
                if let Some(sent) = sent {
                    let mut stats = lock(&self.stats);
                    stats.round_trips += 1;
                    if stats.recent_round_trips.len() >= ROUND_TRIP_SAMPLES {
                        stats.recent_round_trips.pop_front();
                    }
                    stats.recent_round_trips.push_back(sent.elapsed());
                }
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

/// An [`EngineClient`] for an engine in another process (or this one, over
/// loopback), connected with [`RemoteClient::connect`] (native) or
/// [`RemoteClient::connect_web`] (in a browser).
///
/// Before the first snapshots arrive, the show is empty and
/// [`EngineClient::with_session`] is `None`. Dropping the client closes the
/// connection, which closes its session on the engine.
pub struct RemoteClient {
    next_seq: AtomicU64,
    state: Arc<RemoteState>,
    /// The streams it receives (for [`Self::reconnect`]).
    streams: Mutex<Vec<Stream>>,
    last_ui: Option<BTreeSet<u8>>,
    transport: Transport,
}

impl RemoteClient {
    fn with_transport(state: Arc<RemoteState>, streams: &[Stream], transport: Transport) -> Self {
        Self {
            next_seq: AtomicU64::new(1),
            state,
            streams: Mutex::new(streams.to_vec()),
            last_ui: None,
            transport,
        }
    }

    /// The engine's version (`CARGO_PKG_VERSION` of its build); `None` until
    /// it answered.
    pub fn engine_version(&self) -> Option<&str> {
        self.state
            .welcome
            .get()
            .map(|welcome| welcome.engine_version.as_str())
    }

    /// `false` once the connection is gone (it is not reopened).
    pub fn is_connected(&self) -> bool {
        self.state.connected.load(Ordering::SeqCst)
    }

    /// Where the connection stands.
    pub fn status(&self) -> RemoteStatus {
        if !self.is_connected() {
            let reason = lock(&self.state.disconnect_reason).clone();
            RemoteStatus::Disconnected(reason.unwrap_or_else(|| DISCONNECTED.to_string()))
        } else if self.state.welcome.get().is_none() {
            RemoteStatus::Connecting
        } else {
            RemoteStatus::Connected
        }
    }

    /// Replaces the streams this client receives.
    pub fn subscribe(&self, streams: &[Stream]) {
        *lock(&self.streams) = streams.to_vec();
        self.transport.send(ClientMessage::Subscribe {
            streams: streams.to_vec(),
        });
    }

    /// The streams this client receives.
    pub fn streams(&self) -> Vec<Stream> {
        lock(&self.streams).clone()
    }

    /// The digest of the last show snapshot, if one arrived.
    pub fn show_digest(&self) -> Option<u64> {
        *lock(&self.state.show_digest)
    }

    /// Whether the first show and session snapshots arrived.
    pub fn has_snapshot(&self) -> bool {
        self.show_digest().is_some() && self.state.session_received.load(Ordering::Relaxed)
    }

    /// What arrived so far.
    pub fn stats(&self) -> RemoteStats {
        lock(&self.state.stats).clone()
    }
}

impl EngineClient for RemoteClient {
    fn client(&self) -> ClientId {
        self.state.client()
    }

    fn session(&self) -> SessionId {
        self.state
            .welcome
            .get()
            .map_or(UNKNOWN_SESSION, |welcome| welcome.session)
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
            && self.transport.send(ClientMessage::Command { seq, body });
        if sent {
            inbox.pending.insert(seq, Instant::now());
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
        self.transport.send(ClientMessage::PublishUi {
            open_plugins: snapshot.open_plugins.clone(),
        });
        self.last_ui = Some(snapshot.open_plugins);
    }
}
