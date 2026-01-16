use crate::{
    app::{
        components::{
            DmxSimulator, LogWindow, Numberpad, TimeSeriesGraph, DEFAULT_NEW_GROUP_NAME,
            DEFAULT_NEW_SCENE_NAME,
        },
        pages::{AddFixtureKind, AnimationEditState, AnimationUI, FixturePerfUi, SystemUI},
        ui::FileDialogOpenOrigin,
    },
    state::{AppStateWrapper, NUM_DMX_UNIVERSES},
};
use blaulicht_shared::fixture::dimmer::Dimmer;
use blaulicht_shared::fixture::light::Light;
use blaulicht_shared::fixture::moving_head::MovingHead;
use blaulicht_shared::{AppPage, CollectedAudioSnapshot};
use egui::Color32;
use egui_file::FileDialog;
use pages::ViewUI;
use std::time::{Duration, Instant};
use strum::EnumIter;

pub mod components;
pub mod pages;
mod plugin_ui;
mod popup;
mod theme;
mod ui;

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
    // Auto-close duration
    pub lifetime_duration: Duration,
    pub button: Option<PopupButtonSpec>,
}

impl PopupSpec {
    pub fn default(label: String) -> Self {
        Self {
            label,
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
}

pub struct BlaulichtApp {
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
    // bpm_graph: TimeSeriesGraph,
    // time_between_beats_graph: TimeSeriesGraph,

    // For continuous rendering
    frame_count: u64,

    animation_time: f32,

    // Spectrogram smooth scrolling
    spectro_scroll_px_offset: f32,
    spectro_last_instant: Instant,

    pub data: AppStateWrapper,

    // recv: Receiver<UnifiedMessage>,
    // collector: SignalCollector,
    loop_speed: usize,
    tick_speed: usize,

    // logs: Vec<String>,
    log_window: LogWindow,

    // Current page
    pub current_page: AppPage,
    last_heartbeat_frame: u64,
    selected_fixture_group: Option<u8>,

    // Audio devices.
    available_audio_devices: Vec<String>,

    // animation_page: AnimationPageState,
    new_scene_name: String,
    new_scene_dialog_open: bool,
    clone_scene_dialog_open: bool,
    rename_scene_dialog_open: bool,
    rename_scene_name: String,
    delete_scene_dialog_open: bool,
    current_scene_changeset_dialog_open: bool,
    current_scene_animations_dialog_open: bool,
    add_animations_dialog_open: bool,
    add_selected_animation: Option<u8>,

    popup: Option<PopupSpec>,
    popup_open_time: Instant,
    init_popup_open_time: Instant,

    set_audio_device_popup_open: bool,

    // TODO: move into custom scroll area or whatever
    scene_page_index: usize,

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
    add_fixture_pos_x: usize,
    add_fixture_pos_y: usize,
    add_fixture_kind: AddFixtureKind,
    add_fixture_kind_dialog_open: bool,
    add_fixture_model_dialog_open: bool,
    add_fixture_selected_moving_head: MovingHead,
    add_fixture_selected_light: Light,
    add_fixture_selected_dimmer: Dimmer,
    // number of fixtures to create in one action
    add_fixture_count: u16,
    add_fixture_start_addr_numberpad: Numberpad,
    add_fixture_universe_numberpad: Numberpad,
    add_fixture_pos_x_numberpad: Numberpad,
    add_fixture_pos_y_numberpad: Numberpad,
    add_fixture_count_numberpad: Numberpad,

    setup_fixture_id: u8,

    new_fixture_name: String,
    new_fixture_uni: usize,
    new_fixture_addr: usize,
    new_fixture_pos_x: usize,
    new_fixture_pos_y: usize,
    new_fixture_pos_z: usize,
    edit_fixture_start_addr_numberpad: Numberpad,
    edit_fixture_universe_numberpad: Numberpad,
    edit_fixture_pos_x_numberpad: Numberpad,
    edit_fixture_pos_y_numberpad: Numberpad,
    edit_fixture_pos_z_numberpad: Numberpad,

    add_group_open: bool,
    delete_group_open: bool,
    new_group_name: String,

    universe_simulations: [DmxSimulator; NUM_DMX_UNIVERSES],

    fixture_perf_ui: FixturePerfUi,

    view_ui_state: ViewUI,
    animation_ui_state: AnimationUI,
}

impl BlaulichtApp {
    fn new_default(data: AppStateWrapper) -> Self {
        Self {
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
                egui::Color32::from_rgb(0, 200, 255),
            ),
            collector_snapshot: CollectedAudioSnapshot::default(),
            frame_count: 0,
            animation_time: 0.0,
            spectro_scroll_px_offset: 0.0,
            spectro_last_instant: Instant::now(),
            data,
            loop_speed: 0,
            tick_speed: 0,
            log_window: LogWindow::new(100),
            current_page: AppPage::Logs,
            last_heartbeat_frame: 0,
            selected_fixture_group: None,
            available_audio_devices: vec![],
            new_scene_name: DEFAULT_NEW_SCENE_NAME.to_string(),
            new_scene_dialog_open: false,
            clone_scene_dialog_open: false,
            rename_scene_dialog_open: false,
            rename_scene_name: String::new(),
            delete_scene_dialog_open: false,
            current_scene_changeset_dialog_open: false,
            current_scene_animations_dialog_open: false,
            add_animations_dialog_open: false,
            add_selected_animation: None,
            popup: None,
            popup_open_time: Instant::now(),
            init_popup_open_time: Instant::now(),
            set_audio_device_popup_open: false,
            scene_page_index: 0,
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
            add_fixture_pos_x: 0,
            add_fixture_pos_y: 0,
            add_fixture_kind: AddFixtureKind::MovingHead,
            add_fixture_kind_dialog_open: false,
            add_fixture_model_dialog_open: false,
            add_fixture_selected_moving_head: MovingHead::MartinMac250E,
            add_fixture_selected_light: Light::Generic3ChanNoAlpha,
            add_fixture_selected_dimmer: Dimmer::FogMachineSingle,
            add_fixture_count: 1,
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
            add_fixture_count_numberpad: Numberpad::new()
                .dialog_title("Fixture Count")
                .field_width(140.0)
                .range(1.0, 64.0),
            setup_fixture_id: 0,
            add_fixture_universe_no: 0,
            new_fixture_name: String::new(),
            new_fixture_addr: 0,
            new_fixture_uni: 0,
            new_fixture_pos_x: 0,
            new_fixture_pos_y: 0,
            new_fixture_pos_z: 0,
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
            add_group_open: false,
            delete_group_open: false,
            new_group_name: DEFAULT_NEW_GROUP_NAME.to_string(),
            view_ui_state: ViewUI::default(),
            animation_ui_state: AnimationUI::default(),
            fixture_perf_ui: FixturePerfUi {
                scene_overview_animation_edit: AnimationEditState::default(),
                scene_overview_animation_selection_edit: None,
                scene_overview_animation_selection_edit_need_to_load: false,
            },
            system_ui_state: SystemUI::default(),
            universe_simulations: [DmxSimulator::default(); NUM_DMX_UNIVERSES],
        }
    }
}
