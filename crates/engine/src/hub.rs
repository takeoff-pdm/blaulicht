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
    net::SocketAddr,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
        Arc, Mutex, MutexGuard, PoisonError,
    },
    time::Duration,
};

use blaulicht_shared::{
    fixture::state::Fixture,
    folder::FolderId,
    scene::FixtureSelection,
    scene_graph::{GraphId, NodeId, TransitionCondition},
    view::ViewSceneMasters,
    AnimationPreset, AnimationSpec, AnimationSpeedModifier, ControlEvent, ControlEventMessage,
    CONTROLS_REQUIRING_SELECTION,
};
use crossbeam_channel::{Receiver, Sender};
use serde::{Deserialize, Serialize};
use strum::EnumCount;

use crate::{
    config::ShowfileUiState,
    event::{EventBusHandle, SystemEventBusConnection},
    frontend_ui::{FrontendUiMirror, FrontendUiPublisher, FrontendUiSnapshot},
    msg::{StageStatus, StartupStage, SystemMessage, TERMINAL_ONLY_LOG_TARGET},
    stage::{FixturePlacement, StageObject, StagePatch, StageScene},
};

//
// Identities.
//

/// Who sent a bus message, and who receives its [`CommandResult`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
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
/// see `DESK`'s session-scoped events ([`plugin_events`]), the desk ignores
/// other sessions' events for its jog target, and the engine doesn't mark
/// the show edited for other sessions' events that only change session state
/// (`persistence::control_event_marks_showfile_dirty`).
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
        // Live mode is global: there is one rig. The sender becomes the
        // preview's owner (`EngineState::live_owner`), but every session sees
        // the overlays suppressed.
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
///
/// Commands that act on a group, scene, fixture or template name it by id
/// (never by the sender's focus), so a dialog acts on what it was opened for.
///
/// Serializable for remote clients (`crate::server`). Changing it changes
/// the wire protocol: bump `server::protocol::PROTOCOL_VERSION`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum HostCommand {
    /// Sets whether the server accepts remote clients and the passcode they
    /// must state (`None` or blank: none), and saves both in the config.
    /// Turning it off disconnects every remote client; changing the passcode
    /// disconnects them too, so they must state the new one. Desk only.
    SetRemoteAccess {
        enabled: bool,
        passcode: Option<String>,
    },
    /// Does nothing; the engine answers `Ok`. Lets a client check its command
    /// path end to end.
    Ping,
    /// Applies a starter preset to the sender's selection in its focused
    /// scene, or removes it if it is already on it. The preset's template
    /// (the lowest-id template named after it) is created first if there is
    /// none, and reported as `CreatedId::AnimationTemplate`. Refused with
    /// "Animation limit reached" when every template id is taken, or with the
    /// engine's reason when it rejects the add (the template stays).
    ToggleAnimationPreset(AnimationPreset),
    /// Saves the animation `animation_id` on `selection` in the sender's
    /// focused scene as a new template named `name` (trimmed), copying a WASM
    /// animation's plugin state to the template. Answers
    /// `CreatedId::AnimationTemplate`; refused with "Animation not found" or
    /// "Animation limit reached".
    SaveAnimationAsTemplate {
        selection: FixtureSelection,
        animation_id: u8,
        name: String,
    },
    /// Copies the persisted state of WASM animation instance `source` of
    /// plugin `plugin_key` to instance `target` (for example from a scene's
    /// animation to an editor, and back on Apply). `target` restarts with it.
    /// Does nothing if `source` has no state.
    CloneAnimationInstanceState {
        plugin_key: String,
        source: u64,
        target: u64,
    },
    /// Keeps a WASM animation editor ticking: the plugin `plugin_key` runs
    /// editor instance `instance_id` on `fixtures` and publishes its editor
    /// UI. The engine drops a request it hasn't heard of for 250 ms, so an
    /// open editor resends it (`ANIMATION_EDITOR_KEEPALIVE`).
    KeepAnimationEditorAlive {
        plugin_key: String,
        instance_id: u64,
        fixtures: Vec<(u8, u8)>,
    },

    //
    // Setup › Groups.
    //
    /// Creates a group named `name` (as given) and moves it into `folder`
    /// (`None` = top level; a folder it can't move into leaves it at the top
    /// level). Answers `CreatedId::Group`; refused with "Group limit
    /// reached".
    CreateGroup {
        name: String,
        folder: Option<FolderId>,
    },
    /// Renames a group to `name` (trimmed). Refused with "Name is empty" or
    /// "Group not found".
    RenameGroup { group: u8, name: String },
    /// Deletes a group, its fixtures' scene state and every session's
    /// selection of it (the sender's selection and stack are cleared, as on
    /// the desk before). Refused with "Group not found".
    DeleteGroup(u8),

    //
    // Setup › Scenes.
    //
    /// Creates an empty scene named `name` (as given) in `folder` (as for
    /// `CreateGroup`). Answers `CreatedId::Scene`; refused with "Scene limit
    /// reached".
    CreateScene {
        name: String,
        folder: Option<FolderId>,
    },
    /// Copies scene `scene` (its changes, animations and masters) into a new
    /// scene named `name`, in the same folder. Answers `CreatedId::Scene`;
    /// refused with "Scene not found" (also for `BLANK`) or "Scene limit
    /// reached".
    CloneScene { scene: u8, name: String },
    /// Renames a scene to `name` (trimmed). Refused with "Name is empty" or
    /// "Scene not found" (also for `BLANK`).
    RenameScene { scene: u8, name: String },
    /// Deletes a scene; sessions focused on it move to another scene, and a
    /// live preview of it ends. Refused with "Scene not found" (also for
    /// `BLANK`) or "At least one scene must remain".
    DeleteScene(u8),

    //
    // Setup › Fixtures.
    //
    /// Adds `fixtures` to `group` as one batch: all or none. Refused with
    /// the reason (`FixtureBatchError`), e.g. a DMX overlap.
    AddFixtures { group: u8, fixtures: Vec<Fixture> },
    /// Replaces fixture `fixture` of `group` with `value` (name, address,
    /// position, ...). Refused with the reason (`FixtureBatchError`).
    UpdateFixture {
        group: u8,
        fixture: u8,
        value: Fixture,
    },
    /// Deletes a fixture and its scene state; every session's selection is
    /// repaired (the sender's selection and stack are cleared, as on the desk
    /// before). Refused with "Fixture not found".
    DeleteFixture { group: u8, fixture: u8 },
    /// Swaps the ids (the order) of fixtures `a` and `b` of `group`, with
    /// their scene state. Refused with "Fixture not found".
    SwapFixtures { group: u8, a: u8, b: u8 },
    /// Moves fixture `fixture` of `group` into group `target`, with its scene
    /// state. Answers `CreatedId::Fixture` (its id in `target`); refused with
    /// "Could not move fixture".
    MoveFixtureToGroup { group: u8, fixture: u8, target: u8 },
    /// Runs the `setup` routine (e.g. a lamp strike) of each fixture
    /// `(group, fixture)` on top of the running show for a few seconds.
    /// Refused with "Fixture not found" if one doesn't exist (then none
    /// runs).
    InitFixtures(Vec<(u8, u8)>),

    //
    // Setup › Animations.
    //
    /// Adds `spec` as a new animation template (lowest free id). Answers
    /// `CreatedId::AnimationTemplate`; refused with "Animation limit
    /// reached".
    CreateAnimationTemplate(AnimationSpec),
    /// Replaces template `id`'s spec. Refused with "Animation not found".
    UpdateAnimationTemplate { id: u8, spec: AnimationSpec },
    /// Deletes template `id` and removes it from every scene.
    DeleteAnimationTemplate(u8),

    //
    // View Performance.
    //
    /// Sets the grand master (0..=100 percent; higher values are clamped).
    SetGrandMaster(u8),

    //
    // View.
    //
    /// Creates a view named `name` (trimmed) that captures the current
    /// overlay stack. Answers `CreatedId::View`; refused with "Name is
    /// empty" or "View limit reached".
    CreateView { name: String },
    /// As [`HostCommand::CreateView`], but the new view starts with no
    /// overlays (the desktop Add view dialog).
    CreateEmptyView { name: String },
    /// Deletes a view. Refused with "View not found" or "At least one view
    /// must remain".
    DeleteView(u8),
    /// Appends scene `scene` to view `view`'s overlays (nothing changes if it
    /// is already one). Refused with "View not found" or "Scene not found"
    /// (also for `BLANK`).
    AddViewOverlay { view: u8, scene: u8 },
    /// Removes scene `scene` from view `view`'s overlays, with its masters.
    /// Refused with "View not found".
    RemoveViewOverlay { view: u8, scene: u8 },
    /// Sets the masters view `view` gives overlay `scene` when applied.
    /// Refused with "View not found" or "Scene is not an overlay of this
    /// view".
    SetViewSceneMasters {
        view: u8,
        scene: u8,
        masters: ViewSceneMasters,
    },

    //
    // Scene graph. Each edits one part of one graph, so the engine's own
    // per-tick changes (the active node, countdowns, the runtime clock)
    // survive an edit.
    //
    /// Creates a scene graph named `name` (as given) and focuses it in the
    /// sender's session. Answers `CreatedId::SceneGraph`; refused with
    /// "Scene graph limit reached".
    CreateSceneGraph { name: String },
    /// Deletes a scene graph; every session focused on it loses its focus.
    /// Refused with "Scene graph not found".
    DeleteSceneGraph(GraphId),
    /// Adds a default node to `graph` at canvas position (`x`, `y`). Answers
    /// `CreatedId::SceneGraphNode`; refused with "Scene graph not found" or
    /// "Node limit reached".
    AddSceneGraphNode { graph: GraphId, x: f32, y: f32 },
    /// Removes a node and its edges (and the active node, if it is that).
    /// Refused with "Scene graph not found" or "Node not found".
    RemoveSceneGraphNode { graph: GraphId, node: NodeId },
    /// Moves nodes of `graph` to canvas positions `(node, x, y)` (clamped to
    /// the canvas). Refused with "Scene graph not found" or "Node not found"
    /// (then nothing moves).
    MoveSceneGraphNodes {
        graph: GraphId,
        positions: Vec<(NodeId, f32, f32)>,
    },
    /// Adds a `Manual` edge `from` -> `to` (nothing changes if there is one).
    /// Refused with "Scene graph not found" or "Node not found".
    ConnectSceneGraphNodes {
        graph: GraphId,
        from: NodeId,
        to: NodeId,
    },
    /// Removes the edge `from` -> `to`. Refused with "Scene graph not found"
    /// or "Edge not found".
    DisconnectSceneGraphNodes {
        graph: GraphId,
        from: NodeId,
        to: NodeId,
    },
    /// Sets one parameter of a node. Refused with "Scene graph not found",
    /// "Node not found" or, for `AddScene`, "Scene not found".
    SetSceneGraphNode {
        graph: GraphId,
        node: NodeId,
        param: SceneGraphNodeParam,
    },
    /// Sets one parameter of the edge `from` -> `to`. Refused with "Scene
    /// graph not found" or "Edge not found".
    SetSceneGraphEdge {
        graph: GraphId,
        from: NodeId,
        to: NodeId,
        param: SceneGraphEdgeParam,
    },
    /// Makes `node` the active node of `graph` ("Set as Start"). Refused
    /// with "Scene graph not found" or "Node not found".
    SetSceneGraphStart { graph: GraphId, node: NodeId },

    //
    // Visualizer: the stage (room, objects, trusses, fixture attachments).
    // After each, and after every patch change, the engine places the
    // fixtures attached to trusses (`EngineState::reconcile_stage`).
    //
    /// Adds an object to the stage. Answers `CreatedId::StageObject`.
    AddStageObject(StageObject),
    /// Removes objects with their truss connections and the attachments of
    /// fixtures hanging on them (the fixtures stay where they are). Refused
    /// with "Stage object not found" (then nothing is removed).
    RemoveStageObjects(Vec<u64>),
    /// Applies one editor change as a whole: room, object contents,
    /// connections, attachments and hand-placed fixtures. Refused with
    /// "Stage object not found", "Fixture not found", "Truss not found" or
    /// "Trusses cannot be connected" (then nothing changes).
    PatchStage(Box<StagePatch>),
    /// Replaces the whole stage and puts `fixtures` where they were, for the
    /// visualizer's undo and redo. The engine never changes the stage by
    /// itself between commands, so this loses no engine-side update.
    RestoreStage {
        stage: Box<StageScene>,
        fixtures: Vec<FixturePlacement>,
    },
    /// Stores a binary glTF model next to the showfile (in `<showfile
    /// stem>.assets/`, named by its content hash) and adds it to the stage.
    /// Answers `CreatedId::StageObject`; refused with "Save the showfile
    /// before importing models" or why the file was rejected.
    ImportStageModel {
        file_name: String,
        bytes: ModelBytes,
    },
    /// Removes imported models (assets) and the objects that show them.
    /// Refused with "Model not found" (then nothing is removed).
    RemoveStageAssets(Vec<u64>),

    //
    // Audio. The audio input and the plugin reload are carried out by the
    // supervisor thread (`mainloop::SupervisorRequest`), which owns and
    // restarts the main loop; the engine checks and forwards them.
    //
    /// Runs the engine on the audio input named `name`: a device name from
    /// `SystemMessage::AudioDevicesView` or a `DummyInput` name (`None`:
    /// "Dummy: Silence"), and stores it as the config's
    /// `default_audio_device`. The main loop restarts on the new input and
    /// `SystemMessage::AudioSelected` follows. Refused with "Audio input not
    /// found" (nothing changes) or "The engine supervisor is not running".
    SelectAudioInput { name: Option<String> },
    /// Sets one audio analysis parameter (the collector picks it up within
    /// one spectrogram period).
    SetAudioParam(AudioParam),

    //
    // System.
    //
    /// Reloads every plugin (and the MIDI and UDP managers) if the engine is
    /// running; does nothing while it is starting or already reloading.
    /// Refused with "The engine supervisor is not running".
    ReloadPlugins,
    /// Adds a user Art-Net receiver for all universes (saved in the
    /// showfile). Refused with "Art-Net receiver must be IPv4" or "Art-Net
    /// receiver <address> already exists".
    AddArtNetReceiver(SocketAddr),
    /// Removes a user Art-Net receiver. Refused with "Art-Net receiver not
    /// found" (also for a plugin's receiver).
    RemoveArtNetReceiver(SocketAddr),
    /// Turns the receiver at `address` owned by `owner` (`None`: a user
    /// receiver, `Some(plugin)`: a plugin's) on or off. Refused with
    /// "Art-Net receiver not found".
    SetArtNetReceiverEnabled {
        address: SocketAddr,
        owner: Option<u8>,
        enabled: bool,
    },
    /// Limits a user Art-Net receiver to universes `first..=last` (clamped,
    /// swapped if inverted). Refused with "Art-Net receiver not found" (also
    /// for a plugin's receiver, whose range the plugin owns).
    SetArtNetReceiverUniverses {
        address: SocketAddr,
        first: u8,
        last: u8,
    },

    //
    // Showfile and lifecycle. Carried out by the persistence service
    // (`crate::persistence`) on its own thread, so file IO never runs under
    // the engine lock; it answers them itself, once done.
    //
    /// Saves the show to the open showfile (nothing is written if it is
    /// unchanged). Refused with "No showfile is open" or "Save failed: <why>".
    SaveShowfile,
    /// Saves the show as `path`, which becomes the open showfile (also in the
    /// config), copying the stage's model assets next to it. Refused with
    /// "Failed to copy stage assets: <why>" or "Save failed: <why>".
    SaveShowfileAs(PathBuf),
    /// Replaces the show with the showfile at `path`, which becomes the open
    /// showfile (also in the config), and resets the grand master.
    /// `SystemMessage::ShowLoaded` follows. Refused with "Could not load
    /// <file>: <why>" (then nothing changes).
    LoadShowfile(PathBuf),
    /// Closes the showfile: the show becomes a new, empty show without one,
    /// and the grand master is reset. `SystemMessage::ShowLoaded` follows.
    CloseShowfile,
    /// The desk frontend's section of the showfile (its screen layouts),
    /// which the engine saves with the show and hands back on load. Send it
    /// when it changed, for the `generation` of the last
    /// `SystemMessage::ShowLoaded` the frontend applied; a section for
    /// another generation is ignored. A changed section marks the show
    /// edited, except the first one for a generation (the frontend's form of
    /// the loaded section).
    SetFrontendSection {
        generation: u64,
        /// On the wire as JSON text: its showfile form uses serde features
        /// bincode can't decode.
        #[serde(with = "crate::server::protocol::serde_as::json_string")]
        section: ShowfileUiState,
    },
    /// Runs the shutdown hook (saves the show if `save`, resets the grand
    /// master) and lets the process exit (`SystemMessage::Exiting`). Refused
    /// with "Save failed: <why>" (then nothing else happens).
    Quit { save: bool },
    /// As [`HostCommand::Quit`], then the process exits with code 42 (its
    /// launcher starts it again).
    Restart { save: bool },
    /// Flashes fixture `(group, fixture)` (Setup › Fixtures › Identify) on
    /// top of the running show, or stops flashing (`None`). Only one fixture
    /// flashes at a time; the flash stops by itself after
    /// [`IDENTIFY_TIMEOUT`] without a repeat, so a client that vanishes
    /// can't leave it on. Refused with "Fixture not found".
    IdentifyFixture(Option<(u8, u8)>),
    /// Renames view `view` to `name` (trimmed). Refused with "Name is
    /// empty" or "View not found".
    RenameView { view: u8, name: String },
}

