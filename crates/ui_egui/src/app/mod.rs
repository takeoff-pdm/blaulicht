use crate::app::{
    components::{
        DmxSimulator, LogWindow, Navbar, Numberpad, TimeSeriesGraph, DEFAULT_NEW_GROUP_NAME,
        DEFAULT_NEW_SCENE_NAME,
    },
    external_screen::Pane,
    pages::{
        AddFixtureIncrementAxis, AddFixtureKind, AnimationEditState, AnimationUI, ProgrammerUi,
        SceneGraphUI, SystemUI, ViewPerfUI, VisualizerUiState,
    },
};
use blaulicht_engine::{
    client::{LocalClient, ReplyHandlers},
    config::ShowfileUiState,
    msg::TickSpeeds,
    persistence::SaveStatus,
    state::{ScreenId, NUM_DMX_UNIVERSES},
};
use blaulicht_shared::fixture::dimmer::Dimmer;
use blaulicht_shared::fixture::light::Light;
use blaulicht_shared::fixture::moving_head::MovingHead;
use blaulicht_shared::{AppPage, CollectedAudioSnapshot};
use egui::{Color32, Pos2, Vec2};
use egui_dock::DockState;
use pages::{PaletteUI, ViewUI};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::PathBuf,
    time::{Duration, Instant, SystemTime},
};

mod app_navbar;
mod commands;
pub mod components;
mod debug;
mod event;
pub mod external_screen;
mod jog;
mod page;
pub mod pages;
mod plugin_ui;
mod popup;
mod startup;
mod theme;
mod ui;
mod view_state;

#[derive(PartialEq, Eq, Clone, Copy)]
enum Selection {
    Off,
    Limited,
    Cascading,
}

impl Selection {
    fn color(&self) -> Color32 {
        match self {
            Selection::Off => Color32::from_gray(60),
            Selection::Limited => Color32::from_rgb(120, 180, 80),
            Selection::Cascading => Color32::from_rgb(80, 180, 120),
        }
    }
}

#[derive(Clone)]
pub struct PopupButtonSpec {
    label: String,
}

#[derive(Clone)]
pub struct PopupSpec {
    pub label: String,
    pub label_size: Option<f32>,
    // Auto-close duration
    pub lifetime_duration: Duration,
    pub button: Option<PopupButtonSpec>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum GuardedLifecycleAction {
    Quit,
    Restart,
}

impl PopupSpec {
    pub fn default(label: String) -> Self {
        Self {
            label,
            label_size: None,
            lifetime_duration: Duration::from_secs(5),
            button: Some(PopupButtonSpec {
                label: "Close".to_string(),
            }),
        }
    }

    pub fn with_duration(duration: Duration, label: String) -> Self {
        let mut s = Self::default(label);
        s.lifetime_duration = duration;
        s
    }

    pub fn label_size(self, size: f32) -> Self {
        Self {
            label_size: Some(size),
            ..self
        }
    }
}

#[derive(Clone)]
pub struct ExternalScreen {
    dock_state: DockState<Pane>,
    dimensions: Vec2,
    position: Option<Pos2>,
    owner_plugin_id: Option<u8>,
}

impl ExternalScreen {
    pub fn mock() -> Self {
        Self::new(egui::vec2(1920.0, 1080.0))
    }

    pub(crate) fn dock_state(&mut self) -> &mut DockState<Pane> {
        &mut self.dock_state
    }
}

pub struct BlaulichtApp {
    desktop_mode: bool,
    /// Where the file dialogs start (`--showfile-home`).
    #[cfg_attr(not(feature = "file-dialog"), allow(dead_code))]
    showfile_home: Option<PathBuf>,
    /// The config file's folder: where the file dialogs start without a
    /// showfile or `showfile_home`.
    #[cfg_attr(not(feature = "file-dialog"), allow(dead_code))]
    config_dir: Option<PathBuf>,
    navbar: Navbar,

