//! The remote client protocol: what a frontend and the engine's websocket
//! server ([`crate::server`]) exchange. [`crate::client::RemoteClient`] is
//! the Rust client.
//!
//! # Connection
//!
//! 1. The client opens a websocket to `ws://<engine host>:<port>/` (`port`
//!    from the engine's `config.toml`, any path).
//! 2. Its first message is [`ClientMessage::Hello`]. The frame type picks the
//!    encoding for the whole connection, both ways: a **binary** frame means
//!    bincode ([`Encoding::Bincode`]), a **text** frame JSON
//!    ([`Encoding::Json`]).
//! 3. The server answers [`ServerMessage::Welcome`] with the client's
//!    [`ClientId`] and its own new operator [`SessionId`], or
//!    [`ServerMessage::Rejected`] (wrong [`PROTOCOL_VERSION`], missing or wrong
//!    passcode, too many clients) and closes the connection.
//! 4. Then the server sends, as things change: the show state
//!    ([`ServerMessage::Show`]), the stage ([`ServerMessage::Stage`]), the
//!    client's session ([`ServerMessage::Session`]), every system message
//!    (logs, startup stages, tick speeds, showfile status, ...; first the
//!    replay a late subscriber gets: startup stages, the last log lines, the
//!    latest audio devices and showfile status), the results of the client's
//!    commands, and the [`Stream`]s it subscribed to.
//! 5. The client sends commands ([`ClientMessage::Command`]) with its own
//!    sequence numbers; each gets a [`ServerMessage::Result`] with that
//!    number. A command's effect on the client's session arrives before its
//!    result; the show state may follow up to one snapshot interval later. It can change its streams ([`ClientMessage::Subscribe`]) and
//!    report which plugin UIs it shows ([`ClientMessage::PublishUi`]).
//!
//! Closing the connection closes the client's session and drops what it
//! reported about its view state.
//!
//! # Encodings
//!
//! - **bincode**: `bincode` 2 in serde mode with `config::standard()`
//!   ([`Encoding::encode`]). The show state inside [`ServerMessage::Show`] is
//!   a byte string holding the plugin ABI's bincode bytes of
//!   `blaulicht_shared::EngineState` (`EngineState::serialize`), the same
//!   bytes plugins get every tick.
//! - **JSON**: serde's externally tagged form (`{"Command":{"seq":1,...}}`,
//!   unit variants as strings). Server messages are written so that maps
//!   whose keys aren't strings or numbers stay representable: such a key is
//!   the JSON text of the key (the override of universe 0, channel 5 is
//!   `"[0,5]"`). The show state is the `EngineState` object itself. Floats
//!   that are not finite are `null`. The Rust client only speaks bincode.
//!
//! # Version
//!
//! [`PROTOCOL_VERSION`] is separate from the plugin ABI. Bump it whenever a
//! message type here changes, or `ControlEvent`, `HostCommand`,
//! `blaulicht_shared::EngineState` or a stream's payload type.

use std::{
    collections::{BTreeSet, HashMap},
    net::SocketAddr,
    path::PathBuf,
    sync::Arc,
};

use blaulicht_audio_engine::{AudioBucket, SignalCollectorParams};
use blaulicht_shared::{CollectedAudioSnapshot, LogLevel};
use serde::{de::DeserializeOwned, Deserialize, Deserializer, Serialize, Serializer};

use crate::{
    audio::defs::AudioThreadControlSignal,
    dmx::OperatorSession,
    hub::{BusBody, BusMessage, ClientId, CommandResult, CommandSeq, SessionId},
    msg::{StageStatus, StartupStage, SystemMessage, TickSpeeds, WasmLogBody},
    persistence::ShowfileStatus,
    stage::StageScene,
    state::{AppHealthState, ArtNetReceiver, PluginRuntimeKind, PluginState},
    ui_ops::WasmUiOp,
};

/// The protocol version a client states in [`ClientMessage::Hello`]. The
/// server rejects any other.
pub const PROTOCOL_VERSION: u32 = 5;

/// How the messages of one connection are encoded; chosen by the frame type
/// of the client's [`ClientMessage::Hello`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Encoding {
    /// Binary frames, bincode 2 (serde mode, `config::standard()`).
    Bincode,
    /// Text frames, JSON.
    Json,
}