/// How long an [`HostCommand::IdentifyFixture`] flash lasts without the
/// client repeating it.
pub const IDENTIFY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3);

/// One audio analysis parameter ([`HostCommand::SetAudioParam`]).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum AudioParam {
    Volume(u8),
    Gate(u8),
    /// `None` (or `Some(0)`) turns the boost off.
    Boost(Option<u8>),
    AutoCalibrate(bool),
    /// `Some(bpm)` bypasses tempo detection; refused with "BPM must be
    /// positive" unless `bpm` is finite and above 0.
    ManualBpm(Option<f32>),
}

/// The contents of a model file in [`HostCommand::ImportStageModel`].
/// Shared, so the bus copies a pointer, not the file, to each connection;
/// `Debug` prints only the size. On the wire as a byte string (bincode: raw
/// bytes; JSON: an array of numbers).
#[derive(Clone, Serialize, Deserialize)]
pub struct ModelBytes(
    #[serde(with = "crate::server::protocol::serde_as::shared_bytes")] pub Arc<[u8]>,
);

impl fmt::Debug for ModelBytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ModelBytes({} bytes)", self.0.len())
    }
}

/// One editable parameter of a scene graph node
/// ([`HostCommand::SetSceneGraphNode`]).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum SceneGraphNodeParam {
    Name(String),
    /// 0..=100 percent (higher values are clamped).
    MasterAlpha(u8),
    MasterSpeed(AnimationSpeedModifier),
    /// Appends a scene (nothing changes if the node already has it).
    AddScene(u8),
    /// Removes a scene (nothing changes if the node doesn't have it).
    RemoveScene(u8),
}

