//! What a frontend sees of the engine: read show state, read its own operator
//! session, send commands, get their results, and read the live streams.
//!
//! - [`EngineClient`] is that surface. Page code written against it runs the
//!   same way in process ([`LocalClient`]) and against a remote engine
//!   ([`RemoteClient`], over the websocket protocol in
//!   [`crate::server::protocol`]): locally `with_show` is a read guard on the
//!   engine state, remotely on the client's copy of it.
//! - [`LocalClient`] is the in-process implementation. It holds the engine's
//!   `Arc<AppState>` privately, the client's [`ClientConnection`], and the
//!   hub and bus handles (to attach more clients, see
//!   [`LocalClient::open_client`]). Reads take a read lock; sends go over a
//!   crossbeam channel, with no serialization.
//! - [`ReplyHandlers`] runs a closure when the reply to a command arrives
//!   ("send, then do this on the result"), for dialogs.
//!
//! The trait is not object-safe: its readers take closures generically, which
//! keeps them zero-cost. Frontends hold a concrete client type.

use std::{
    collections::{HashMap, VecDeque},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard, PoisonError, RwLockReadGuard},
};

use blaulicht_audio_engine::{AudioSpectrogram, SignalCollectorParams};
use blaulicht_shared::{ControlEvent, ControlEventMessage, EventOriginator};

use crate::{
    audio::defs::AudioThreadControlSignal,
    dmx::{EngineState, SessionView},
    event::EventBusHandle,
    frontend_ui::FrontendUiSnapshot,
    hub::{
        BusBody, BusMessage, ClientConnection, ClientId, CommandOutcome, CommandResult, CommandSeq,
        SessionId, SystemHub, SUBSCRIBER_CAPACITY,
    },
    msg::{SystemMessage, TERMINAL_ONLY_LOG_TARGET},
    state::{AppHealthState, AppState, ArtNetReceiver, PluginRuntimeKind, PluginState},
    ui_ops::WasmUiOp,
};

mod remote;
pub use remote::{RemoteClient, RemoteStats, DISCONNECTED};

/// A frontend's connection to the engine.
///
/// Readers (`with_*`) run a closure on the current state. Scope them to one
/// widget block, don't nest them, and don't send commands from inside them:
/// locally the closure runs under the engine's read lock, which the engine
/// needs to write every tick.
pub trait EngineClient {
    /// Who this client is; the engine addresses its [`CommandResult`]s to it.
    fn client(&self) -> ClientId;

    /// The operator session this client acts in (selection, selection stack,
    /// control buffer, scene focus, focused scene graph).
    /// [`SessionId::DESK`] for the native UI.
    fn session(&self) -> SessionId;

    //
    // Show state.
    //

    /// Runs `f` on the engine state: show state plus every operator session.
    /// The `DESK` session is in the shared fields (`.0.selection`,
    /// `curr_scene()`, `get_selection()`, ...); another session's state is
    /// only reachable through [`EngineState::session`], or
    /// [`Self::with_session`].
    fn with_show<R>(&self, f: impl FnOnce(&EngineState) -> R) -> R;

