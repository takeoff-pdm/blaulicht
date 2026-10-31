//! Engine event hub: who sent a message, the envelope it travels in on the
//! bus, the engine's reply, and the system-message broadcast that every client
//! subscribes to.
//!
//! - **Bus messages** are host-only [`BusMessage`] envelopes. Their body is
//!   either a [`ControlEventMessage`] (forwarded to plugins exactly as before)
//!   or a [`HostCommand`] (host-only, never serialized to plugins). Plugins
//!   only ever see the plain `ControlEventMessage`s ([`plugin_events`]), so
//!   the envelope is not a plugin ABI change.
//! - **Replies:** after applying a message, the engine sends a
//!   [`CommandResult`] to the sending client only, as
//!   [`SystemMessage::CommandResult`] on its hub subscription.
//! - **System messages** (logs, startup stages, heartbeats, ...) go through
//!   the [`SystemHub`]. Every subscriber has its own bounded queue; a full
//!   queue drops its oldest messages and never blocks the publisher. A late
//!   subscriber first gets the replayable state: the current startup stages
//!   and the last log lines.
//! - A frontend holds one [`ClientConnection`]: its bus connection, its hub
//!   subscription and its view-state publisher under one [`ClientId`].
//!   Dropping it disconnects the client everywhere.

use std::{
    collections::{BTreeMap, VecDeque},
    fmt::{self, Display},
    sync::{
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
        Arc, Mutex, MutexGuard, PoisonError,
    },
    time::Duration,
};

use blaulicht_shared::{ControlEvent, ControlEventMessage, CONTROLS_REQUIRING_SELECTION};
use crossbeam_channel::{Receiver, Sender};
use strum::EnumCount;

use crate::{
    event::{EventBusHandle, SystemEventBusConnection},
    frontend_ui::{FrontendUiMirror, FrontendUiPublisher, FrontendUiSnapshot},
    msg::{StageStatus, StartupStage, SystemMessage, TERMINAL_ONLY_LOG_TARGET},
};

//
// Identities.
//

/// Who sent a bus message, and who receives its [`CommandResult`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ClientId {
    /// The native egui frontend running in the engine process.
    Desk,
    /// A WASM plugin, by plugin id (its index in `config.toml`).
    Plugin(u8),
    /// A client attached at runtime (e.g. a web frontend), allocated with
    /// [`SystemHub::allocate_client`].
    Remote(u32),
}

impl Display for ClientId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ClientId::Desk => write!(f, "desk"),
            ClientId::Plugin(id) => write!(f, "plugin {id}"),
            ClientId::Remote(id) => write!(f, "remote {id}"),
        }
    }
}

/// An operator session: selection, selection stack, control buffer, scene
/// focus and focused scene graph. The desk and every locally hosted plugin
/// share [`SessionId::DESK`], whose state stays in the existing `EngineState`
/// fields. Other sessions are opened with [`SystemHub::open_session`] and
/// live in the engine's side store (`dmx::session`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SessionId(pub u32);

impl SessionId {
    pub const DESK: Self = Self(0);
}

impl Display for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            SessionId::DESK => write!(f, "desk session"),
            SessionId(id) => write!(f, "session {id}"),
        }
    }
}

/// How a `ControlEvent` relates to the sender's operator session.
///
/// The engine applies every event with the sender's session swapped in. Other
/// consumers use the scope to ignore other operators' events: plugins only
/// see `DESK`'s session-scoped events ([`plugin_events`]), and the desk
/// ignores other sessions' events for its jog target and, if they only change
/// session state, for dirty marking.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SessionScope {
    /// Independent of the sender's session: show state, overrides, live mode,
    /// plugin and UI events.
    Global,
    /// Changes only the sender's session: its selection, selection stack,
    /// scene focus or focused scene graph.
    Session,
    /// Acts on the sender's selection and focused scene and writes its
    /// control buffer: it edits show state on behalf of that session.
    Control,
}

