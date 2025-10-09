use crate::{
    app::{
        components::{LogWindow, TimeSeriesGraph, DEFAULT_NEW_GROUP_NAME, DEFAULT_NEW_SCENE_NAME},
        ui::FileDialogOpenOrigin,
    },
    state::AppStateWrapper,
};
use blaulicht_shared::{FixtureProperty, MathematicalBaseFunction, PhaserDuration, SyncMode};
use egui::Color32;
use egui_file::FileDialog;
use std::time::{Duration, Instant};
use strum::EnumIter;

mod components;
mod pages;
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

/// We derive Deserialize/Serialize so we can persist app state on shutdown.
#[derive(Clone, PartialEq, EnumIter)]
pub enum AppPage {
    Logs,
    System,
    Audio,
    FixturesSetup,
    FixturesPerformance,
    Animations,
}

impl AppPage {
    fn icon(&self) -> &'static str {
        match self {
            AppPage::Logs => egui_phosphor::regular::TERMINAL_WINDOW,
            AppPage::System => egui_phosphor::regular::CPU,
            AppPage::Audio => egui_phosphor::regular::MICROPHONE,
            AppPage::FixturesSetup => egui_phosphor::regular::WRENCH,
            AppPage::FixturesPerformance => egui_phosphor::regular::FADERS,
            AppPage::Animations => egui_phosphor::regular::WAVE_SINE,
        }
    }

    fn short(&self) -> &'static str {
        match self {
            AppPage::Logs => "Logs",
            AppPage::System => "Sys",
            AppPage::Audio => "Audio",
            AppPage::FixturesSetup => "F. Setup",
            AppPage::FixturesPerformance => "F. Perf",
            AppPage::Animations => "Anim",
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

pub struct AnimationPageState {
    pub selected_animation: Option<u8>,
    pub clamp_min: u16,
    pub clamp_max: u16,
    pub base_function: MathematicalBaseFunction,
    pub sync_mode: SyncMode,
    pub timing: PhaserDuration,
    pub create_open: bool,
    pub new_name: String,
    pub new_mode: &'static str,
    pub new_prop: FixtureProperty,
}

pub struct BlaulichtApp {
    // Example stuff:
    label: String,

    value: f32,

    // Multiple graph instances for all audio values
    volume_graph: TimeSeriesGraph,
    beat_volume_graph: TimeSeriesGraph,
    bass_graph: TimeSeriesGraph,
    bass_avg_graph: TimeSeriesGraph,
    bass_avg_short_graph: TimeSeriesGraph,
    bpm_graph: TimeSeriesGraph,
    time_between_beats_graph: TimeSeriesGraph,

    // For continuous rendering
    frame_count: u64,

    animation_time: f32,

    pub data: AppStateWrapper,

    // recv: Receiver<UnifiedMessage>,
    // collector: SignalCollector,
    loop_speed: usize,
    tick_speed: usize,

    // logs: Vec<String>,
    log_window: LogWindow,

    // Current page
    current_page: AppPage,
    last_heartbeat_frame: u64,
    selected_fixture_group: Option<u8>,

    // Audio devices.
    available_audio_devices: Vec<String>,

    animation_page: AnimationPageState,

    new_scene_name: String,
    new_scene_dialog_open: bool,
    clone_scene_dialog_open: bool,
    current_scene_changeset_dialog_open: bool,
    current_scene_animations_dialog_open: bool,
    add_animations_dialog_open: bool,
    add_selected_animation: Option<u8>,

    show_dmx_simulation_universes: [bool; 2],

    popup: Option<PopupSpec>,
    popup_open_time: Instant,

    set_audio_device_popup_open: bool,

    // TODO: move into custom scroll area or whatever
    scene_page_index: usize,

    open_file_dialog: Option<FileDialog>,
    file_dialog_open_origin: FileDialogOpenOrigin,

    reload_dialog_open: bool,

    confirm_shutdown_open: bool,

    add_dmx_override_open: bool,
    add_dmx_override_uni: u16,
    add_dmx_override_chan: u16,
    add_dmx_override_value: u8,

    dmx_override_dialog_open: bool,

    // Add Fixture dialog state
    add_fixture_open: bool,
    add_fixture_group: Option<u8>,
    add_fixture_name: String,
    add_fixture_start_addr: u16,
    add_fixture_universe_no: u16,
    add_fixture_pos_x: usize,
    add_fixture_pos_y: usize,
    // 0 = MovingHead, 1 = Light, 2 = Dimmer
    add_fixture_kind: usize,
    // model index per kind (simple integer mapping to enum variants)
    add_fixture_model_index: usize,
    // number of fixtures to create in one action
    add_fixture_count: u16,

    setup_fixture_id: u8,

    new_fixture_name: String,
    new_fixture_uni: usize,
    new_fixture_addr: usize,

    add_group_open: bool,
    delete_group_open: bool,
    new_group_name: String,
}

impl BlaulichtApp {
    fn new_default(data: AppStateWrapper) -> Self {
        Self {
            // Example stuff:
            label: "Hello World!".to_owned(),
            value: 2.7,
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
            bpm_graph: TimeSeriesGraph::new(
                "BPM".to_string(),
                0,
                255,
                egui::Color32::from_rgb(0, 200, 255),
            ),
            time_between_beats_graph: TimeSeriesGraph::new(
                "Time Between Beats".to_string(),
                0,
                255,
                egui::Color32::from_rgb(0, 200, 255),
            ),
            frame_count: 0,
            animation_time: 0.0,
            data,
            // recv,
            // collector,
            loop_speed: 0,
            tick_speed: 0,
            // logs: vec![],
            log_window: LogWindow::new(100),
            current_page: AppPage::Logs,
            last_heartbeat_frame: 0,
            selected_fixture_group: None,
            available_audio_devices: vec![],
            animation_page: AnimationPageState {
                selected_animation: None,
                clamp_min: 0,
                clamp_max: 255,
                base_function: MathematicalBaseFunction::Sin,
                timing: PhaserDuration::Fixed(1000),
                sync_mode: SyncMode::Synced,
                create_open: false,
                new_mode: "PhaserMath",
                new_name: "ANIM".to_string(),
                new_prop: FixtureProperty::Alpha,
            },
            new_scene_name: DEFAULT_NEW_SCENE_NAME.to_string(),
            new_scene_dialog_open: false,
            clone_scene_dialog_open: false,
            current_scene_changeset_dialog_open: false,
            current_scene_animations_dialog_open: false,
            add_animations_dialog_open: false,
            add_selected_animation: None,
            confirm_shutdown_open: false,
            popup: None,
            popup_open_time: Instant::now(),
            set_audio_device_popup_open: false,
            scene_page_index: 0,
            open_file_dialog: None,
            file_dialog_open_origin: FileDialogOpenOrigin::Save,
            add_dmx_override_open: false,
            add_dmx_override_chan: 1,
            add_dmx_override_uni: 0,
            add_dmx_override_value: 0,
            dmx_override_dialog_open: false,
            // Add Fixture defaults
            add_fixture_open: false,
            add_fixture_group: None,
            add_fixture_name: String::from("New Fixture"),
            add_fixture_start_addr: 1,
            add_fixture_pos_x: 0,
            add_fixture_pos_y: 0,
            add_fixture_kind: 0,
            add_fixture_model_index: 0,
            add_fixture_count: 1,
            setup_fixture_id: 0,
            add_fixture_universe_no: 0,
            show_dmx_simulation_universes: [false; 2],
            new_fixture_name: String::new(),
            new_fixture_addr: 0,
            new_fixture_uni: 0,
            add_group_open: false,
            delete_group_open: false,
            new_group_name: DEFAULT_NEW_GROUP_NAME.to_string(),
            reload_dialog_open: false,
        }
    }
}