/// One editable parameter of a scene graph edge
/// ([`HostCommand::SetSceneGraphEdge`]).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum SceneGraphEdgeParam {
    Condition(TransitionCondition),
    Priority(u8),
}

impl HostCommand {
    /// Whether applying the command changes what the showfile saves: the
    /// engine marks the show edited when it applies one
    /// (`crate::persistence`), whichever client sent it.
    pub fn edits_show(&self) -> bool {
        match self {
            HostCommand::ToggleAnimationPreset(_)
            | HostCommand::SaveAnimationAsTemplate { .. }
            | HostCommand::CreateGroup { .. }
            | HostCommand::RenameGroup { .. }
            | HostCommand::DeleteGroup(_)
            | HostCommand::CreateScene { .. }
            | HostCommand::CloneScene { .. }
            | HostCommand::RenameScene { .. }
            | HostCommand::DeleteScene(_)
            | HostCommand::AddFixtures { .. }
            | HostCommand::UpdateFixture { .. }
            | HostCommand::DeleteFixture { .. }
            | HostCommand::SwapFixtures { .. }
            | HostCommand::MoveFixtureToGroup { .. }
            | HostCommand::CreateAnimationTemplate(_)
            | HostCommand::UpdateAnimationTemplate { .. }
            | HostCommand::DeleteAnimationTemplate(_)
            | HostCommand::CreateView { .. }
            | HostCommand::CreateEmptyView { .. }
            | HostCommand::DeleteView(_)
            | HostCommand::RenameView { .. }
            | HostCommand::AddViewOverlay { .. }
            | HostCommand::RemoveViewOverlay { .. }
            | HostCommand::SetViewSceneMasters { .. }
            | HostCommand::CreateSceneGraph { .. }
            | HostCommand::DeleteSceneGraph(_)
            | HostCommand::AddSceneGraphNode { .. }
            | HostCommand::RemoveSceneGraphNode { .. }
            | HostCommand::MoveSceneGraphNodes { .. }
            | HostCommand::ConnectSceneGraphNodes { .. }
            | HostCommand::DisconnectSceneGraphNodes { .. }
            | HostCommand::SetSceneGraphNode { .. }
            | HostCommand::SetSceneGraphEdge { .. }
            | HostCommand::SetSceneGraphStart { .. }
            | HostCommand::AddStageObject(_)
            | HostCommand::RemoveStageObjects(_)
            | HostCommand::PatchStage(_)
            | HostCommand::RestoreStage { .. }
            | HostCommand::ImportStageModel { .. }
            | HostCommand::RemoveStageAssets(_)
            | HostCommand::AddArtNetReceiver(_)
            | HostCommand::RemoveArtNetReceiver(_)
            | HostCommand::SetArtNetReceiverUniverses { .. } => true,
            // User receivers are saved in the showfile; plugin receivers
            // belong to their plugin.
            HostCommand::SetArtNetReceiverEnabled { owner, .. } => owner.is_none(),
            // A copy into an editor instance is scratch state; the Apply
            // that copies it back sends `LoadSpecIntoAnimation` (Programmer)
            // or `UpdateAnimationTemplate` (Setup), which mark the show.
            // Initializing a fixture only changes its DMX output for a while.
            // The grand master and the audio parameters are not saved in the
            // showfile; the audio input is saved in the config. The showfile
            // commands are the persistence service's; it tracks the frontend
            // section itself.
            HostCommand::Ping
            | HostCommand::SetRemoteAccess { .. }
            | HostCommand::SaveShowfile
            | HostCommand::SaveShowfileAs(_)
            | HostCommand::LoadShowfile(_)
            | HostCommand::CloseShowfile
            | HostCommand::SetFrontendSection { .. }
            | HostCommand::Quit { .. }
            | HostCommand::Restart { .. }
            | HostCommand::SetGrandMaster(_)
            | HostCommand::SelectAudioInput { .. }
            | HostCommand::SetAudioParam(_)
            | HostCommand::ReloadPlugins
            | HostCommand::CloneAnimationInstanceState { .. }
            | HostCommand::KeepAnimationEditorAlive { .. }
            | HostCommand::InitFixtures(_)
            | HostCommand::IdentifyFixture(_) => false,
        }
    }
}