    /// Runs `f` on the engine state and this client's session, the
    /// session-aware form of reading `.0.selection` or `curr_scene()`:
    /// `me.selection`, `me.fixture_selection(&show.0.groups)`,
    /// `show.get_scene(me.scene_focus)`.
    ///
    /// `None` if the session is not open: a new session opens on the engine's
    /// next tick after the client connected, and is gone once the engine
    /// closed it. Never `None` for [`SessionId::DESK`].
    fn with_session<R>(&self, f: impl FnOnce(&EngineState, SessionView<'_>) -> R) -> Option<R> {
        let session = self.session();
        self.with_show(|show| show.session(session).map(|me| f(show, me)))
    }

    //
    // Commands and their results.
    //

    /// Sends a command in this client's envelope: a `ControlEventMessage`
    /// (forwarded to plugins too), a `HostCommand` (host-only) or a
    /// [`BusBody`]. Returns the sequence number its [`CommandResult`] will
    /// carry.
    fn send(&self, body: impl Into<BusBody>) -> CommandSeq;

    /// Sends `event` as this frontend (`EventOriginator::Web`).
    fn send_event(&self, event: ControlEvent) -> CommandSeq {
        self.send(ControlEventMessage::new(EventOriginator::Web, event))
    }

    /// The results of this client's commands that arrived since the last
    /// call, in arrival order. Call it once per frame; see [`ReplyHandlers`]
    /// to run a closure on one command's result.
    fn take_results(&self) -> Vec<CommandResult>;

    //
    // Streams.
    //

    /// Next system message: logs, startup stages, tick speeds, heartbeats,
    /// audio devices, plugin alerts, and requests addressed to this client.
    /// [`SystemMessage::CommandResult`]s are not in this stream; they are
    /// collected for [`Self::take_results`].
    fn try_recv_system(&self) -> Option<SystemMessage>;

    /// Next message from the bus: every client's commands, this one's too.
    fn try_recv_bus(&self) -> Option<BusMessage>;

    /// Runs `f` on the DMX output buffer of `universe` (index 0 is the start
    /// code, channels are 1..=512). `None` if there is no such universe.
    fn with_dmx_universe<R>(&self, universe: usize, f: impl FnOnce(&[u8; 513]) -> R) -> Option<R>;

    /// Runs `f` on the DMX, Art-Net, MIDI and serial health.
    fn with_health<R>(&self, f: impl FnOnce(&AppHealthState) -> R) -> R;

    /// Runs `f` on the audio spectrogram (recent analysis columns).
    fn with_spectrogram<R>(&self, f: impl FnOnce(&AudioSpectrogram) -> R) -> R;

    /// Runs `f` on the latest UI ops of every plugin, by plugin id.
    fn with_plugin_ui_ops<R>(&self, f: impl FnOnce(&HashMap<u8, Vec<WasmUiOp>>) -> R) -> R;

    /// Runs `f` on the audio analysis parameters.
    fn with_audio_params<R>(&self, f: impl FnOnce(&SignalCollectorParams) -> R) -> R;

    /// Runs `f` on the Art-Net receivers, user-created and plugin-owned.
    fn with_artnet_receivers<R>(&self, f: impl FnOnce(&[ArtNetReceiver]) -> R) -> R;

    /// Runs `f` on the configured plugins, by plugin id.
    fn with_plugins<R>(&self, f: impl FnOnce(&HashMap<u8, PluginState>) -> R) -> R;

    /// The grand master, 0..=100 percent (`HostCommand::SetGrandMaster`).
    fn grand_master_percent(&self) -> u8;

    /// Whether the main loop is running, reloading, stopped or crashed.
    fn mainloop_state(&self) -> AudioThreadControlSignal;

    /// The open showfile, if any (the engine's `EngineState::showfile`).
    fn showfile_path(&self) -> Option<PathBuf> {
        self.with_show(|show| show.showfile().map(Path::to_path_buf))
    }

    /// Runs `f` on what each loaded plugin is (an animation plugin with its
    /// stable key and name, or a regular plugin), by plugin id.
    fn with_plugin_runtime_kinds<R>(
        &self,
        f: impl FnOnce(&HashMap<u8, PluginRuntimeKind>) -> R,
    ) -> R;

    /// Runs `f` on the latest editor UI ops of every WASM animation editor
    /// instance (see `HostCommand::KeepAnimationEditorAlive`), by instance id.
    fn with_animation_editor_ui_ops<R>(
        &self,
        f: impl FnOnce(&HashMap<u64, Vec<WasmUiOp>>) -> R,
    ) -> R;

    //
    // View state.
    //

    /// Publishes the part of this frontend's view state the engine and
    /// plugins may read (open plugin UIs, external screens). Cheap to call
    /// every frame: only changes are sent.
    fn publish_ui(&mut self, snapshot: FrontendUiSnapshot);
}

/// System messages read past while collecting results, and results read past
/// while reading system messages.
#[derive(Default)]
struct Inbox {
    system: VecDeque<SystemMessage>,
    results: Vec<CommandResult>,
    /// The system backlog hit its cap and dropped messages (warned once).
    overflowed: bool,
}

impl Inbox {
    /// Keeps a system message for [`EngineClient::try_recv_system`]. A client
    /// that only ever takes results would otherwise grow the backlog without
    /// bound, so it is capped like a hub subscription.
    fn keep_system(&mut self, message: SystemMessage) {
        if self.system.len() >= SUBSCRIBER_CAPACITY {
            self.system.pop_front();
            if !self.overflowed {
                self.overflowed = true;
                tracing::warn!(
                    target: TERMINAL_ONLY_LOG_TARGET,
                    "Engine client: system messages are not read; dropping the oldest (logged once)"
                );
            }
        }
        self.system.push_back(message);
    }
}

/// The in-process [`EngineClient`]: reads take the engine's locks directly,
/// commands go over the event bus.
///
/// Connecting the same [`ClientId`] twice is unsupported (the first
/// connection's drop would clear both from the view-state mirror). Dropping a
/// client disconnects it from the bus and the hub, and closes the session it
/// opened, if any.
pub struct LocalClient {
    connection: ClientConnection,
    app: Arc<AppState>,
    hub: SystemHub,
    bus: EventBusHandle<BusMessage>,
    inbox: Mutex<Inbox>,
}

impl LocalClient {
    /// Connects client `id` in `session`. The desk is
    /// `connect(ClientId::Desk, SessionId::DESK, ..)`.
    pub fn connect(
        id: ClientId,
        session: SessionId,
        app: Arc<AppState>,
        hub: &SystemHub,
        bus: &EventBusHandle<BusMessage>,
    ) -> Self {
        let connection = ClientConnection::connect(id, session, hub, bus, &app.frontend_ui);
        Self::new(connection, app, hub, bus)
    }