impl std::fmt::Display for Encoding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Encoding::Bincode => write!(f, "bincode"),
            Encoding::Json => write!(f, "JSON"),
        }
    }
}

/// A websocket message body: what [`Encoding::encode`] produces and
/// [`Encoding::decode`] reads.
#[derive(Clone, Debug, PartialEq)]
pub enum Frame {
    Binary(Vec<u8>),
    Text(String),
}

impl Frame {
    pub fn len(&self) -> usize {
        match self {
            Frame::Binary(bytes) => bytes.len(),
            Frame::Text(text) => text.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The encoding a frame of this type carries.
    pub fn encoding(&self) -> Encoding {
        match self {
            Frame::Binary(_) => Encoding::Bincode,
            Frame::Text(_) => Encoding::Json,
        }
    }
}

impl Encoding {
    /// Encodes `message` as a frame of this encoding. JSON keeps maps with
    /// composite keys representable (see the module docs).
    pub fn encode<T: Serialize + ?Sized>(self, message: &T) -> anyhow::Result<Frame> {
        Ok(match self {
            Encoding::Bincode => Frame::Binary(bincode::serde::encode_to_vec(
                message,
                bincode::config::standard(),
            )?),
            Encoding::Json => Frame::Text(super::json::to_string(message)?),
        })
    }

    /// Decodes a frame; its type (binary or text) decides the encoding.
    pub fn decode<T: DeserializeOwned>(frame: &Frame) -> anyhow::Result<T> {
        Ok(match frame {
            Frame::Binary(bytes) => {
                let (message, _) =
                    bincode::serde::decode_from_slice(bytes, bincode::config::standard())?;
                message
            }
            Frame::Text(text) => serde_json::from_str(text)?,
        })
    }
}

/// The bincode bytes of `value` (serde mode), for telling whether a stream's
/// payload changed.
pub(crate) fn fingerprint<T: Serialize + ?Sized>(value: &T) -> Vec<u8> {
    bincode::serde::encode_to_vec(value, bincode::config::standard()).unwrap_or_default()
}

//
// Client to server.
//

/// A message from a remote client.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ClientMessage {
    /// The first message. `protocol_version` must be [`PROTOCOL_VERSION`].
    Hello {
        protocol_version: u32,
        streams: Vec<Stream>,
        /// The engine's passcode, if it has one (`server_passcode` in
        /// `config.toml`). Sent as is: the connection is not encrypted.
        passcode: Option<String>,
    },
    /// A command in the client's session: a `ControlEvent` (also forwarded to
    /// plugins) or a `HostCommand`. `seq` is the client's own number for it;
    /// the [`ServerMessage::Result`] carries it back. The server stamps
    /// control events `EventOriginator::Web`. Desk-only host commands
    /// (`HostCommand::is_desk_only`) are refused.
    Command { seq: CommandSeq, body: BusBody },
    /// Replaces the set of streams the client receives.
    Subscribe { streams: Vec<Stream> },
    /// The plugins whose UI the client currently shows. Plugins read it
    /// (`ui_is_open`); it is dropped when the client disconnects.
    PublishUi { open_plugins: BTreeSet<u8> },
}

/// Live streams a client can subscribe to. Each is throttled by the server
/// and only sent when it changed. System messages (logs, startup stages,
/// tick speeds, ...), the show state, the stage and the client's session
/// are always sent.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Stream {
    /// [`ServerMessage::Dmx`]: the DMX universes that changed, up to 30 Hz.
    Dmx,
    /// [`ServerMessage::Health`]: DMX, Art-Net, MIDI and serial health, up
    /// to 2 Hz.
    Health,
    /// [`ServerMessage::Spectrogram`]: new spectrogram columns, up to 20 Hz.
    Spectrogram,
    /// [`ServerMessage::PluginUi`]: every plugin's UI ops, up to 20 Hz.
    PluginUi,
    /// [`ServerMessage::AnimationEditorUi`]: the UI ops of every open WASM
    /// animation editor, up to 20 Hz.
    AnimationEditorUi,
    /// [`ServerMessage::Status`]: grand master, main loop state, audio
    /// parameters, Art-Net receivers and plugins, up to 2 Hz.
    Status,
    /// [`ServerMessage::Bus`]: every message on the event bus (every
    /// client's commands), as they happen.
    Bus,
}