/// The [`SessionScope`] of `event`. A transaction takes the widest scope of
/// its events (`Control` over `Session` over `Global`).
pub fn session_scope(event: &ControlEvent) -> SessionScope {
    match event {
        // Selection, selection stack and focus.
        ControlEvent::SelectGroup(_)
        | ControlEvent::DeSelectGroup(_)
        | ControlEvent::LimitSelectionToFixtureInCurrentGroup(_)
        | ControlEvent::UnLimitSelectionToFixtureInCurrentGroup(_)
        | ControlEvent::RemoveSelection
        | ControlEvent::RemoveAllSelection
        | ControlEvent::PushSelection
        | ControlEvent::PopSelection
        | ControlEvent::SelectFolder(_)
        | ControlEvent::SetSceneFocus(_)
        | ControlEvent::SetFocusedSceneGraph(_) => SessionScope::Session,
        // Applied to the selection in the focused scene, through the control
        // buffer.
        CONTROLS_REQUIRING_SELECTION!() | ControlEvent::RemoveChange { .. } => {
            SessionScope::Control
        }
        ControlEvent::Transaction(events) => events
            .iter()
            .map(session_scope)
            .max()
            .unwrap_or(SessionScope::Global),
        // Live mode stays global here; 041 gives the preview an owner session.
        ControlEvent::MiscEvent { .. }
        | ControlEvent::SetOverlays(_)
        | ControlEvent::RemoveOverlayScene(_)
        | ControlEvent::AddOverlayScene(_)
        | ControlEvent::SetLiveMode(_)
        | ControlEvent::ToggleLiveMode
        | ControlEvent::SetSceneMasterAlpha(_, _)
        | ControlEvent::SetSceneMasterSpeed(_, _)
        | ControlEvent::SetChannelOverride(_, _, _)
        | ControlEvent::RemoveChannelOverride(_, _)
        | ControlEvent::MainUi(_)
        | ControlEvent::PluginUi(_, _)
        | ControlEvent::AnimationPluginUi(_, _, _)
        | ControlEvent::CreatePalette(_, _)
        | ControlEvent::UpdatePalette(_, _)
        | ControlEvent::DeletePalette(_)
        | ControlEvent::RenamePalette(_, _)
        | ControlEvent::UnassignPalette(_)
        | ControlEvent::SetSceneGraphEnabled(_, _)
        | ControlEvent::ToggleSceneGraphEnabled(_)
        | ControlEvent::CreateFolder { .. }
        | ControlEvent::RenameFolder(_, _)
        | ControlEvent::DeleteFolder(_)
        | ControlEvent::MoveToFolder { .. }
        | ControlEvent::ReorderFolder(_, _) => SessionScope::Global,
    }
}

/// Per-client sequence number of a sent message. The engine echoes it in the
/// [`CommandResult`] (and in rollbacks), so a client can match replies to
/// what it sent.
pub type CommandSeq = u64;

//
// Envelope.
//

/// Host-only operations. Unlike `ControlEvent`, these are never serialized to
/// plugins, so adding one is not a plugin ABI change. New UI-only operations
/// go here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostCommand {
    /// Does nothing; the engine answers `Ok`. Lets a client check its command
    /// path end to end.
    Ping,
}

#[derive(Clone, Debug)]
pub enum BusBody {
    /// Applied by the engine and forwarded to plugins unchanged.
    Control(ControlEventMessage),
    /// Applied by the engine; never forwarded to plugins.
    Host(HostCommand),
}

/// A message on the event bus. Host-only: plugins receive only the
/// `ControlEventMessage` inside a [`BusBody::Control`].
#[derive(Clone, Debug)]
pub struct BusMessage {
    pub client: ClientId,
    pub session: SessionId,
    pub seq: CommandSeq,
    pub body: BusBody,
}

impl BusMessage {
    pub fn control(
        client: ClientId,
        session: SessionId,
        seq: CommandSeq,
        message: ControlEventMessage,
    ) -> Self {
        Self {
            client,
            session,
            seq,
            body: BusBody::Control(message),
        }
    }

    /// The message as plugins see it: the `ControlEventMessage` of a
    /// [`BusBody::Control`], unchanged; nothing for a [`BusBody::Host`].
    pub fn plugin_event(&self) -> Option<&ControlEventMessage> {
        match &self.body {
            BusBody::Control(message) => Some(message),
            BusBody::Host(_) => None,
        }
    }

    pub fn into_plugin_event(self) -> Option<ControlEventMessage> {
        match self.body {
            BusBody::Control(message) => Some(message),
            BusBody::Host(_) => None,
        }
    }

    /// `true` if a consumer acting for `session` should treat this message
    /// as its own operator's: every message from `session` itself, and
    /// messages from other sessions whose event is [`SessionScope::Global`].
    /// Host commands count as global.
    pub fn concerns_session(&self, session: SessionId) -> bool {
        self.session == session
            || match &self.body {
                BusBody::Control(message) => session_scope(&message.body()) == SessionScope::Global,
                BusBody::Host(_) => true,
            }
    }
}

impl Display for BusMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

/// The events plugins receive in `TickInput`, in bus order: the
/// `ControlEventMessage`s of every [`BusBody::Control`], with the envelope
/// stripped. Host commands are dropped.
///
/// Plugins act for [`SessionId::DESK`], so other sessions' session-scoped
/// events are dropped too ([`BusMessage::concerns_session`]): a remote
/// operator's selection must not light up a controller's LEDs, for example.
pub fn plugin_events(messages: impl IntoIterator<Item = BusMessage>) -> Vec<ControlEventMessage> {
    messages
        .into_iter()
        .filter(|message| message.concerns_session(SessionId::DESK))
        .filter_map(BusMessage::into_plugin_event)
        .collect()
}

//
// Replies.
//

/// An id the engine allocated while applying a command, so the sender (for
/// example a dialog) learns it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CreatedId {
    Palette(u8),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommandOutcome {
    Ok {
        created: Option<CreatedId>,
    },
    /// The engine refused the command and changed nothing (a transaction is
    /// rolled back as a whole).
    Refused(String),
}

/// The engine's reply to one bus message, sent to its client only.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandResult {
    pub client: ClientId,
    pub seq: CommandSeq,
    pub outcome: CommandOutcome,
}

//
// System-message hub.
//

