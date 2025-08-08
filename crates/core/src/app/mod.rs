use egui::Color32;

use crate::{
    app::{graph::TimeSeriesGraph, log::LogWindow},
    audio::capture::SignalCollector,
    dmx::animation::{MathematicalBaseFunction, MathematicalPhaser, PhaserDuration},
    routes::AppStateWrapper,
};

mod animations;
mod fixtures;
mod graph;
mod log;
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
#[derive(Clone, PartialEq)]
pub enum AppPage {
    Main,
    Fixtures,
    Animations,
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
    collector: SignalCollector,

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
}

pub struct AnimationPageState {
    pub selected_animation: Option<u8>,
    pub clamp_min: u8,
    pub clamp_max: u8,
    pub base_function: MathematicalBaseFunction,
    pub timing: PhaserDuration,
}
