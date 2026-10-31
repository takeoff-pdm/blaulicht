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

use blaulicht_shared::ControlEventMessage;
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

/// An operator session: selection, selection stack, control buffer and scene
/// focus. The desk and every locally hosted plugin share [`SessionId::DESK`],
/// whose state stays in the existing `EngineState` fields. Other sessions are
/// added in 040.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SessionId(pub u32);

impl SessionId {
    pub const DESK: Self = Self(0);
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
}

impl Display for BusMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

/// The events plugins receive in `TickInput`, in bus order: the
/// `ControlEventMessage`s of every [`BusBody::Control`], with the envelope
/// stripped. Host commands are dropped.
pub fn plugin_events(messages: impl IntoIterator<Item = BusMessage>) -> Vec<ControlEventMessage> {
    messages
        .into_iter()
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
    shutdown: AtomicBool,
}

impl HubInner {
    fn lock(&self) -> MutexGuard<'_, HubState> {
        // Logging publishes here too, also from panicking threads.
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
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