    /// Connects a new client ([`SystemHub::allocate_client`]) in a new
    /// operator session of its own, which closes when the client is dropped.
    /// The engine opens the session on its next tick, so
    /// [`EngineClient::with_session`] is `None` until then; commands sent
    /// before that are applied in it.
    pub fn connect_new_session(
        app: Arc<AppState>,
        hub: &SystemHub,
        bus: &EventBusHandle<BusMessage>,
    ) -> Self {
        let connection = ClientConnection::connect_new_session(
            hub.allocate_client(),
            hub,
            bus,
            &app.frontend_ui,
        );
        Self::new(connection, app, hub, bus)
    }

    /// Another client of the same engine, in a new session of its own (see
    /// [`Self::connect_new_session`]).
    pub fn open_client(&self) -> Self {
        Self::connect_new_session(Arc::clone(&self.app), &self.hub, &self.bus)
    }

    fn new(
        connection: ClientConnection,
        app: Arc<AppState>,
        hub: &SystemHub,
        bus: &EventBusHandle<BusMessage>,
    ) -> Self {
        Self {
            connection,
            app,
            hub: hub.clone(),
            bus: bus.clone(),
            inbox: Mutex::default(),
        }
    }

    fn inbox(&self) -> MutexGuard<'_, Inbox> {
        self.inbox.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// A read guard that survives a writer's panic: readers keep showing the last
/// state instead of panicking the frontend too.
fn read<T>(lock: &std::sync::RwLock<T>) -> RwLockReadGuard<'_, T> {
    lock.read().unwrap_or_else(PoisonError::into_inner)
}

impl EngineClient for LocalClient {
    fn client(&self) -> ClientId {
        self.connection.id()
    }

    fn session(&self) -> SessionId {
        self.connection.session()
    }

    fn with_show<R>(&self, f: impl FnOnce(&EngineState) -> R) -> R {
        f(&read(&self.app.dmx_engine))
    }

    fn send(&self, body: impl Into<BusBody>) -> CommandSeq {
        self.connection.send_body(body.into())
    }

    fn take_results(&self) -> Vec<CommandResult> {
        let mut inbox = self.inbox();
        while let Some(message) = self.connection.try_recv_system() {
            match message {
                SystemMessage::CommandResult(result) => inbox.results.push(result),
                message => inbox.keep_system(message),
            }
        }
        std::mem::take(&mut inbox.results)
    }

    fn try_recv_system(&self) -> Option<SystemMessage> {
        let mut inbox = self.inbox();
        if let Some(message) = inbox.system.pop_front() {
            return Some(message);
        }
        loop {
            match self.connection.try_recv_system()? {
                SystemMessage::CommandResult(result) => inbox.results.push(result),
                message => return Some(message),
            }
        }
    }

    fn try_recv_bus(&self) -> Option<BusMessage> {
        self.connection.try_recv_bus()
    }

    fn with_dmx_universe<R>(&self, universe: usize, f: impl FnOnce(&[u8; 513]) -> R) -> Option<R> {
        let buffer = read(self.app.dmx_universes.get(universe)?);
        Some(f(&buffer.dmx_buffer))
    }

    fn with_health<R>(&self, f: impl FnOnce(&AppHealthState) -> R) -> R {
        f(&read(&self.app.health_data))
    }

    fn with_spectrogram<R>(&self, f: impl FnOnce(&AudioSpectrogram) -> R) -> R {
        f(&read(&self.app.audio_spectrogram))
    }

    fn with_plugin_ui_ops<R>(&self, f: impl FnOnce(&HashMap<u8, Vec<WasmUiOp>>) -> R) -> R {
        f(&read(&self.app.plugin_ui_ops))
    }

    fn with_audio_params<R>(&self, f: impl FnOnce(&SignalCollectorParams) -> R) -> R {
        f(&read(&self.app.audio_params))
    }

    fn with_artnet_receivers<R>(&self, f: impl FnOnce(&[ArtNetReceiver]) -> R) -> R {
        f(&read(&self.app.artnet_output).receivers)
    }

    fn with_plugins<R>(&self, f: impl FnOnce(&HashMap<u8, PluginState>) -> R) -> R {
        f(&read(&self.app.plugins))
    }