impl Stream {
    pub const ALL: [Stream; 7] = [
        Stream::Dmx,
        Stream::Health,
        Stream::Spectrogram,
        Stream::PluginUi,
        Stream::AnimationEditorUi,
        Stream::Status,
        Stream::Bus,
    ];
}

//
// Server to client.
//

/// A message from the engine's server.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ServerMessage {
    /// The answer to [`ClientMessage::Hello`]: who the client is and the
    /// operator session it acts in.
    Welcome {
        protocol_version: u32,
        client: ClientId,
        session: SessionId,
        /// The engine's version (`CARGO_PKG_VERSION`).
        engine_version: String,
    },
    /// The server refused the connection and closes it.
    Rejected {
        protocol_version: u32,
        reason: String,
        /// The passcode was missing or wrong: ask for it rather than retry.
        passcode_required: bool,
    },
    /// The show state, sent when it changed, at most 10 times a second.
    Show(ShowSnapshot),
    /// The visualizer's stage and the open showfile, sent when either
    /// changed.
    Stage(Box<StageSnapshot>),
    /// The client's operator session, sent after every change.
    Session(SessionSnapshot),
    /// A system message (log line, startup stage, tick speeds, showfile
    /// status, ...).
    System(WireSystemMessage),
    /// The result of one of the client's commands.
    Result(CommandResult),
    /// A message on the event bus ([`Stream::Bus`]).
    Bus(BusMessage),
    /// DMX universes that changed ([`Stream::Dmx`]).
    Dmx(Vec<DmxUniverse>),
    Health(Box<AppHealthState>),
    Spectrogram(SpectrogramUpdate),
    /// Every plugin's latest UI ops, by plugin id ([`Stream::PluginUi`]).
    PluginUi(HashMap<u8, Vec<WasmUiOp>>),
    /// Every open WASM animation editor's UI ops, by instance id
    /// ([`Stream::AnimationEditorUi`]).
    AnimationEditorUi(HashMap<u64, Vec<WasmUiOp>>),
    Status(Box<EngineStatus>),
}

/// The shared show state (what plugins see: groups, scenes, palettes, views,
/// templates, overrides, scene graphs, folders, and the desk session's
/// selection and focus), identified by `digest` (FNV-1a over its bincode
/// bytes, as the plugin tick computes it).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ShowSnapshot {
    pub digest: u64,
    pub state: ShowState,
}

/// The show state in a [`ShowSnapshot`]: bincode carries the plugin ABI's
/// bytes as a byte string, JSON the state as an object.
#[derive(Clone, Debug)]
pub enum ShowState {
    /// `blaulicht_shared::EngineState::serialize()` bytes.
    Encoded(Arc<Vec<u8>>),
    Decoded(Box<blaulicht_shared::EngineState>),
}

impl ShowState {
    /// The decoded state.
    pub fn into_state(self) -> anyhow::Result<blaulicht_shared::EngineState> {
        match self {
            ShowState::Decoded(state) => Ok(*state),
            ShowState::Encoded(bytes) => decode_engine_state(&bytes),
        }
    }
}

/// Decodes the plugin ABI's bincode bytes of the shared engine state
/// (`EngineState::deserialize` panics on bad input; this doesn't).
pub fn decode_engine_state(bytes: &[u8]) -> anyhow::Result<blaulicht_shared::EngineState> {
    let (state, _) = bincode::decode_from_slice(bytes, bincode::config::standard())?;
    Ok(state)
}

impl Serialize for ShowState {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::Error;
        if serializer.is_human_readable() {
            match self {
                ShowState::Decoded(state) => Serialize::serialize(&**state, serializer),
                ShowState::Encoded(bytes) => Serialize::serialize(
                    &decode_engine_state(bytes).map_err(S::Error::custom)?,
                    serializer,
                ),
            }
        } else {
            match self {
                ShowState::Encoded(bytes) => serializer.serialize_bytes(bytes),
                // The plugin ABI's bytes (`EngineState::serialize`).
                ShowState::Decoded(state) => {
                    serializer.serialize_bytes(&blaulicht_shared::EngineState::serialize(state))
                }
            }
        }
    }
}

