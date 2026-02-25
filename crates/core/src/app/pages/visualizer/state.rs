use std::sync::{Arc, Mutex};

use egui::Pos2;

use super::data::RenderFixture;
use super::renderer::GlowRenderer;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VisualizerSettings {
    pub show_grid: bool,
    pub show_axes: bool,
    pub brightness: f32,
}

impl Default for VisualizerSettings {
    fn default() -> Self {
        Self {
            show_grid: true,
            show_axes: false,
            brightness: 1.2,
        }
    }
}

pub struct VisualizerUiState {
    pub(super) settings: VisualizerSettings,
    pub(super) camera_yaw: f32,
    pub(super) camera_pitch: f32,
    pub(super) camera_radius: f32,
    pub(super) drag_start_yaw: Option<f32>,
    pub(super) drag_start_pitch: Option<f32>,
    pub(super) drag_last_pos: Option<Pos2>,
    pub(super) shared: Arc<Mutex<GlowShared>>,
}

impl VisualizerUiState {
    fn new_shared() -> Arc<Mutex<GlowShared>> {
        Arc::new(Mutex::new(GlowShared {
            renderer: None,
            last_error: None,
            settings: VisualizerSettings::default(),
            camera_yaw: 0.0,
            camera_pitch: -0.3,
            camera_radius: 12.0,
            time: 0.0,
            fixtures: Vec::new(),
        }))
    }
}

#[derive(Default)]
pub(super) struct GlowShared {
    pub(super) renderer: Option<GlowRenderer>,
    pub(super) last_error: Option<String>,
    pub(super) settings: VisualizerSettings,
    pub(super) camera_yaw: f32,
    pub(super) camera_pitch: f32,
    pub(super) camera_radius: f32,
    pub(super) time: f32,
    pub(super) fixtures: Vec<RenderFixture>,
}

impl GlowShared {
    pub(super) fn ensure_renderer(&mut self, gl: &Arc<egui_glow::glow::Context>) {
        if self.renderer.is_some() || self.last_error.is_some() {
            return;
        }
        match GlowRenderer::new(gl) {
            Ok(renderer) => {
                self.renderer = Some(renderer);
            }
            Err(err) => {
                self.last_error = Some(err);
            }
        }
    }
}

impl Default for VisualizerUiState {
    fn default() -> Self {
        Self {
            settings: VisualizerSettings::default(),
            camera_yaw: 0.0,
            camera_pitch: -0.3,
            camera_radius: 12.0,
            drag_start_yaw: None,
            drag_start_pitch: None,
            drag_last_pos: None,
            shared: Self::new_shared(),
        }
    }
}