    fn grand_master_percent(&self) -> u8 {
        self.app.grand_master_percent()
    }

    fn mainloop_state(&self) -> AudioThreadControlSignal {
        *read(&self.app.mainloop_state)
    }

    fn with_plugin_runtime_kinds<R>(
        &self,
        f: impl FnOnce(&HashMap<u8, PluginRuntimeKind>) -> R,
    ) -> R {
        f(&read(&self.app.plugin_runtime_kinds))
    }

    fn with_animation_editor_ui_ops<R>(
        &self,
        f: impl FnOnce(&HashMap<u64, Vec<WasmUiOp>>) -> R,
    ) -> R {
        f(&read(&self.app.wasm_animation_ui_ops))
    }

    fn publish_ui(&mut self, snapshot: FrontendUiSnapshot) {
        self.connection.publish_ui(snapshot);
    }
}

/// A closure waiting for the result of one command.
pub type ReplyHandler<Ctx> = Box<dyn FnOnce(&mut Ctx, &CommandOutcome)>;

/// "Send, then run this closure on the matching result": closures waiting
/// for the results of commands a frontend sent, for example a dialog that
/// closes on success, shows a refusal or selects the id the engine created.
///
/// `Ctx` is what the closures may change; the desk passes the whole app. Its
/// frame loop takes the client's results once per frame and hands them to
/// [`ReplyHandlers::dispatch`].
pub struct ReplyHandlers<Ctx> {
    waiting: Vec<(CommandSeq, ReplyHandler<Ctx>)>,
}

impl<Ctx> Default for ReplyHandlers<Ctx> {
    fn default() -> Self {
        Self {
            waiting: Vec::new(),
        }
    }
}

impl<Ctx> ReplyHandlers<Ctx> {
    /// Sends `body` through `client` and runs `on_result` with the command's
    /// outcome when its result is dispatched.
    pub fn send_then(
        &mut self,
        client: &impl EngineClient,
        body: impl Into<BusBody>,
        on_result: impl FnOnce(&mut Ctx, &CommandOutcome) + 'static,
    ) -> CommandSeq {
        let seq = client.send(body);
        self.waiting.push((seq, Box::new(on_result)));
        seq
    }

    /// `true` while the result of command `seq` has not been dispatched.
    pub fn is_waiting(&self, seq: CommandSeq) -> bool {
        self.waiting.iter().any(|(waiting, _)| *waiting == seq)
    }

    /// Commands still waiting for their result.
    pub fn len(&self) -> usize {
        self.waiting.len()
    }

    pub fn is_empty(&self) -> bool {
        self.waiting.is_empty()
    }

    /// Runs the closure of every command in `results` with `ctx`, in result
    /// order. `handlers` finds this registry inside `ctx` (for the desk,
    /// `|app| &mut app.reply_handlers`), so the closures can borrow all of
    /// `ctx`. Results nobody waits for are ignored.
    pub fn dispatch(ctx: &mut Ctx, handlers: fn(&mut Ctx) -> &mut Self, results: &[CommandResult]) {
        for result in results {
            let waiting = &mut handlers(ctx).waiting;
            let Some(index) = waiting.iter().position(|(seq, _)| *seq == result.seq) else {
                continue;
            };
            let (_, on_result) = waiting.swap_remove(index);
            on_result(ctx, &result.outcome);
        }
    }
}

/// A headless engine with clients attached, for tests: the real
/// `DmxEngine`, event bus and hub, no UI and no audio thread.
///
/// ```ignore
/// let mut h = TwoClients::start(small_rig);
/// h.engine.round_trip(&h.desk, ControlEventMessage::new(Web, SelectGroup(0)));
/// h.engine.round_trip(&h.remote, ControlEventMessage::new(Web, SelectGroup(1)));
/// assert_eq!(h.desk.with_session(|_, me| me.selection.clone()), ..);
/// ```
#[cfg(test)]
pub(crate) mod testing {
    use std::{sync::Arc, time::Duration};
    use web_time::Instant;

    use blaulicht_audio_engine::CollectorOutput;
    use blaulicht_shared::{
        fixture::{
            light::Light,
            state::{Fixture, FixtureGroup},
            FixtureType,
        },
        scene::EngineSink,
    };

    use super::{EngineClient, LocalClient};
    use crate::{
        dmx::{DmxEngine, EngineState},
        event::{EventBusHandle, SystemEventBus},
        hub::{BusBody, BusMessage, ClientId, CommandOutcome, CommandResult, SessionId, SystemHub},
        state::AppState,
    };

    /// How long [`TestEngine::replies`] waits before failing the test.
    const REPLY_TIMEOUT: Duration = Duration::from_secs(5);