impl HostCommand {
    /// Commands only the desk frontend may send; the engine refuses them
    /// from every other client. `SetFrontendSection` is the desk's own part
    /// of the showfile (its screen layouts). `SetRemoteAccess` stays with the
    /// desk so a remote client can't lock others out, or itself.
    pub fn is_desk_only(&self) -> bool {
        matches!(
            self,
            HostCommand::SetFrontendSection { .. } | HostCommand::SetRemoteAccess { .. }
        )
    }

    /// Whether the command can change the fixture patch, so fixtures
    /// attached to trusses need placing again (stage commands do that
    /// themselves).
    pub fn changes_patch(&self) -> bool {
        matches!(
            self,
            HostCommand::DeleteGroup(_)
                | HostCommand::AddFixtures { .. }
                | HostCommand::UpdateFixture { .. }
                | HostCommand::DeleteFixture { .. }
                | HostCommand::SwapFixtures { .. }
                | HostCommand::MoveFixtureToGroup { .. }
        )
    }
}

/// How often an open WASM animation editor resends
/// [`HostCommand::KeepAnimationEditorAlive`]; well below the engine's 250 ms
/// timeout.
pub const ANIMATION_EDITOR_KEEPALIVE: Duration = Duration::from_millis(100);

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum BusBody {
    /// Applied by the engine and forwarded to plugins unchanged.
    Control(ControlEventMessage),
    /// Applied by the engine; never forwarded to plugins.
    Host(HostCommand),
}