/// Most messages a subscriber's queue holds before its oldest are dropped.
/// The queue only allocates what it holds, so this can be generous: the desk
/// receives ~12 messages per second (tick speeds, heartbeats, audio devices)
/// and stops reading while its window is hidden and not repainting.
pub const SUBSCRIBER_CAPACITY: usize = 65536;
/// Log lines a late subscriber gets replayed.
pub const LOG_REPLAY_LEN: usize = 1000;

/// State a late subscriber needs to catch up: replayed on subscribe, before
/// any live message.
struct Replay {
    stages: [Option<StageStatus>; StartupStage::COUNT],
    initialization_complete: bool,
    audio_devices: Option<SystemMessage>,
    audio_selected: Option<SystemMessage>,
    logs: VecDeque<SystemMessage>,
    log_capacity: usize,
}

impl Replay {
    fn new(log_capacity: usize) -> Self {
        Self {
            stages: std::array::from_fn(|_| None),
            initialization_complete: false,
            audio_devices: None,
            audio_selected: None,
            logs: VecDeque::with_capacity(log_capacity),
            log_capacity,
        }
    }

    fn record(&mut self, message: &SystemMessage) {
        match message {
            SystemMessage::StartupStage { stage, status } => {
                self.stages[*stage as usize] = Some(status.clone());
            }
            SystemMessage::EngineInitializationComplete => self.initialization_complete = true,
            SystemMessage::AudioDevicesView(_) => self.audio_devices = Some(message.clone()),
            SystemMessage::AudioSelected(_) => self.audio_selected = Some(message.clone()),
            SystemMessage::Log { .. } | SystemMessage::WasmLog(_) => {
                if self.log_capacity == 0 {
                    return;
                }
                if self.logs.len() == self.log_capacity {
                    self.logs.pop_front();
                }
                self.logs.push_back(message.clone());
            }
            _ => {}
        }
    }

    fn messages(&self) -> impl Iterator<Item = SystemMessage> + '_ {
        use strum::IntoEnumIterator;
        let stages = StartupStage::iter()
            .zip(self.stages.iter())
            .filter_map(|(stage, status)| {
                status
                    .clone()
                    .map(|status| SystemMessage::StartupStage { stage, status })
            });
        stages
            .chain(
                self.initialization_complete
                    .then_some(SystemMessage::EngineInitializationComplete),
            )
            .chain(self.audio_devices.clone())
            .chain(self.audio_selected.clone())
            .chain(self.logs.iter().cloned())
    }
}

struct Subscriber {
    client: ClientId,
    /// Unbounded channel; [`Subscriber::push`] enforces the capacity, so
    /// nothing is allocated up front.
    sender: Sender<SystemMessage>,
    /// A second receiver of the subscriber's queue, used only to drop the
    /// oldest message when the queue is full.
    oldest: Receiver<SystemMessage>,
    /// Set on the first overflow, so it is logged once.
    overflowed: bool,
}

impl Subscriber {
    /// Queues `message`, first dropping the oldest queued message if the
    /// queue holds `capacity` messages. Never blocks. Returns `true` on this
    /// subscriber's first overflow.
    fn push(&mut self, message: SystemMessage, capacity: usize) -> bool {
        let mut first_overflow = false;
        // Pushes are serialized by the hub lock; the reader only shrinks the
        // queue.
        if self.sender.len() >= capacity {
            let _ = self.oldest.try_recv();
            first_overflow = !std::mem::replace(&mut self.overflowed, true);
        }
        // Cannot fail: `oldest` keeps the channel connected.
        let _ = self.sender.send(message);
        first_overflow
    }
}

struct HubState {
    next_subscriber: u64,
    subscribers: BTreeMap<u64, Subscriber>,
    replay: Replay,
}

struct HubInner {
    state: Mutex<HubState>,
    capacity: usize,
    next_remote: AtomicU32,
    /// Next [`SessionId`] to open; 0 is [`SessionId::DESK`].
    next_session: AtomicU32,
    /// Sessions opened and closed since the engine last took them. Kept apart
    /// from `state`, so logging never contends with it.
    session_changes: Mutex<Vec<SessionChange>>,
    shutdown: AtomicBool,
}

impl HubInner {
    fn lock(&self) -> MutexGuard<'_, HubState> {
        // Logging publishes here too, also from panicking threads.
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn lock_session_changes(&self) -> MutexGuard<'_, Vec<SessionChange>> {
        self.session_changes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

/// Broadcast hub for [`SystemMessage`]s. Cheap to clone; every clone publishes
/// to the same subscribers.
///
/// Never log through `tracing` while holding the hub lock: the log layer
/// publishes into the hub.
#[derive(Clone)]
pub struct SystemHub {
    inner: Arc<HubInner>,
}

impl Default for SystemHub {
    fn default() -> Self {
        Self::new()
    }
}

impl SystemHub {
    pub fn new() -> Self {
        Self::with_capacity(SUBSCRIBER_CAPACITY, LOG_REPLAY_LEN)
    }