    /// The engine side: ticked by the test, not by a main loop.
    pub(crate) struct TestEngine {
        app: Arc<AppState>,
        engine: DmxEngine,
        hub: SystemHub,
        bus: EventBusHandle<BusMessage>,
    }

    impl TestEngine {
        /// An engine on a show that `seed` builds (see [`small_rig`]). The bus
        /// runs on its own thread, as in the app.
        pub(crate) fn start(seed: impl FnOnce(&mut EngineState)) -> Self {
            let app = Arc::new(AppState::new(&[]));
            seed(&mut app.dmx_engine.write().unwrap());
            let hub = SystemHub::new();
            let mut bus = SystemEventBus::<BusMessage>::new();
            let handle = bus.handle();
            let engine = DmxEngine::new(
                Arc::clone(&app),
                handle.connect(),
                hub.clone(),
                [String::new(), String::new()],
            );
            std::thread::spawn(move || bus.run());
            Self {
                app,
                engine,
                hub,
                bus: handle,
            }
        }

        /// The desk's client: `ClientId::Desk` in `SessionId::DESK`.
        pub(crate) fn connect_desk(&self) -> LocalClient {
            LocalClient::connect(
                ClientId::Desk,
                SessionId::DESK,
                Arc::clone(&self.app),
                &self.hub,
                &self.bus,
            )
        }

        /// A new client in a session of its own, already opened by the
        /// engine.
        pub(crate) fn connect_remote(&mut self) -> LocalClient {
            let client =
                LocalClient::connect_new_session(Arc::clone(&self.app), &self.hub, &self.bus);
            self.tick();
            assert!(
                client.with_session(|_, _| ()).is_some(),
                "the engine did not open {}",
                client.session()
            );
            client
        }

        /// One engine tick (bus messages and session changes, scene graph,
        /// animations, rendering), with silent audio.
        pub(crate) fn tick(&mut self) {
            self.engine.tick(&CollectorOutput::default());
        }

        /// Ticks until `client` has `count` results, and returns them.
        pub(crate) fn replies(&mut self, client: &LocalClient, count: usize) -> Vec<CommandResult> {
            let deadline = Instant::now() + REPLY_TIMEOUT;
            let mut results = Vec::new();
            while results.len() < count {
                assert!(Instant::now() < deadline, "only got {results:?}");
                self.tick();
                results.extend(client.take_results());
                if results.len() < count {
                    std::thread::sleep(Duration::from_millis(1));
                }
            }
            results
        }

        /// Sends `body` from `client` and ticks until its result arrives.
        pub(crate) fn round_trip(
            &mut self,
            client: &LocalClient,
            body: impl Into<BusBody>,
        ) -> CommandOutcome {
            let seq = client.send(body);
            let mut results = self.replies(client, 1);
            assert_eq!(results.len(), 1, "{results:?}");
            let result = results.remove(0);
            assert_eq!(result.seq, seq, "a result for another command");
            result.outcome
        }

        /// Engine-side write access, for setting a test up.
        pub(crate) fn edit_show<R>(&self, f: impl FnOnce(&mut EngineState) -> R) -> R {
            f(&mut self.app.dmx_engine.write().unwrap())
        }

        /// Lets the engine forward supervisor requests (audio input, plugin
        /// reload); the test reads them from the returned receiver.
        pub(crate) fn connect_supervisor(
            &mut self,
        ) -> crossbeam_channel::Receiver<crate::mainloop::SupervisorRequest> {
            let (sender, receiver) = crossbeam_channel::unbounded();
            self.engine.set_supervisor(sender);
            receiver
        }

        pub(crate) fn app(&self) -> &Arc<AppState> {
            &self.app
        }

        /// A persistence service for this engine (not running: the test
        /// steps it), writing its config to `config_path`.
        pub(crate) fn persistence(
            &self,
            config_path: std::path::PathBuf,
        ) -> crate::persistence::PersistenceService {
            crate::persistence::PersistenceService::new(
                Arc::clone(&self.app),
                self.hub.clone(),
                crate::config::SharedConfig::new(crate::config::Config::default(), config_path),
            )
        }

        pub(crate) fn hub(&self) -> &SystemHub {
            &self.hub
        }

        pub(crate) fn engine(&self) -> &DmxEngine {
            &self.engine
        }

        /// The websocket server for this engine, on `options.bind` (port 0
        /// picks a free one).
        pub(crate) fn start_server(
            &self,
            options: crate::server::ServerOptions,
        ) -> crate::server::ServerHandle {
            crate::server::Server::start(
                options,
                Arc::clone(&self.app),
                self.hub.clone(),
                self.bus.clone(),
            )
            .expect("server")
        }