impl<'de> Deserialize<'de> for ShowState {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        if deserializer.is_human_readable() {
            // Composite map keys are JSON text there, which `EngineState`'s
            // own `Deserialize` doesn't read back.
            return Err(D::Error::custom(
                "a JSON show state is for non-Rust clients; use bincode",
            ));
        }
        let bytes = serde_as::bytes::deserialize(deserializer)?;
        decode_engine_state(&bytes)
            .map(|state| ShowState::Decoded(Box::new(state)))
            .map_err(D::Error::custom)
    }
}

/// The stage and the open showfile ([`ServerMessage::Stage`]).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StageSnapshot {
    /// Changes whenever the stage does (`EngineState::stage_revision`).
    pub revision: u64,
    /// The open showfile on the engine host, if any.
    pub showfile: Option<PathBuf>,
    pub scene: StageScene,
}

/// The client's operator session ([`ServerMessage::Session`]).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SessionSnapshot {
    pub session: SessionId,
    pub state: OperatorSession,
    /// The session whose focus the live preview renders, if one is live.
    pub live_owner: Option<SessionId>,
}

/// One DMX universe's output ([`ServerMessage::Dmx`]): 513 bytes, index 0 is
/// the start code, channels are 1..=512.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DmxUniverse {
    pub universe: u8,
    pub channels: Vec<u8>,
}

/// New spectrogram columns ([`ServerMessage::Spectrogram`]).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SpectrogramUpdate {
    /// Drop the columns received so far first (the first update, or the
    /// engine's spectrogram started over).
    pub reset: bool,
    pub max_columns: u32,
    pub bin_count: u32,
    pub tick_period_ms: u32,
    /// The span of time the engine keeps (`AudioSpectrogram::window`).
    pub window_ms: Option<u32>,
    /// Oldest first.
    pub columns: Vec<SpectrogramColumn>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SpectrogramColumn {
    /// When the engine took the column, in milliseconds of engine time
    /// (`AudioSpectrogram::column_times_ms`).
    pub time_ms: u64,
    pub snapshot: CollectedAudioSnapshot,
    pub buckets: Vec<AudioBucket>,
}

/// Engine status a frontend shows ([`ServerMessage::Status`]).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EngineStatus {
    pub grand_master_percent: u8,
    pub mainloop_state: AudioThreadControlSignal,
    pub audio_params: SignalCollectorParams,
    pub artnet_receivers: Vec<ArtNetReceiver>,
    pub plugins: HashMap<u8, PluginState>,
    pub plugin_runtime_kinds: HashMap<u8, PluginRuntimeKind>,
    pub remote_server: RemoteServerStatus,
}

/// The websocket server's settings and connected clients.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RemoteServerStatus {
    /// Whether it accepts connections (the setting).
    pub enabled: bool,
    /// Where it listens right now.
    pub listening: Option<SocketAddr>,
    /// Why it can't listen, if it can't.
    pub error: Option<String>,
    pub passcode_set: bool,
    /// The passcode, for the desk. Never leaves the engine.
    #[serde(skip)]
    pub passcode: Option<String>,
    pub max_clients: u32,
    pub sessions: Vec<RemoteSessionInfo>,
}

/// One connected remote client.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RemoteSessionInfo {
    pub client: ClientId,
    pub session: SessionId,
    pub peer: SocketAddr,
    pub connected_secs: u64,
    pub sent_bytes: u64,
    /// Its socket takes no more data right now.
    pub backlogged: bool,
}

impl Default for EngineStatus {
    fn default() -> Self {
        Self {
            grand_master_percent: 100,
            mainloop_state: AudioThreadControlSignal::ABORTED,
            audio_params: SignalCollectorParams::default(),
            artnet_receivers: Vec::new(),
            plugins: HashMap::new(),
            plugin_runtime_kinds: HashMap::new(),
            remote_server: RemoteServerStatus::default(),
        }
    }
}

