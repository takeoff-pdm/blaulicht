use std::time::{Duration, Instant};

use egui::Color32;
use egui_file::FileDialog;
use strum::EnumIter;

use crate::{
    app::{
        components::{LogWindow, TimeSeriesGraph},
        pages::DEFAULT_NEW_SCENE_NAME,
        ui::FileDialogOpenOrigin,
    },
    dmx::animation::{MathematicalBaseFunction, PhaserDuration},
    state::AppStateWrapper,
};

mod components;
mod pages;
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
    label: String,
    // Auto-close duration
    lifetime_duration: Duration,
    button: Option<PopupButtonSpec>,
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
    pub timing: PhaserDuration,
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
    current_scene_changeset_dialog_open: bool,
    current_scene_animations_dialog_open: bool,
    show_dmx_simulation: bool,

    popup: Option<PopupSpec>,
    popup_open_time: Instant,

    set_audio_device_popup_open: bool,

    // TODO: move into custom scroll area or whatever
    scene_page_index: usize,

    open_file_dialog: Option<FileDialog>,
    file_dialog_open_origin: FileDialogOpenOrigin,

    confirm_shutdown_open: bool,
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
            },
            new_scene_name: DEFAULT_NEW_SCENE_NAME.to_string(),
            new_scene_dialog_open: false,
            current_scene_changeset_dialog_open: false,
            current_scene_animations_dialog_open: false,
            show_dmx_simulation: false,
            confirm_shutdown_open: false,
            popup: None,
            popup_open_time: Instant::now(),
            set_audio_device_popup_open: false,
            scene_page_index: 0,
            open_file_dialog: None,
            file_dialog_open_origin: FileDialogOpenOrigin::Save,
        }
    }
}