        /// Ticks until `done` holds, and fails the test after a while.
        pub(crate) fn tick_until(&mut self, what: &str, mut done: impl FnMut(&mut Self) -> bool) {
            let deadline = Instant::now() + REPLY_TIMEOUT;
            while !done(self) {
                assert!(Instant::now() < deadline, "timed out waiting for {what}");
                self.tick();
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    }

    /// The engine with the desk and one remote client in its own session.
    pub(crate) struct TwoClients {
        pub(crate) engine: TestEngine,
        pub(crate) desk: LocalClient,
        pub(crate) remote: LocalClient,
    }

    impl TwoClients {
        pub(crate) fn start(seed: impl FnOnce(&mut EngineState)) -> Self {
            let mut engine = TestEngine::start(seed);
            let desk = engine.connect_desk();
            let remote = engine.connect_remote();
            Self {
                engine,
                desk,
                remote,
            }
        }
    }

    /// Groups 0, 1 and 2 with two RGB fixtures each (universe 0, channels
    /// from 1, three per fixture), user scenes 0 ("Main"), 1 ("A") and 2
    /// ("B"), and the `BLANK` scene. Focus is scene 0.
    pub(crate) fn small_rig(state: &mut EngineState) {
        for gid in 0..3u8 {
            let fixtures = (0..2u8)
                .map(|fid| {
                    let addr = 1 + 3 * (gid as usize * 2 + fid as usize);
                    let light = FixtureType::from(Light::Generic3ChanNoAlpha);
                    (fid, Fixture::new(0, addr, format!("F{addr}"), light))
                })
                .collect();
            state.0.groups.insert(
                gid,
                FixtureGroup {
                    name: format!("G{gid}"),
                    fixtures,
                    folder: None,
                },
            );
        }
        let sink = EngineSink::from_groups(&state.0.groups);
        state.0.scenes.get_mut(&0).expect("scene 0").sink = sink;
        state.0.new_scene("A".to_string());
        state.0.new_scene("B".to_string());
        state.0.ensure_blank_scene();
        state.0.current_scene_focus = 0;
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, collections::HashSet, rc::Rc};

    use blaulicht_shared::{
        fixture::{state::FixtureState, value::FixtureValue},
        ControlEvent, ControlEventMessage, EventOriginator, LogLevel,
    };

    use super::{
        testing::{small_rig, TestEngine, TwoClients},
        EngineClient, ReplyHandlers,
    };
    use crate::{
        hub::{
            ClientId, CommandOutcome, CommandResult, CreatedId, HostCommand, SessionId, SystemHub,
        },
        msg::SystemMessage,
    };

    const OK: CommandOutcome = CommandOutcome::Ok { created: None };

    fn web(event: ControlEvent) -> ControlEventMessage {
        ControlEventMessage::new(EventOriginator::Web, event)
    }

    /// The acceptance test for the harness: two clients in two sessions keep
    /// two selections, and each sees only its own.
    #[test]
    fn two_sessions_keep_two_selections() {
        let mut h = TwoClients::start(small_rig);
        assert_eq!(h.desk.session(), SessionId::DESK);
        assert_ne!(h.remote.session(), SessionId::DESK);
        assert!(matches!(h.remote.client(), ClientId::Remote(_)));

        assert_eq!(
            h.engine
                .round_trip(&h.desk, web(ControlEvent::SelectGroup(0))),
            OK
        );
        assert_eq!(
            h.engine.round_trip(
                &h.remote,
                web(ControlEvent::Transaction(vec![
                    ControlEvent::SelectGroup(1),
                    ControlEvent::LimitSelectionToFixtureInCurrentGroup(1),
                ]))
            ),
            OK
        );

        let selection = |client: &super::LocalClient| {
            client
                .with_session(|show, me| {
                    (
                        me.selection.group_ids.clone(),
                        me.fixture_selection(&show.0.groups).fixtures,
                    )
                })
                .expect("open session")
        };
        assert_eq!(
            selection(&h.desk),
            (HashSet::from([0]), vec![(0, 0), (0, 1)])
        );
        assert_eq!(selection(&h.remote), (HashSet::from([1]), vec![(1, 1)]));

        // The desk's session is the shared fields, which plugins and the
        // showfile see; the remote's lives in the side store.
        h.desk.with_show(|show| {
            assert_eq!(show.0.selection.group_ids, HashSet::from([0]));
            assert_eq!(show.get_selection().fixtures, vec![(0, 0), (0, 1)]);
        });

        // A control event writes the sender's selection only.
        assert_eq!(
            h.engine
                .round_trip(&h.remote, web(ControlEvent::SetAlpha(200))),
            OK
        );
        h.desk.with_show(|show| {
            let alpha = |gid: u8, fid: u8| show.curr_scene().sink.fixture_states[&(gid, fid)].alpha;
            assert_eq!(alpha(1, 1), FixtureValue::literal_u8(200));
            assert_eq!(alpha(1, 0), FixtureState::default().alpha);
            assert_eq!(alpha(0, 0), FixtureState::default().alpha);
        });
    }

    #[test]
    fn a_session_view_follows_its_own_focus() {
        let mut h = TwoClients::start(small_rig);
        assert_eq!(
            h.engine
                .round_trip(&h.remote, web(ControlEvent::SetSceneFocus(2))),
            OK
        );
        let focused_name = |client: &super::LocalClient| {
            client
                .with_session(|show, me| show.get_scene(me.scene_focus).map(|s| s.name.clone()))
                .flatten()
        };
        assert_eq!(focused_name(&h.remote).as_deref(), Some("B"));
        assert_eq!(
            focused_name(&h.desk),
            h.desk.with_show(|s| Some(s.curr_scene().name.clone()))
        );
    }

    #[test]
    fn results_reach_only_their_sender_in_order() {
        let mut h = TwoClients::start(small_rig);
        let ping = h.desk.send(HostCommand::Ping);
        let refused = h.desk.send_event(ControlEvent::SelectGroup(9));
        let palette = h.desk.send_event(ControlEvent::CreatePalette(
            "Red".to_string(),
            blaulicht_shared::palette::PaletteKind::Color(Default::default()),
        ));
        let results = h.engine.replies(&h.desk, 3);
        assert_eq!(
            results,
            vec![
                CommandResult {
                    client: ClientId::Desk,
                    seq: ping,
                    outcome: OK
                },
                CommandResult {
                    client: ClientId::Desk,
                    seq: refused,
                    outcome: CommandOutcome::Refused("Illegal group".to_string())
                },
                CommandResult {
                    client: ClientId::Desk,
                    seq: palette,
                    outcome: CommandOutcome::Ok {
                        created: Some(CreatedId::Palette(0))
                    }
                },
            ]
        );
        h.engine.tick();
        assert!(h.remote.take_results().is_empty());
        assert!(h.desk.take_results().is_empty());
    }

    #[test]
    fn closing_a_client_closes_its_session() {
        let mut h = TwoClients::start(small_rig);
        let session = h.remote.session();
        assert!(h.engine.edit_show(|show| show.has_session(session)));
        drop(h.remote);
        h.engine.tick();
        assert!(!h.engine.edit_show(|show| show.has_session(session)));

        // A third client gets a fresh session.
        let third = h.engine.connect_remote();
        assert!(third.session() > session);
        assert_eq!(
            third.open_client().session(),
            SessionId(third.session().0 + 1)
        );
    }

    /// Taking results doesn't lose the system messages read past them, and
    /// reading system messages doesn't lose the results.
    #[test]
    fn results_and_system_messages_are_sorted_without_loss() {
        let engine = TestEngine::start(|_| {});
        let hub = SystemHub::new();
        let bus = crate::event::SystemEventBus::<crate::hub::BusMessage>::new();
        let client = super::LocalClient::connect(
            ClientId::Desk,
            SessionId::DESK,
            std::sync::Arc::clone(engine.app()),
            &hub,
            &bus.handle(),
        );
        let result = |seq| {
            SystemMessage::CommandResult(CommandResult {
                client: ClientId::Desk,
                seq,
                outcome: OK,
            })
        };
        let heartbeat = |n| SystemMessage::Heartbeat(n);
        let seqs = |results: Vec<CommandResult>| results.iter().map(|r| r.seq).collect::<Vec<_>>();
        let beats = || {
            std::iter::from_fn(|| client.try_recv_system())
                .map(|message| match message {
                    SystemMessage::Heartbeat(n) => n,
                    _ => panic!("not a heartbeat"),
                })
                .collect::<Vec<_>>()
        };

        for message in [heartbeat(1), result(1), heartbeat(2), result(2)] {
            hub.publish_to(ClientId::Desk, message);
        }
        assert_eq!(seqs(client.take_results()), vec![1, 2]);
        assert_eq!(beats(), vec![1, 2]);

        for message in [heartbeat(3), result(3), heartbeat(4), result(4)] {
            hub.publish_to(ClientId::Desk, message);
        }
        assert_eq!(beats(), vec![3, 4]);
        assert_eq!(seqs(client.take_results()), vec![3, 4]);
        assert!(client.take_results().is_empty());
        assert!(client.try_recv_system().is_none());

        // Logs are not results either.
        hub.publish(SystemMessage::Log {
            message: "hello".to_string(),
            level: LogLevel::Info,
            source: "test".into(),
        });
        assert!(client.take_results().is_empty());
        assert!(matches!(
            client.try_recv_system(),
            Some(SystemMessage::Log { .. })
        ));
    }

    /// What a dialog does: send, keep the dialog open, and close it or show
    /// the refusal when the result arrives.
    #[test]
    fn reply_handlers_run_on_their_own_result() {
        #[derive(Default)]
        struct Dialog {
            handlers: ReplyHandlers<Dialog>,
            open: bool,
            error: Option<String>,
            created: Option<CreatedId>,
        }

        let mut h = TwoClients::start(small_rig);
        let mut dialog = Dialog {
            open: true,
            ..Dialog::default()
        };
        let order = Rc::new(RefCell::new(Vec::new()));

        let refused = {
            let order = Rc::clone(&order);
            dialog.handlers.send_then(
                &h.desk,
                web(ControlEvent::SelectGroup(9)),
                move |dialog: &mut Dialog, outcome| {
                    order.borrow_mut().push("refused");
                    if let CommandOutcome::Refused(why) = outcome {
                        dialog.error = Some(why.clone());
                    }
                },
            )
        };
        let created = {
            let order = Rc::clone(&order);
            dialog.handlers.send_then(
                &h.desk,
                web(ControlEvent::CreatePalette(
                    "Red".to_string(),
                    blaulicht_shared::palette::PaletteKind::Color(Default::default()),
                )),
                move |dialog: &mut Dialog, outcome| {
                    order.borrow_mut().push("created");
                    if let CommandOutcome::Ok { created } = outcome {
                        dialog.created = *created;
                        dialog.open = false;
                    }
                },
            )
        };
        // A command nobody waits for.
        h.desk.send(HostCommand::Ping);
        assert!(dialog.handlers.is_waiting(refused));
        assert!(dialog.handlers.is_waiting(created));
        assert_eq!(dialog.handlers.len(), 2);

        let results = h.engine.replies(&h.desk, 3);
        ReplyHandlers::dispatch(&mut dialog, |d| &mut d.handlers, &results);

        assert_eq!(*order.borrow(), vec!["refused", "created"]);
        assert_eq!(dialog.error.as_deref(), Some("Illegal group"));
        assert_eq!(dialog.created, Some(CreatedId::Palette(0)));
        assert!(!dialog.open);
        assert!(dialog.handlers.is_empty());

        // Each closure runs once.
        ReplyHandlers::dispatch(&mut dialog, |d| &mut d.handlers, &results);
        assert_eq!(order.borrow().len(), 2);
    }

    #[test]
    fn stream_readers_see_the_engine_state() {
        let mut h = TwoClients::start(small_rig);
        assert_eq!(
            h.engine
                .round_trip(&h.desk, web(ControlEvent::SetChannelOverride(0, 1, 77))),
            OK
        );
        h.engine.tick();
        assert_eq!(h.remote.with_dmx_universe(0, |dmx| dmx[1]), Some(77));
        assert_eq!(h.remote.with_dmx_universe(99, |dmx| dmx[1]), None);
        assert!(h.desk.with_plugins(|plugins| plugins.is_empty()));
        assert!(h
            .desk
            .with_artnet_receivers(|receivers| receivers.is_empty()));
        assert!(h.desk.with_plugin_ui_ops(|ops| ops.is_empty()));
        assert_eq!(
            h.desk
                .with_health(|health| health.dmx_universes_healthy.len()),
            crate::state::NUM_DMX_UNIVERSES
        );
        h.desk.with_spectrogram(|_| ());
        h.desk.with_audio_params(|_| ());
        assert_eq!(
            h.remote.mainloop_state(),
            crate::audio::defs::AudioThreadControlSignal::ABORTED,
            "no main loop runs in the test engine"
        );
        assert_eq!(h.remote.showfile_path(), None);
        h.engine
            .edit_show(|show| show.load_stage(Default::default(), Some("/shows/a.json".into())));
        assert_eq!(h.remote.showfile_path(), Some("/shows/a.json".into()));
    }

    /// The view-state snapshot reaches the engine's mirror under the
    /// client's id.
    #[test]
    fn published_view_state_reaches_the_mirror() {
        let engine = TestEngine::start(|_| {});
        let mut desk = engine.connect_desk();
        let mut snapshot = crate::frontend_ui::FrontendUiSnapshot::default();
        snapshot.open_plugins.insert(3);
        desk.publish_ui(snapshot);
        assert!(engine.app().frontend_ui.is_plugin_open(3));
        drop(desk);
        assert!(!engine.app().frontend_ui.is_plugin_open(3));
    }
}