impl From<ControlEventMessage> for BusBody {
    fn from(message: ControlEventMessage) -> Self {
        BusBody::Control(message)
    }
}

impl From<HostCommand> for BusBody {
    fn from(command: HostCommand) -> Self {
        BusBody::Host(command)
    }
}

/// A message on the event bus. Host-only: plugins receive only the
/// `ControlEventMessage` inside a [`BusBody::Control`].
#[derive(Clone, Debug, Serialize, Deserialize)]
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CreatedId {
    Palette(u8),
    /// An animation template.
    AnimationTemplate(u8),
    Group(u8),
    Scene(u8),
    /// A fixture's id in its (new) group.
    Fixture {
        group: u8,
        fixture: u8,
    },
    View(u8),
    SceneGraph(GraphId),
    /// A node's id in its graph.
    SceneGraphNode {
        graph: GraphId,
        node: NodeId,
    },
    /// An object on the visualizer's stage.
    StageObject(u64),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommandOutcome {
    Ok {
        created: Option<CreatedId>,
    },
    /// The engine refused the command and changed nothing (a transaction is
    /// rolled back as a whole).
    Refused(String),
}

/// The engine's reply to one bus message, sent to its client only.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
    show_loaded: Option<SystemMessage>,
    showfile_status: Option<SystemMessage>,
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
            show_loaded: None,
            showfile_status: None,
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
            SystemMessage::ShowLoaded { .. } => self.show_loaded = Some(message.clone()),
            SystemMessage::ShowfileStatus(_) => self.showfile_status = Some(message.clone()),
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
            .chain(self.show_loaded.clone())
            .chain(self.showfile_status.clone())
            .chain(self.logs.iter().cloned())
    }
}