    /// A hub whose subscribers each queue at most `subscriber_capacity`
    /// messages and that replays the last `log_replay_len` log lines (capped
    /// so a replay always fits into a fresh queue).
    pub fn with_capacity(subscriber_capacity: usize, log_replay_len: usize) -> Self {
        let capacity = subscriber_capacity.max(1);
        // Room for the stages, the init flag and the two audio messages.
        let fixed_replay = StartupStage::COUNT + 3;
        let log_replay_len = log_replay_len.min(capacity.saturating_sub(fixed_replay));
        Self {
            inner: Arc::new(HubInner {
                state: Mutex::new(HubState {
                    next_subscriber: 0,
                    subscribers: BTreeMap::new(),
                    replay: Replay::new(log_replay_len),
                }),
                capacity,
                next_remote: AtomicU32::new(0),
                next_session: AtomicU32::new(SessionId::DESK.0 + 1),
                session_changes: Mutex::new(Vec::new()),
                shutdown: AtomicBool::new(false),
            }),
        }
    }

    /// Sends `message` to every subscriber and records it for the replay.
    pub fn publish(&self, message: SystemMessage) {
        let capacity = self.inner.capacity;
        let overflowed: Vec<ClientId> = {
            let mut state = self.inner.lock();
            state.replay.record(&message);
            state
                .subscribers
                .values_mut()
                .filter_map(|subscriber| {
                    subscriber
                        .push(message.clone(), capacity)
                        .then_some(subscriber.client)
                })
                .collect()
        };
        warn_overflow(overflowed);
    }

    /// Sends `message` only to the subscribers of `client`; it is not
    /// replayed. Returns `false` if `client` has no subscriber.
    pub fn publish_to(&self, client: ClientId, message: SystemMessage) -> bool {
        let mut delivered = false;
        let mut overflowed = Vec::new();
        {
            let mut state = self.inner.lock();
            for subscriber in state.subscribers.values_mut() {
                if subscriber.client == client {
                    delivered = true;
                    if subscriber.push(message.clone(), self.inner.capacity) {
                        overflowed.push(client);
                    }
                }
            }
        }
        warn_overflow(overflowed);
        delivered
    }

    /// Subscribes `client`. The subscription first yields the replayable
    /// state, then every message published from now on.
    pub fn subscribe(&self, client: ClientId) -> HubSubscription {
        let (sender, receiver) = crossbeam_channel::unbounded();
        let mut state = self.inner.lock();
        for message in state.replay.messages() {
            let _ = sender.send(message);
        }
        let id = state.next_subscriber;
        state.next_subscriber += 1;
        state.subscribers.insert(
            id,
            Subscriber {
                client,
                sender,
                oldest: receiver.clone(),
                overflowed: false,
            },
        );
        HubSubscription {
            id,
            client,
            receiver,
            hub: Arc::clone(&self.inner),
        }
    }

    /// `true` if `client` has at least one subscription.
    pub fn is_subscribed(&self, client: ClientId) -> bool {
        self.inner
            .lock()
            .subscribers
            .values()
            .any(|subscriber| subscriber.client == client)
    }

    /// A fresh [`ClientId::Remote`] for a client attached at runtime.
    pub fn allocate_client(&self) -> ClientId {
        ClientId::Remote(self.inner.next_remote.fetch_add(1, Ordering::Relaxed))
    }

    /// Opens a new operator session with a fresh [`SessionId`] (ids are never
    /// reused). The engine creates its state (focused on `DESK`'s scene and
    /// scene graph, with an empty selection) before it applies any bus
    /// message sent after this returns. Dropping the handle closes the
    /// session.
    pub fn open_session(&self) -> SessionHandle {
        let id = SessionId(self.inner.next_session.fetch_add(1, Ordering::Relaxed));
        self.inner
            .lock_session_changes()
            .push(SessionChange::Opened(id));
        SessionHandle {
            id,
            hub: Arc::clone(&self.inner),
        }
    }

    /// Engine side: the sessions opened and closed since the last call, in
    /// order. Take them *after* draining the bus: a session is registered
    /// here before its first message is sent, so every session a drained
    /// message uses is then either known already or in the returned list.
    pub fn take_session_changes(&self) -> Vec<SessionChange> {
        std::mem::take(&mut *self.inner.lock_session_changes())
    }

    /// Asks the engine to stop. The supervisor checks this on every heartbeat;
    /// it no longer stops when nobody listens.
    pub fn shutdown(&self) {
        self.inner.shutdown.store(true, Ordering::Relaxed);
    }

    pub fn is_shut_down(&self) -> bool {
        self.inner.shutdown.load(Ordering::Relaxed)
    }
}

/// Logs the first overflow of each subscriber. Called after the hub lock is
/// released, and terminal only: the Logs page would publish it back here.
fn warn_overflow(clients: Vec<ClientId>) {
    for client in clients {
        tracing::warn!(
            target: TERMINAL_ONLY_LOG_TARGET,
            "System hub: subscriber {client} is not keeping up; dropping its oldest messages (logged once)"
        );
    }
}

/// One subscription to the [`SystemHub`]. Dropping it unsubscribes.
pub struct HubSubscription {
    id: u64,
    client: ClientId,
    receiver: Receiver<SystemMessage>,
    hub: Arc<HubInner>,
}

impl HubSubscription {
    pub fn client(&self) -> ClientId {
        self.client
    }