    main_screen_desktop_mode: ExternalScreen,
    external_screens: Vec<ExternalScreen>,
    plugin_ui_visible_tabs: HashSet<(ScreenId, u8)>,
    /// Plugin window state; frontend-owned, published to the engine through
    /// the desk's client (`client`).
    view_state: view_state::NativeViewState,

    // Example stuff:
    // label: String,
    //
    // value: f32,

    // Multiple graph instances for all audio values
    volume_graph: TimeSeriesGraph,
    beat_volume_graph: TimeSeriesGraph,
    bass_graph: TimeSeriesGraph,
    bass_avg_graph: TimeSeriesGraph,
    bass_avg_short_graph: TimeSeriesGraph,
    collector_snapshot: CollectedAudioSnapshot,
    beat_marker_index: usize,
    beat_marker_has_beat: bool,
    beat_marker_anchor_instant: Instant,
    beat_marker_interval: Duration,
    last_beat_marker_snapshot_time: u64,

    band_energy_graphs: [TimeSeriesGraph; 3],
    /// Learned beat model activations (beat, downbeat), in percent.
    beat_model_graphs: [TimeSeriesGraph; 2],
    last_beat_model_snapshot_time: u64,
    // bpm_graph: TimeSeriesGraph,
    // time_between_beats_graph: TimeSeriesGraph,

    // For continuous rendering
    frame_count: u64,

    animation_time: f32,

    /// The Audio page's smooth-scrolling spectrogram texture.
    spectrogram_ring: pages::SpectrogramRing,

    /// The desk's connection to the engine (`ClientId::Desk`,
    /// `SessionId::DESK`): read show state and sessions, send commands, take
    /// their results, read the live streams, publish view state. Use it
    /// through the `EngineClient` trait.
    client: LocalClient,
    /// Closures waiting for the results of the desk's commands; run by
    /// `handle_events` (`ReplyHandlers::send_then`).
    reply_handlers: ReplyHandlers<BlaulichtApp>,

    fps_samples: VecDeque<f32>,
    tick_speeds: TickSpeeds,
    loop_speed_graph: TimeSeriesGraph,
    plugin_speed_graph: TimeSeriesGraph,

    log_window: LogWindow,

    // Current page
    // pub current_page: AppPage,
    last_heartbeat_frame: u64,
    selected_fixture_group: Option<u8>,

    // Audio devices (`SystemMessage::AudioDevicesView`), without the dummy
    // inputs, and the engine's input (`SystemMessage::AudioSelected`).
    available_audio_devices: Vec<String>,
    selected_audio_input: Option<String>,

    // animation_page: AnimationPageState,
    new_scene_name: String,
    new_scene_dialog_open: bool,
    clone_scene_dialog_open: bool,
    rename_scene_dialog_open: bool,
    rename_scene_name: String,
    delete_scene_dialog_open: bool,
    /// The scene the open clone, rename or delete dialog acts on (the focused
    /// scene when it opened).
    scene_dialog_target: u8,
    current_scene_animations_dialog_open: bool,

    popup: Option<PopupSpec>,
    popup_open_time: Instant,
    init_popup_open_time: Instant,
    engine_initialization_complete: bool,
    startup_progress: startup::StartupProgress,

    set_audio_device_popup_open: bool,
    audio_info_dialog_open: bool,
    manual_bpm_numberpad: Numberpad,

    system_ui_state: SystemUI,

    add_dmx_override_open: bool,
    add_dmx_override_uni: u16,
    add_dmx_override_chan: u16,
    add_dmx_override_value: u8,
    add_dmx_override_uni_numberpad: Numberpad,
    add_dmx_override_chan_numberpad: Numberpad,
    add_dmx_override_value_numberpad: Numberpad,

    dmx_override_dialog_open: bool,