struct Subscriber {
    client: ClientId,
    /// Most messages this subscriber's queue holds.
    capacity: usize,
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
    fn push(&mut self, message: SystemMessage) -> bool {
        let mut first_overflow = false;
        // Pushes are serialized by the hub lock; the reader only shrinks the
        // queue.
        if self.sender.len() >= self.capacity {
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
    /// The most messages a replay can hold; no subscriber's queue is
    /// smaller.
    replay_len: usize,
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
        // Room for the stages, the init flag, the two audio messages and the
        // two showfile messages.
        let fixed_replay = StartupStage::COUNT + 5;
        let log_replay_len = log_replay_len.min(capacity.saturating_sub(fixed_replay));
        Self {
            inner: Arc::new(HubInner {
                state: Mutex::new(HubState {
                    next_subscriber: 0,
                    subscribers: BTreeMap::new(),
                    replay: Replay::new(log_replay_len),
                }),
                capacity,
                replay_len: fixed_replay + log_replay_len,
                next_remote: AtomicU32::new(0),
                next_session: AtomicU32::new(SessionId::DESK.0 + 1),
                session_changes: Mutex::new(Vec::new()),
                shutdown: AtomicBool::new(false),
            }),
        }
    }

    /// Sends `message` to every subscriber and records it for the replay.
    pub fn publish(&self, message: SystemMessage) {
        let overflowed: Vec<ClientId> = {
            let mut state = self.inner.lock();
            state.replay.record(&message);
            state
                .subscribers
                .values_mut()
                .filter_map(|subscriber| {
                    subscriber
                        .push(message.clone())
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
                    if subscriber.push(message.clone()) {
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
        self.subscribe_with_capacity(client, self.inner.capacity)
    }

    /// [`Self::subscribe`] with a queue of at most `capacity` messages
    /// (raised to what a replay can hold, up to the hub's own capacity), for
    /// example for a remote client.
    pub fn subscribe_with_capacity(&self, client: ClientId, capacity: usize) -> HubSubscription {
        let capacity = capacity
            .max(self.inner.replay_len.min(self.inner.capacity))
            .max(1);
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
                capacity,
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

    /// A remote client's connection: a fresh [`ClientId::Remote`] in a new
    /// session of its own (see [`Self::connect_new_session`]), whose hub
    /// queue holds at most `queue_capacity` messages.
    pub fn connect_remote(
        hub: &SystemHub,
        bus: &EventBusHandle<BusMessage>,
        mirror: &FrontendUiMirror,
        queue_capacity: usize,
    ) -> Self {
        let id = hub.allocate_client();
        let session = hub.open_session();
        Self {
            id,
            session: session.id(),
            next_seq: AtomicU64::new(1),
            bus: bus.connect(),
            system: hub.subscribe_with_capacity(id, queue_capacity),
            ui: mirror.connect(id),
            _session: Some(session),
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
        self.send_with_seq(seq, body);
        seq
    }

    /// Sends `body` in this client's envelope under the sequence number the
    /// client chose (a remote client numbers its own commands).
    pub fn send_with_seq(&self, seq: CommandSeq, body: BusBody) {
        self.bus.send(BusMessage {
            client: self.id,
            session: self.session,
            seq,
            body,
        });
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

    /// System messages waiting in this client's hub queue.
    pub fn queued_system(&self) -> usize {
        self.system.len()
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
    use web_time::Instant;

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
    fn a_late_subscriber_gets_the_latest_audio_devices_and_input() {
        let hub = SystemHub::new();
        let devices = |names: &[&str]| {
            SystemMessage::AudioDevicesView(
                names
                    .iter()
                    .map(|name| ("ALSA".to_string(), name.to_string()))
                    .collect(),
            )
        };
        hub.publish(devices(&["mic"]));
        hub.publish(SystemMessage::AudioSelected("Dummy: Silence".to_string()));
        hub.publish(devices(&["mic", "line in"]));
        hub.publish(SystemMessage::AudioSelected("Dummy: Noise".to_string()));

        let late = hub.subscribe(ClientId::Remote(0));
        let replayed: Vec<String> = std::iter::from_fn(|| late.try_recv())
            .map(|message| match message {
                SystemMessage::AudioDevicesView(devices) => format!("devices:{devices:?}"),
                SystemMessage::AudioSelected(name) => format!("selected:{name}"),
                _ => "other".to_string(),
            })
            .collect();
        assert_eq!(
            replayed,
            [
                r#"devices:[("ALSA", "mic"), ("ALSA", "line in")]"#,
                "selected:Dummy: Noise",
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