    pub fn try_recv(&self) -> Option<SystemMessage> {
        self.receiver.try_recv().ok()
    }

    /// Waits up to `timeout` for the next message.
    pub fn recv_timeout(&self, timeout: Duration) -> Option<SystemMessage> {
        self.receiver.recv_timeout(timeout).ok()
    }

    /// Messages waiting in this subscription's queue.
    pub fn len(&self) -> usize {
        self.receiver.len()
    }

    pub fn is_empty(&self) -> bool {
        self.receiver.is_empty()
    }
}

impl Drop for HubSubscription {
    fn drop(&mut self) {
        self.hub.lock().subscribers.remove(&self.id);
    }
}

//
// Sessions.
//

/// A session opened or closed through the hub, for the engine to apply.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionChange {
    Opened(SessionId),
    Closed(SessionId),
}

/// An open operator session, from [`SystemHub::open_session`]. Dropping it
/// closes the session: on its next tick the engine applies the messages it
/// has received, then drops the session's state. Messages from the session
/// that arrive later are refused.
#[must_use = "dropping the handle closes the session"]
pub struct SessionHandle {
    id: SessionId,
    hub: Arc<HubInner>,
}

impl SessionHandle {
    pub fn id(&self) -> SessionId {
        self.id
    }
}

impl Drop for SessionHandle {
    fn drop(&mut self) {
        self.hub
            .lock_session_changes()
            .push(SessionChange::Closed(self.id));
    }
}

//
// Client connection.
//

/// Everything one frontend holds to talk to the engine, under one
/// [`ClientId`]: a bus connection, a hub subscription and its view-state
/// publisher ([`FrontendUiMirror`]). The three share its lifetime: dropping
/// the connection disconnects the client from the bus, unsubscribes it and
/// clears its view-state mirror entries.
pub struct ClientConnection {
    id: ClientId,
    session: SessionId,
    next_seq: AtomicU64,
    bus: SystemEventBusConnection<BusMessage>,
    system: HubSubscription,
    ui: FrontendUiPublisher,
    /// The session this connection opened, if any; closed on drop, after
    /// the bus connection.
    _session: Option<SessionHandle>,
}

impl ClientConnection {
    pub fn connect(
        id: ClientId,
        session: SessionId,
        hub: &SystemHub,
        bus: &EventBusHandle<BusMessage>,
        mirror: &FrontendUiMirror,
    ) -> Self {
        Self {
            id,
            session,
            next_seq: AtomicU64::new(1),
            bus: bus.connect(),
            system: hub.subscribe(id),
            ui: mirror.connect(id),
            _session: None,
        }
    }

    /// Like [`Self::connect`], but in a new operator session of its own
    /// ([`SystemHub::open_session`]), which closes when the connection is
    /// dropped. This is how a remote client attaches.
    pub fn connect_new_session(
        id: ClientId,
        hub: &SystemHub,
        bus: &EventBusHandle<BusMessage>,
        mirror: &FrontendUiMirror,
    ) -> Self {
        let session = hub.open_session();
        let connection = Self::connect(id, session.id(), hub, bus, mirror);
        Self {
            _session: Some(session),
            ..connection
        }
    }

    pub fn id(&self) -> ClientId {
        self.id
    }

    pub fn session(&self) -> SessionId {
        self.session
    }

    /// Wraps `body` in this client's envelope and sends it. Returns the
    /// sequence number its [`CommandResult`] will carry.
    pub fn send_body(&self, body: BusBody) -> CommandSeq {
        let seq = self.next_seq.fetch_add(1, Ordering::Relaxed);
        self.bus.send(BusMessage {
            client: self.id,
            session: self.session,
            seq,
            body,
        });
        seq
    }

    /// Sends a `ControlEvent` (forwarded to plugins as well).
    pub fn send(&self, message: ControlEventMessage) -> CommandSeq {
        self.send_body(BusBody::Control(message))
    }

    /// Sends a host-only command.
    pub fn send_host(&self, command: HostCommand) -> CommandSeq {
        self.send_body(BusBody::Host(command))
    }

    /// Next message from the bus (every client's messages, this one's too).
    pub fn try_recv_bus(&self) -> Option<BusMessage> {
        self.bus.try_recv()
    }

    /// Next system message: broadcasts, plus replies and requests addressed to
    /// this client.
    pub fn try_recv_system(&self) -> Option<SystemMessage> {
        self.system.try_recv()
    }