    // Add Fixture dialog state
    add_fixture_open: bool,
    add_fixture_group: Option<u8>,
    add_fixture_name: String,
    add_fixture_start_addr: u16,
    add_fixture_universe_no: u16,
    add_fixture_pos_x: f32,
    add_fixture_pos_y: f32,
    add_fixture_rot_x: f32,
    add_fixture_rot_y: f32,
    add_fixture_rot_z: f32,
    add_fixture_kind: AddFixtureKind,
    add_fixture_kind_dialog_open: bool,
    add_fixture_model_dialog_open: bool,
    add_fixture_selected_moving_head: MovingHead,
    add_fixture_selected_light: Light,
    add_fixture_selected_dimmer: Dimmer,
    // number of fixtures to create in one action
    add_fixture_count: u16,
    // when creating multiple fixtures, optionally step one axis per fixture
    add_fixture_increment_axis: AddFixtureIncrementAxis,
    add_fixture_start_addr_numberpad: Numberpad,
    add_fixture_universe_numberpad: Numberpad,
    add_fixture_pos_x_numberpad: Numberpad,
    add_fixture_pos_y_numberpad: Numberpad,
    add_fixture_rot_x_numberpad: Numberpad,
    add_fixture_rot_y_numberpad: Numberpad,
    add_fixture_rot_z_numberpad: Numberpad,
    add_fixture_count_numberpad: Numberpad,

    setup_fixture_id: u8,

    new_fixture_name: String,
    new_fixture_uni: usize,
    new_fixture_addr: usize,
    new_fixture_pos_x: f32,
    new_fixture_pos_y: f32,
    new_fixture_pos_z: f32,
    new_fixture_rot_x: f32,
    new_fixture_rot_y: f32,
    new_fixture_rot_z: f32,
    edit_fixture_start_addr_numberpad: Numberpad,
    edit_fixture_universe_numberpad: Numberpad,
    edit_fixture_pos_x_numberpad: Numberpad,
    edit_fixture_pos_y_numberpad: Numberpad,
    edit_fixture_pos_z_numberpad: Numberpad,
    edit_fixture_rot_x_numberpad: Numberpad,
    edit_fixture_rot_y_numberpad: Numberpad,
    edit_fixture_rot_z_numberpad: Numberpad,

    add_group_open: bool,
    rename_group_dialog_open: bool,
    rename_group_name: String,
    delete_group_open: bool,
    delete_fixture: Option<(u8, u8)>,
    /// The Setup dialog command (batch add, rename, create) whose reply is
    /// still on its way; the dialog's OK waits for it (see
    /// `setup_reply_pending`).
    setup_reply: Option<blaulicht_engine::hub::CommandSeq>,
    move_fixture_group_dialog_open: bool,
    new_group_name: String,
    /// Setup › Groups: selected folder and the open folder dialog.
    setup_groups_ui: components::SetupFolderUi,
    /// Setup › Scenes: selected folder and the open folder dialog.
    setup_scenes_ui: components::SetupFolderUi,
    /// Setup › Palettes: selected folder and the open folder dialog.
    setup_palettes_ui: components::SetupFolderUi,

    dmx_simulator: DmxSimulator,

    programmer_ui_state: ProgrammerUi,

    view_ui_state: ViewUI,
    view_perf_ui_state: ViewPerfUI,
    animation_ui_state: AnimationUI,
    visualizer_ui_state: VisualizerUiState,
    scene_graph_ui_state: SceneGraphUI,
    palette_ui_state: PaletteUI,