/// The system messages a remote client gets: every [`SystemMessage`] except
/// the desk's own (`DeskRequest`, the desk's showfile section in
/// `ShowLoaded`) and the unused `DMX`. Results arrive as
/// [`ServerMessage::Result`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum WireSystemMessage {
    Heartbeat(u64),
    EngineInitializationComplete,
    StartupStage {
        stage: StartupStage,
        status: StageStatus,
    },
    Log {
        message: String,
        level: LogLevel,
        source: String,
    },
    WasmLog(WasmLogBody),
    TickSpeeds(TickSpeeds),
    AudioSelected(String),
    AudioDevicesView(Vec<(String, String)>),
    PluginAlert {
        plugin_id: u8,
        label: String,
        duration_ms: u32,
    },
    /// The engine loaded another show; ids from the previous one mean
    /// nothing now.
    ShowLoaded {
        generation: u64,
    },
    ShowfileStatus(ShowfileStatus),
    Exiting {
        restart: bool,
    },
}

impl WireSystemMessage {
    /// The wire form of `message`; `None` for what remote clients don't get.
    pub fn from_system(message: &SystemMessage) -> Option<Self> {
        Some(match message {
            SystemMessage::Heartbeat(n) => WireSystemMessage::Heartbeat(*n as u64),
            SystemMessage::EngineInitializationComplete => {
                WireSystemMessage::EngineInitializationComplete
            }
            SystemMessage::StartupStage { stage, status } => WireSystemMessage::StartupStage {
                stage: *stage,
                status: status.clone(),
            },
            SystemMessage::Log {
                message,
                level,
                source,
            } => WireSystemMessage::Log {
                message: message.clone(),
                level: level.clone(),
                source: source.to_string(),
            },
            SystemMessage::WasmLog(body) => WireSystemMessage::WasmLog(body.clone()),
            SystemMessage::TickSpeeds(speeds) => WireSystemMessage::TickSpeeds(speeds.clone()),
            SystemMessage::AudioSelected(name) => WireSystemMessage::AudioSelected(name.clone()),
            SystemMessage::AudioDevicesView(devices) => {
                WireSystemMessage::AudioDevicesView(devices.clone())
            }
            SystemMessage::PluginAlert {
                plugin_id,
                label,
                duration_ms,
            } => WireSystemMessage::PluginAlert {
                plugin_id: *plugin_id,
                label: label.clone(),
                duration_ms: *duration_ms,
            },
            SystemMessage::ShowLoaded { generation, .. } => WireSystemMessage::ShowLoaded {
                generation: *generation,
            },
            SystemMessage::ShowfileStatus(status) => {
                WireSystemMessage::ShowfileStatus(status.clone())
            }
            SystemMessage::Exiting { restart } => WireSystemMessage::Exiting { restart: *restart },
            SystemMessage::DMX(_)
            | SystemMessage::DeskRequest(_)
            | SystemMessage::CommandResult(_) => return None,
        })
    }

    /// The [`SystemMessage`] a remote client hands its frontend.
    pub fn into_system(self) -> SystemMessage {
        match self {
            WireSystemMessage::Heartbeat(n) => SystemMessage::Heartbeat(n as usize),
            WireSystemMessage::EngineInitializationComplete => {
                SystemMessage::EngineInitializationComplete
            }
            WireSystemMessage::StartupStage { stage, status } => {
                SystemMessage::StartupStage { stage, status }
            }
            WireSystemMessage::Log {
                message,
                level,
                source,
            } => SystemMessage::Log {
                message,
                level,
                source: source.into(),
            },
            WireSystemMessage::WasmLog(body) => SystemMessage::WasmLog(body),
            WireSystemMessage::TickSpeeds(speeds) => SystemMessage::TickSpeeds(speeds),
            WireSystemMessage::AudioSelected(name) => SystemMessage::AudioSelected(name),
            WireSystemMessage::AudioDevicesView(devices) => {
                SystemMessage::AudioDevicesView(devices)
            }
            WireSystemMessage::PluginAlert {
                plugin_id,
                label,
                duration_ms,
            } => SystemMessage::PluginAlert {
                plugin_id,
                label,
                duration_ms,
            },
            WireSystemMessage::ShowLoaded { generation } => SystemMessage::ShowLoaded {
                generation,
                frontend_section: None,
            },
            WireSystemMessage::ShowfileStatus(status) => SystemMessage::ShowfileStatus(status),
            WireSystemMessage::Exiting { restart } => SystemMessage::Exiting { restart },
        }
    }
}