    /// Publishes this frontend's view-state snapshot; cheap to call every
    /// frame (only changes are sent).
    pub fn publish_ui(&mut self, snapshot: FrontendUiSnapshot) {
        self.ui.publish(snapshot);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::msg::StageState;
    use blaulicht_shared::{
        CollectedAudioSnapshot, ControlEvent, ControlEventCollection, EventOriginator, LogLevel,
        TickInput,
    };
    use std::time::Instant;

    fn log(message: &str) -> SystemMessage {
        SystemMessage::Log {
            message: message.to_string(),
            level: LogLevel::Info,
            source: "Test".into(),
        }
    }

    fn drain(subscription: &HubSubscription) -> Vec<String> {
        std::iter::from_fn(|| subscription.try_recv())
            .map(|message| match message {
                SystemMessage::Log { message, .. } => format!("log:{message}"),
                SystemMessage::StartupStage { stage, status } => {
                    format!("stage:{stage:?}:{:?}", status.state)
                }
                SystemMessage::EngineInitializationComplete => "complete".to_string(),
                SystemMessage::CommandResult(result) => {
                    format!(
                        "result:{}:{}:{:?}",
                        result.client, result.seq, result.outcome
                    )
                }
                SystemMessage::Heartbeat(n) => format!("heartbeat:{n}"),
                _ => "other".to_string(),
            })
            .collect()
    }

    #[test]
    fn two_subscribers_both_receive_every_log_line_and_stage() {
        let hub = SystemHub::new();
        let desk = hub.subscribe(ClientId::Desk);
        let remote = hub.subscribe(ClientId::Remote(0));

        hub.publish(log("one"));
        hub.publish(SystemMessage::StartupStage {
            stage: StartupStage::Config,
            status: StageStatus::ok(""),
        });
        hub.publish(log("two"));

        let expected = vec!["log:one", "stage:Config:Ok", "log:two"];
        assert_eq!(drain(&desk), expected);
        assert_eq!(drain(&remote), expected);
    }

    #[test]
    fn a_late_subscriber_gets_the_replay_then_live_messages() {
        let hub = SystemHub::with_capacity(64, 2);
        let _early = hub.subscribe(ClientId::Desk);

        hub.publish(log("old"));
        hub.publish(SystemMessage::StartupStage {
            stage: StartupStage::Plugins,
            status: StageStatus::running("1/2"),
        });
        hub.publish(SystemMessage::StartupStage {
            stage: StartupStage::Config,
            status: StageStatus::ok(""),
        });
        hub.publish(log("kept 1"));
        hub.publish(SystemMessage::Heartbeat(1));
        hub.publish(SystemMessage::StartupStage {
            stage: StartupStage::Plugins,
            status: StageStatus::ok("2/2"),
        });
        hub.publish(SystemMessage::EngineInitializationComplete);
        hub.publish(log("kept 2"));

        let late = hub.subscribe(ClientId::Remote(0));
        hub.publish(log("live"));

        // Stages in stage order with their latest status, the init flag, then
        // the last two log lines. Heartbeats are not replayed.
        assert_eq!(
            drain(&late),
            vec![
                "stage:Config:Ok",
                "stage:Plugins:Ok",
                "complete",
                "log:kept 1",
                "log:kept 2",
                "log:live",
            ]
        );
    }

    #[test]
    fn a_full_subscriber_does_not_block_the_publisher() {
        let hub = SystemHub::with_capacity(8, 0);
        let stuck = hub.subscribe(ClientId::Remote(0));
        let reader = hub.subscribe(ClientId::Desk);

        let publisher = {
            let hub = hub.clone();
            std::thread::spawn(move || {
                for i in 0..10_000 {
                    hub.publish(SystemMessage::Heartbeat(i));
                }
            })
        };
        let deadline = Instant::now() + Duration::from_secs(10);
        while !publisher.is_finished() {
            assert!(Instant::now() < deadline, "publisher blocked");
            // The reader keeps reading while the stuck subscriber never does.
            let _ = reader.try_recv();
        }
        publisher.join().unwrap();

        // The stuck subscriber kept the newest messages.
        let kept = drain(&stuck);
        assert_eq!(kept.len(), 8);
        assert_eq!(kept.last().map(String::as_str), Some("heartbeat:9999"));
        assert_eq!(kept.first().map(String::as_str), Some("heartbeat:9992"));
    }

    #[test]
    fn targeted_messages_reach_only_their_client() {
        let hub = SystemHub::new();
        let desk = hub.subscribe(ClientId::Desk);
        let remote = hub.subscribe(ClientId::Remote(3));

        let result = CommandResult {
            client: ClientId::Remote(3),
            seq: 7,
            outcome: CommandOutcome::Refused("No selected object(s)".to_string()),
        };
        assert!(hub.publish_to(
            ClientId::Remote(3),
            SystemMessage::CommandResult(result.clone())
        ));
        assert!(!hub.publish_to(ClientId::Plugin(1), SystemMessage::CommandResult(result)));

        assert!(drain(&desk).is_empty());
        assert_eq!(
            drain(&remote),
            vec![r#"result:remote 3:7:Refused("No selected object(s)")"#]
        );

        // Targeted messages are not replayed.
        let late = hub.subscribe(ClientId::Remote(3));
        assert!(drain(&late).is_empty());
    }

    #[test]
    fn dropping_a_subscription_unsubscribes() {
        let hub = SystemHub::new();
        let desk = hub.subscribe(ClientId::Desk);
        assert!(hub.is_subscribed(ClientId::Desk));
        drop(desk);
        assert!(!hub.is_subscribed(ClientId::Desk));
        assert!(!hub.publish_to(ClientId::Desk, SystemMessage::Heartbeat(0)));
    }

    #[test]
    fn replayed_stage_status_is_the_latest() {
        let hub = SystemHub::new();
        hub.publish(SystemMessage::StartupStage {
            stage: StartupStage::MainLoop,
            status: StageStatus::running("starting engine"),
        });
        hub.publish(SystemMessage::StartupStage {
            stage: StartupStage::MainLoop,
            status: StageStatus::restarting("restarting after crash"),
        });
        let late = hub.subscribe(ClientId::Desk);
        match late.try_recv() {
            Some(SystemMessage::StartupStage { stage, status }) => {
                assert_eq!(stage, StartupStage::MainLoop);
                assert_eq!(status.state, StageState::Restarting);
            }
            _ => panic!("expected the MainLoop stage"),
        }
        assert!(late.try_recv().is_none());
    }

    #[test]
    fn client_connection_stamps_its_envelope_and_disconnects_everywhere_on_drop() {
        let hub = SystemHub::new();
        let bus = crate::event::SystemEventBus::<BusMessage>::new();
        let mirror = FrontendUiMirror::new();
        let client = hub.allocate_client();
        let connection =
            ClientConnection::connect(client, SessionId::DESK, &hub, &bus.handle(), &mirror);
        assert!(hub.is_subscribed(client));
        assert!(mirror.is_connected(client));

        let first = connection.send(ControlEventMessage::new(
            EventOriginator::Web,
            ControlEvent::SelectGroup(1),
        ));
        let second = connection.send_host(HostCommand::Ping);
        assert_eq!((first, second), (1, 2));

        drop(connection);
        assert!(!hub.is_subscribed(client));
        assert!(!mirror.is_connected(client));
        assert_eq!(bus.handle().connection_count(), 0);
    }

    /// The session-scoped events, written out: selection, focus and the
    /// control-buffer events. Everything else is global.
    #[test]
    fn session_scoped_events_are_selection_focus_and_control_events() {
        use blaulicht_shared::{
            scene::FixtureSelection, AnimationSpeedModifier, FixtureProperty, MainUiEvent,
        };

        let session = [
            ControlEvent::SelectGroup(0),
            ControlEvent::DeSelectGroup(0),
            ControlEvent::LimitSelectionToFixtureInCurrentGroup(0),
            ControlEvent::UnLimitSelectionToFixtureInCurrentGroup(0),
            ControlEvent::RemoveSelection,
            ControlEvent::RemoveAllSelection,
            ControlEvent::PushSelection,
            ControlEvent::PopSelection,
            ControlEvent::SelectFolder(0),
            ControlEvent::SetSceneFocus(0),
            ControlEvent::SetFocusedSceneGraph(None),
        ];
        let control = [
            ControlEvent::SetEnabled(true),
            ControlEvent::SetAlpha(0),
            ControlEvent::SetStrobeSpeed(0),
            ControlEvent::SetFocus(0),
            ControlEvent::SetTilt(0),
            ControlEvent::SetPan(0),
            ControlEvent::SetColor((0, 0, 0)),
            ControlEvent::SetColorHue(0),
            ControlEvent::SetColorSaturation(0),
            ControlEvent::SetColorValue(0),
            ControlEvent::AddToProperty(FixtureProperty::Alpha, 1),
            ControlEvent::AddAnimation(0),
            ControlEvent::RemoveAnimation(0),
            ControlEvent::ResetAnimation(0),
            ControlEvent::PauseAnimation(0),
            ControlEvent::PlayAnimation(0),
            ControlEvent::SetAnimationSpeed(0, AnimationSpeedModifier::from_index(0)),
            ControlEvent::AssignPaletteToProperty(FixtureProperty::Alpha, 0),
            ControlEvent::UnassignPaletteFromProperty(FixtureProperty::Alpha),
            ControlEvent::AssignPaletteToSelection(0),
            ControlEvent::UnassignPaletteFromSelection(0),
            ControlEvent::RemoveChange {
                selection: FixtureSelection { fixtures: vec![] },
                property: FixtureProperty::Alpha,
            },
        ];
        let global = [
            ControlEvent::MiscEvent {
                descriptor: 0,
                value: 0,
            },
            ControlEvent::SetOverlays(vec![]),
            ControlEvent::AddOverlayScene(0),
            ControlEvent::RemoveOverlayScene(0),
            ControlEvent::SetLiveMode(true),
            ControlEvent::ToggleLiveMode,
            ControlEvent::SetSceneMasterAlpha(0, 100),
            ControlEvent::SetChannelOverride(0, 1, 255),
            ControlEvent::RemoveChannelOverride(0, 1),
            ControlEvent::MainUi(MainUiEvent::NumberpadAdjust { delta: 1 }),
            ControlEvent::DeletePalette(0),
            ControlEvent::ToggleSceneGraphEnabled(0),
            ControlEvent::DeleteFolder(0),
            ControlEvent::Transaction(vec![]),
        ];
        for event in &session {
            assert_eq!(session_scope(event), SessionScope::Session, "{event:?}");
        }
        for event in &control {
            assert_eq!(session_scope(event), SessionScope::Control, "{event:?}");
        }
        for event in &global {
            assert_eq!(session_scope(event), SessionScope::Global, "{event:?}");
        }

        // A transaction takes its widest scope.
        let transaction =
            |events: Vec<ControlEvent>| session_scope(&ControlEvent::Transaction(events));
        assert_eq!(
            transaction(vec![
                ControlEvent::SetChannelOverride(0, 1, 2),
                ControlEvent::SelectGroup(0),
            ]),
            SessionScope::Session
        );
        assert_eq!(
            transaction(vec![
                ControlEvent::SelectGroup(0),
                ControlEvent::SetAlpha(1),
            ]),
            SessionScope::Control
        );
    }

    /// Plugins act for the desk session: another session's selection,
    /// focus and control events don't reach them, its global events do.
    #[test]
    fn plugins_only_see_desk_session_events_and_global_ones() {
        let remote = SessionId(3);
        let message = |session, event| {
            BusMessage::control(
                ClientId::Remote(0),
                session,
                1,
                ControlEventMessage::new(EventOriginator::Web, event),
            )
        };
        let bus = vec![
            message(remote, ControlEvent::SelectGroup(1)),
            message(SessionId::DESK, ControlEvent::SelectGroup(2)),
            message(remote, ControlEvent::SetColor((1, 2, 3))),
            message(remote, ControlEvent::SetChannelOverride(0, 1, 9)),
            message(
                remote,
                ControlEvent::Transaction(vec![ControlEvent::SetSceneFocus(1)]),
            ),
            message(SessionId::DESK, ControlEvent::SetSceneFocus(1)),
        ];
        assert!(!bus[0].concerns_session(SessionId::DESK));
        assert!(bus[0].concerns_session(remote));
        // The other way round, too.
        assert!(!bus[1].concerns_session(remote));
        assert!(bus[3].concerns_session(SessionId::DESK));

        let seen: Vec<String> = plugin_events(bus)
            .iter()
            .map(|event| format!("{:?}", event.body()))
            .collect();
        assert_eq!(
            seen,
            vec![
                "SelectGroup(2)",
                "SetChannelOverride(0, 1, 9)",
                "SetSceneFocus(1)"
            ]
        );
    }

    #[test]
    fn sessions_open_and_close_in_order_with_fresh_ids() {
        let hub = SystemHub::new();
        let first = hub.open_session();
        let second = hub.open_session();
        assert_ne!(first.id(), SessionId::DESK);
        assert!(second.id() > first.id());
        let (first_id, second_id) = (first.id(), second.id());
        drop(first);
        assert_eq!(
            hub.take_session_changes(),
            vec![
                SessionChange::Opened(first_id),
                SessionChange::Opened(second_id),
                SessionChange::Closed(first_id),
            ]
        );
        assert!(hub.take_session_changes().is_empty());

        // A client in a session of its own closes it when it drops.
        let bus = crate::event::SystemEventBus::<BusMessage>::new();
        let mirror = FrontendUiMirror::new();
        let client = hub.allocate_client();
        let connection =
            ClientConnection::connect_new_session(client, &hub, &bus.handle(), &mirror);
        let session = connection.session();
        assert!(session > second_id);
        drop(connection);
        assert_eq!(
            hub.take_session_changes(),
            vec![
                SessionChange::Opened(session),
                SessionChange::Closed(session)
            ]
        );
    }

    /// Plugins must receive exactly the `ControlEventMessage`s they did before
    /// the envelope existed: same events, same order, same bytes.
    #[test]
    fn plugin_tick_input_is_unchanged_by_the_envelope() {
        let desk = ControlEventMessage::new(EventOriginator::Web, ControlEvent::SelectGroup(3));
        let plugin = ControlEventMessage::new(EventOriginator::Plugin, ControlEvent::SetAlpha(200));
        let rollback =
            ControlEventMessage::new(EventOriginator::DmxEngine, ControlEvent::DeSelectGroup(9));
        let bus = vec![
            BusMessage::control(ClientId::Desk, SessionId::DESK, 1, desk.clone()),
            BusMessage {
                client: ClientId::Remote(0),
                session: SessionId(5),
                seq: 1,
                body: BusBody::Host(HostCommand::Ping),
            },
            BusMessage::control(ClientId::Plugin(2), SessionId::DESK, 4, plugin.clone()),
            BusMessage::control(ClientId::Plugin(2), SessionId::DESK, 5, rollback.clone()),
        ];

        let tick_input = |events| TickInput {
            id: 1,
            clock: 1234,
            initial: false,
            audio_data: CollectedAudioSnapshot::default(),
            events: ControlEventCollection { events },
        };
        let via_envelope = tick_input(plugin_events(bus));
        let before_049 = tick_input(vec![desk, plugin, rollback]);
        assert_eq!(via_envelope.serialize(), before_049.serialize());

        // The events' wire bytes, pinned: `[len, (originator, event...)...]`.
        assert_eq!(
            via_envelope.events.serialize(),
            vec![
                3, //
                0, 0, 3, // Web, SelectGroup(3)
                2, 9, 200, // Plugin, SetAlpha(200)
                1, 1, 9, // DmxEngine, DeSelectGroup(9)
            ]
        );
    }
}