    /// Whether the show has unsaved changes, and when it was last saved
    /// (`SystemMessage::ShowfileStatus`; the engine saves and autosaves).
    save_status: SaveStatus,
    saved_at: Option<SystemTime>,
    /// The show the engine last announced (`SystemMessage::ShowLoaded`); 0
    /// until the first.
    show_generation: u64,
    /// The screen layouts last sent with `HostCommand::SetFrontendSection`.
    published_frontend_section: Option<ShowfileUiState>,
    pending_lifecycle_action: Option<GuardedLifecycleAction>,
    /// The engine ran its shutdown hook (`SystemMessage::Exiting`): close
    /// the window.
    exit_requested: bool,
    allow_close_once: bool,
    /// Accumulated relative numberpad adjustment received from plugins this
    /// frame (see `MainUiEvent::NumberpadAdjust`). Published to the egui
    /// context at the start of every frame and consumed by an open numberpad.
    pending_numberpad_delta: i32,
    recent_fixture_property: jog::RecentFixtureProperty,
}

impl BlaulichtApp {
    fn new_default(
        client: LocalClient,
        desktop_mode: bool,
        showfile_home: Option<PathBuf>,
        config_dir: Option<PathBuf>,
    ) -> Self {
        Self {
            desktop_mode,
            showfile_home,
            config_dir,
            navbar: Navbar::new(AppPage::Logs),
            main_screen_desktop_mode: ExternalScreen::default(),
            external_screens: vec![],
            plugin_ui_visible_tabs: HashSet::new(),
            view_state: view_state::NativeViewState::default(),
            // Example stuff:
            volume_graph: TimeSeriesGraph::new(
                "Volume".to_string(),
                0,
                255,
                egui::Color32::from_rgb(0, 200, 255),
            ),
            beat_volume_graph: TimeSeriesGraph::new(
                "Beat Volume".to_string(),
                0,
                255,
                egui::Color32::from_rgb(0, 200, 255),
            ),
            bass_graph: TimeSeriesGraph::new(
                "Bass".to_string(),
                0,
                255,
                egui::Color32::from_rgb(0, 200, 255),
            ),
            bass_avg_graph: TimeSeriesGraph::new(
                "Bass Avg".to_string(),
                0,
                255,
                egui::Color32::from_rgb(0, 200, 255),
            ),
            bass_avg_short_graph: TimeSeriesGraph::new(
                "Bass Avg Short".to_string(),
                0,
                255,
                egui::Color32::from_rgb(255, 160, 0),
            ),
            band_energy_graphs: [
                TimeSeriesGraph::new("Band Low".to_string(), 0, 1000, Color32::RED)
                    .with_autoscale(),
                TimeSeriesGraph::new("Band Mid".to_string(), 0, 1000, Color32::GREEN)
                    .with_autoscale(),
                TimeSeriesGraph::new("Band High".to_string(), 0, 1000, Color32::BLUE)
                    .with_autoscale(),
            ],
            last_beat_model_snapshot_time: 0,
            beat_model_graphs: [
                TimeSeriesGraph::new(
                    "Beat %".to_string(),
                    0,
                    100,
                    Color32::from_rgb(255, 120, 200),
                ),
                TimeSeriesGraph::new(
                    "Downbeat %".to_string(),
                    0,
                    100,
                    Color32::from_rgb(255, 200, 80),
                ),
            ],
            collector_snapshot: CollectedAudioSnapshot::default(),
            beat_marker_index: 0,
            beat_marker_has_beat: false,
            beat_marker_anchor_instant: Instant::now(),
            beat_marker_interval: Duration::from_millis(1000),
            last_beat_marker_snapshot_time: 0,
            frame_count: 0,
            animation_time: 0.0,
            spectrogram_ring: pages::SpectrogramRing::default(),
            client,
            reply_handlers: ReplyHandlers::default(),
            tick_speeds: TickSpeeds::default(),
            loop_speed_graph: TimeSeriesGraph::new(
                "Loop (ms)".to_string(),
                0,
                blaulicht_engine::mainloop::DMX_TICK_TIME.as_millis() as i32,
                egui::Color32::from_rgb(0, 200, 255),
            )
            .with_autoscale(),
            plugin_speed_graph: TimeSeriesGraph::new(
                "Plugins (µs)".to_string(),
                0,
                1000,
                egui::Color32::from_rgb(255, 200, 0),
            )
            .with_autoscale(),
            fps_samples: VecDeque::with_capacity(5),
            log_window: LogWindow::new(200),
            // current_page: AppPage::Logs,
            last_heartbeat_frame: 0,
            selected_fixture_group: None,
            available_audio_devices: vec![],
            selected_audio_input: None,
            new_scene_name: DEFAULT_NEW_SCENE_NAME.to_string(),
            new_scene_dialog_open: false,
            clone_scene_dialog_open: false,
            rename_scene_dialog_open: false,
            rename_scene_name: String::new(),
            delete_scene_dialog_open: false,
            scene_dialog_target: 0,
            current_scene_animations_dialog_open: false,
            popup: None,
            popup_open_time: Instant::now(),
            init_popup_open_time: Instant::now(),
            engine_initialization_complete: false,
            startup_progress: startup::StartupProgress::default(),
            set_audio_device_popup_open: false,
            audio_info_dialog_open: false,
            manual_bpm_numberpad: Numberpad::new()
                .dialog_title("Manual BPM")
                .field_width(140.0)
                .range(60.0, 220.0),
            add_dmx_override_open: false,
            add_dmx_override_chan: 1,
            add_dmx_override_uni: 0,
            add_dmx_override_value: 0,
            add_dmx_override_uni_numberpad: Numberpad::new()
                .dialog_title("Override Universe")
                .field_width(140.0)
                .range(0.0, (NUM_DMX_UNIVERSES.saturating_sub(1)) as f64)
                .with_random_ids(),
            add_dmx_override_chan_numberpad: Numberpad::new()
                .dialog_title("Override Channel")
                .field_width(140.0)
                .range(1.0, 512.0)
                .with_random_ids(),
            add_dmx_override_value_numberpad: Numberpad::new()
                .dialog_title("Override Value")
                .field_width(140.0)
                .range(0.0, 255.0)
                .with_random_ids(),
            dmx_override_dialog_open: false,
            // Add Fixture defaults
            add_fixture_open: false,
            add_fixture_group: None,
            add_fixture_name: String::from("New Fixture"),
            add_fixture_start_addr: 1,
            add_fixture_pos_x: 0.0,
            add_fixture_pos_y: 0.0,
            add_fixture_rot_x: 0.0,
            add_fixture_rot_y: 0.0,
            add_fixture_rot_z: 0.0,
            add_fixture_kind: AddFixtureKind::MovingHead,
            add_fixture_kind_dialog_open: false,
            add_fixture_model_dialog_open: false,
            add_fixture_selected_moving_head: MovingHead::MartinMac250E,
            add_fixture_selected_light: Light::Generic3ChanNoAlpha,
            add_fixture_selected_dimmer: Dimmer::FogMachineSingle,
            add_fixture_count: 1,
            add_fixture_increment_axis: AddFixtureIncrementAxis::None,
            add_fixture_start_addr_numberpad: Numberpad::new()
                .dialog_title("Start Address")
                .field_width(140.0)
                .range(1.0, 512.0),
            add_fixture_universe_numberpad: Numberpad::new()
                .dialog_title("Universe")
                .field_width(140.0)
                .range(0.0, (NUM_DMX_UNIVERSES.saturating_sub(1)) as f64),
            add_fixture_pos_x_numberpad: Numberpad::new()
                .field_width(140.0)
                .dialog_title("Position X"),
            add_fixture_pos_y_numberpad: Numberpad::new()
                .field_width(140.0)
                .dialog_title("Position Y"),
            add_fixture_rot_x_numberpad: Numberpad::new()
                .field_width(140.0)
                .dialog_title("Rotation X")
                .range(0.0, 360.0),
            add_fixture_rot_y_numberpad: Numberpad::new()
                .field_width(140.0)
                .dialog_title("Rotation Y")
                .range(0.0, 360.0),
            add_fixture_rot_z_numberpad: Numberpad::new()
                .field_width(140.0)
                .dialog_title("Rotation Z")
                .range(0.0, 360.0),
            add_fixture_count_numberpad: Numberpad::new()
                .dialog_title("Fixture Count")
                .field_width(140.0)
                .range(1.0, 200.0),
            setup_fixture_id: 0,
            add_fixture_universe_no: 0,
            new_fixture_name: String::new(),
            new_fixture_addr: 0,
            new_fixture_uni: 0,
            new_fixture_pos_x: 0.0,
            new_fixture_pos_y: 0.0,
            new_fixture_pos_z: 0.0,
            new_fixture_rot_x: 0.0,
            new_fixture_rot_y: 0.0,
            new_fixture_rot_z: 0.0,
            edit_fixture_start_addr_numberpad: Numberpad::new()
                .dialog_title("Start Address")
                .field_width(140.0)
                .range(1.0, 512.0),
            edit_fixture_universe_numberpad: Numberpad::new()
                .dialog_title("Universe")
                .field_width(140.0)
                .range(0.0, (NUM_DMX_UNIVERSES.saturating_sub(1)) as f64),
            edit_fixture_pos_x_numberpad: Numberpad::new()
                .dialog_title("Position X")
                .field_width(140.0)
                .range(0.0, 1000.0),
            edit_fixture_pos_y_numberpad: Numberpad::new()
                .dialog_title("Position Y")
                .field_width(140.0)
                .range(0.0, 1000.0),
            edit_fixture_pos_z_numberpad: Numberpad::new()
                .dialog_title("Position Z")
                .field_width(140.0)
                .range(0.0, 1000.0),
            edit_fixture_rot_x_numberpad: Numberpad::new()
                .dialog_title("Rotation X")
                .field_width(140.0)
                .range(0.0, 360.0),
            edit_fixture_rot_y_numberpad: Numberpad::new()
                .dialog_title("Rotation Y")
                .field_width(140.0)
                .range(0.0, 360.0),
            edit_fixture_rot_z_numberpad: Numberpad::new()
                .dialog_title("Rotation Z")
                .field_width(140.0)
                .range(0.0, 360.0),
            add_group_open: false,
            rename_group_dialog_open: false,
            rename_group_name: String::new(),
            delete_group_open: false,
            delete_fixture: None,
            setup_reply: None,
            move_fixture_group_dialog_open: false,
            new_group_name: DEFAULT_NEW_GROUP_NAME.to_string(),
            setup_groups_ui: components::SetupFolderUi::default(),
            setup_scenes_ui: components::SetupFolderUi::default(),
            setup_palettes_ui: components::SetupFolderUi::default(),
            view_ui_state: ViewUI::default(),
            view_perf_ui_state: ViewPerfUI::default(),
            animation_ui_state: AnimationUI::default(),
            visualizer_ui_state: VisualizerUiState::default(),
            scene_graph_ui_state: SceneGraphUI::default(),
            palette_ui_state: PaletteUI::default(),
            programmer_ui_state: ProgrammerUi {
                scene_overview_animation_edit: AnimationEditState::default(),
                scene_overview_animation_selection_edit: None,
                scene_overview_animation_selection_edit_need_to_load: false,
                close_scene_overview_after_child: false,
                palette_dialog_open: false,
                active_palettes_cache: HashMap::new(),
                scene_changes_dialog_open: false,
                undo_all_changes_confirm_open: false,
                animation_presets: Default::default(),
                master_alpha_held: Default::default(),
                master_speed_held: Default::default(),
            },
            system_ui_state: SystemUI::default(),
            dmx_simulator: DmxSimulator::default(),
            save_status: SaveStatus::NoShowfile,
            saved_at: None,
            show_generation: 0,
            published_frontend_section: None,
            pending_lifecycle_action: None,
            exit_requested: false,
            allow_close_once: false,
            pending_numberpad_delta: 0,
            recent_fixture_property: jog::RecentFixtureProperty::default(),
        }
    }
}