/// `#[serde(with = ..)]` helpers for wire types whose serde form differs
/// from their in-memory one.
pub mod serde_as {
    /// `Option<SystemTime>` as UNIX time in milliseconds (`None` = null).
    pub mod unix_millis {
        use std::time::Duration;
        use web_time::{SystemTime, UNIX_EPOCH};

        use serde::{Deserialize, Deserializer, Serialize, Serializer};

        pub fn serialize<S: Serializer>(
            time: &Option<SystemTime>,
            serializer: S,
        ) -> Result<S::Ok, S::Error> {
            time.map(|time| {
                time.duration_since(UNIX_EPOCH)
                    .map(|since| since.as_millis() as u64)
                    .unwrap_or(0)
            })
            .serialize(serializer)
        }

        pub fn deserialize<'de, D: Deserializer<'de>>(
            deserializer: D,
        ) -> Result<Option<SystemTime>, D::Error> {
            Ok(Option::<u64>::deserialize(deserializer)?
                .map(|millis| UNIX_EPOCH + Duration::from_millis(millis)))
        }
    }

    /// A value as its JSON text, for types whose serde form bincode can't
    /// decode (`skip_serializing_if`, `untagged`).
    pub mod json_string {
        use serde::{de::DeserializeOwned, Deserialize, Deserializer, Serialize, Serializer};

        pub fn serialize<T: Serialize, S: Serializer>(
            value: &T,
            serializer: S,
        ) -> Result<S::Ok, S::Error> {
            use serde::ser::Error;
            serde_json::to_string(value)
                .map_err(S::Error::custom)?
                .serialize(serializer)
        }

        pub fn deserialize<'de, T: DeserializeOwned, D: Deserializer<'de>>(
            deserializer: D,
        ) -> Result<T, D::Error> {
            use serde::de::Error;
            let text = String::deserialize(deserializer)?;
            serde_json::from_str(&text).map_err(D::Error::custom)
        }
    }

    /// A byte string (bincode: length and raw bytes; JSON: an array of
    /// numbers). Reads either form.
    pub mod bytes {
        use std::fmt;

        use serde::{
            de::{SeqAccess, Visitor},
            Deserializer,
        };

        struct BytesVisitor;

        impl<'de> Visitor<'de> for BytesVisitor {
            type Value = Vec<u8>;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "a byte string")
            }

            fn visit_bytes<E: serde::de::Error>(self, bytes: &[u8]) -> Result<Vec<u8>, E> {
                Ok(bytes.to_vec())
            }

            fn visit_byte_buf<E: serde::de::Error>(self, bytes: Vec<u8>) -> Result<Vec<u8>, E> {
                Ok(bytes)
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Vec<u8>, A::Error> {
                let mut bytes = Vec::with_capacity(seq.size_hint().unwrap_or(0).min(1 << 20));
                while let Some(byte) = seq.next_element::<u8>()? {
                    bytes.push(byte);
                }
                Ok(bytes)
            }
        }

        pub fn deserialize<'de, D: Deserializer<'de>>(
            deserializer: D,
        ) -> Result<Vec<u8>, D::Error> {
            deserializer.deserialize_byte_buf(BytesVisitor)
        }
    }

    /// `Arc<[u8]>` as a byte string (see [`bytes`]).
    pub mod shared_bytes {
        use std::sync::Arc;

        use serde::{Deserializer, Serializer};

        pub fn serialize<S: Serializer>(
            bytes: &Arc<[u8]>,
            serializer: S,
        ) -> Result<S::Ok, S::Error> {
            serializer.serialize_bytes(bytes)
        }

        pub fn deserialize<'de, D: Deserializer<'de>>(
            deserializer: D,
        ) -> Result<Arc<[u8]>, D::Error> {
            Ok(super::bytes::deserialize(deserializer)?.into())
        }
    }
}
