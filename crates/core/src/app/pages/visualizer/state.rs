use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    path::PathBuf,
    sync::{
        mpsc::{Receiver, Sender},
        Arc, Mutex,
    },
};

use egui::{Id, Pos2};

use crate::stage::{StageAsset, StageScene};
use crate::stage_assets::PreparedStageModel;
use blaulicht_shared::fixture::state::{Position, Rotation};

use super::constants::{DEFAULT_ROOM_DEPTH, DEFAULT_ROOM_WIDTH};
use super::data::RenderSceneSnapshot;
use super::math::Vec3;
use super::renderer::GlowRenderer;

pub(super) const DEFAULT_CAMERA_YAW: f32 = 0.0;
pub(super) const DEFAULT_CAMERA_PITCH: f32 = 0.3;
pub(super) const DEFAULT_CAMERA_RADIUS: f32 = 8.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum VisualizerMode {
    View,
    Edit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EditorView {
    Perspective,
    Top,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EditorTool {
    Select,
    Translate,
    Rotate,
    Scale,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum SelectionKey {
    Fixture(u8, u8),
    Object(u64),
}

#[derive(Clone)]
pub(super) struct EditorSnapshot {
    pub(super) stage: StageScene,
    pub(super) fixtures: BTreeMap<(u8, u8), (Position, Rotation)>,
}

pub(super) struct EditorState {
    pub(super) mode: VisualizerMode,
    pub(super) view: EditorView,
    pub(super) tool: EditorTool,
    pub(super) selection: BTreeSet<SelectionKey>,
    pub(super) snap_enabled: bool,
    pub(super) position_snap: f32,
    pub(super) rotation_snap: f32,
    pub(super) scale_snap: f32,
    pub(super) top_zoom: f32,
    pub(super) top_center: [f32; 2],
    pub(super) drag_before: Option<EditorSnapshot>,
    pub(super) drag_last_world: Option<[f32; 2]>,
    pub(super) drag_last_pointer: Option<Pos2>,
    pub(super) box_select_start: Option<Pos2>,
    pub(super) box_select_additive: bool,
    pub(super) undo: Vec<EditorSnapshot>,
    pub(super) redo: Vec<EditorSnapshot>,
}

impl Default for EditorState {
    fn default() -> Self {
        Self {
            mode: VisualizerMode::View,
            view: EditorView::Perspective,
            tool: EditorTool::Select,
            selection: BTreeSet::new(),
            snap_enabled: true,
            position_snap: 0.25,
            rotation_snap: 15.0,
            scale_snap: 0.1,
            top_zoom: 1.0,
            top_center: [0.0, 0.0],
            drag_before: None,
            drag_last_world: None,
            drag_last_pointer: None,
            box_select_start: None,
            box_select_additive: false,
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }
}

impl EditorState {
    pub(super) fn push_undo(&mut self, snapshot: EditorSnapshot) {
        self.undo.push(snapshot);
        if self.undo.len() > 64 {
            self.undo.remove(0);
        }
        self.redo.clear();
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VisualizerSettings {
    pub show_grid: bool,
    pub show_axes: bool,
    pub show_beams: bool,
    pub show_bodies: bool,
    pub show_labels: bool,
    pub show_diagnostics: bool,
    pub show_room: bool,
    pub room_width: f32,
    pub room_depth: f32,
    pub room_height: f32,
    pub brightness: f32,
}

impl Default for VisualizerSettings {
    fn default() -> Self {
        Self {
            show_grid: true,
            show_axes: false,
            // Start with fixture silhouettes unobscured; beams remain available
            // as an opt-in overlay from the visualizer controls.
            show_beams: false,
            show_bodies: true,
            show_labels: false,
            show_diagnostics: false,
            show_room: true,
            room_width: DEFAULT_ROOM_WIDTH,
            room_depth: DEFAULT_ROOM_DEPTH,
            room_height: 8.0,
            brightness: 1.2,
        }
    }
}

pub struct VisualizerUiState {
    pub(crate) stage: StageScene,
    pub(super) editor: EditorState,
    pub(super) import_dialog: Option<egui_file_dialog::FileDialog>,
    pub(super) import_receiver: Option<Receiver<Result<StageAsset, String>>>,
    pub(super) import_error: Option<String>,
    pub(super) model_cache: HashMap<String, Arc<PreparedStageModel>>,
    pub(super) model_states: HashMap<String, ModelLoadState>,
    pub(super) model_request_sender: crossbeam_channel::Sender<ModelWorkerRequest>,
    pub(super) model_event_receiver: crossbeam_channel::Receiver<ModelWorkerEvent>,
    pub(super) model_generation: u64,
    pub(super) settings: VisualizerSettings,
    pub(super) camera_yaw: f32,
    pub(super) camera_pitch: f32,
    pub(super) camera_radius: f32,
    pub(super) free_camera: bool,
    pub(super) camera_position: Vec3,
    pub(super) drag_start_yaw: Option<f32>,
    pub(super) drag_start_pitch: Option<f32>,
    pub(super) drag_last_pos: Option<Pos2>,
    render_targets: HashMap<Id, VisualizerRenderTarget>,
}

#[derive(Debug, Clone)]
pub(super) enum ModelLoadState {
    Queued,
    Preparing,
    Ready(crate::stage_assets::StageModelStats),
    Failed(String),
}

pub(super) struct ModelWorkerRequest {
    pub(super) key: String,
    pub(super) path: PathBuf,
    pub(super) generation: u64,
}

pub(super) enum ModelWorkerEvent {
    Started {
        key: String,
        generation: u64,
    },
    Finished {
        key: String,
        generation: u64,
        result: Result<PreparedStageModel, String>,
    },
}

#[derive(Clone)]
pub(super) struct VisualizerRenderTarget {
    pub(super) shared: Arc<Mutex<GlowShared>>,
    pub(super) frame: Arc<Mutex<VisualizerFrame>>,
}

impl VisualizerUiState {
    pub(crate) fn load_stage(&mut self, stage: StageScene) {
        self.stage = stage;
        self.editor.selection.clear();
        self.editor.undo.clear();
        self.editor.redo.clear();
        self.editor.drag_before = None;
        self.editor.drag_last_world = None;
        self.editor.drag_last_pointer = None;
        self.editor.box_select_start = None;
        self.model_cache.clear();
        self.model_states.clear();
        self.model_generation = self.model_generation.wrapping_add(1);
        self.import_error = None;
    }

    fn new_shared() -> Arc<Mutex<GlowShared>> {
        Arc::new(Mutex::new(GlowShared {
            renderer: None,
            last_error: None,
        }))
    }

    fn new_frame() -> Arc<Mutex<VisualizerFrame>> {
        Arc::new(Mutex::new(VisualizerFrame {
            settings: VisualizerSettings::default(),
            camera_yaw: DEFAULT_CAMERA_YAW,
            camera_pitch: DEFAULT_CAMERA_PITCH,
            camera_radius: DEFAULT_CAMERA_RADIUS,
            free_camera: false,
            camera_position: Vec3::new(0.0, 0.0, 0.0),
            snapshot: RenderSceneSnapshot::default(),
        }))
    }

    pub(super) fn render_target(&mut self, target_id: Id) -> VisualizerRenderTarget {
        self.render_targets
            .entry(target_id)
            .or_insert_with(|| VisualizerRenderTarget {
                shared: Self::new_shared(),
                frame: Self::new_frame(),
            })
            .clone()
    }
}

#[derive(Default)]
pub(super) struct GlowShared {
    pub(super) renderer: Option<GlowRenderer>,
    pub(super) last_error: Option<String>,
}

#[derive(Clone)]
pub(super) struct VisualizerFrame {
    pub(super) settings: VisualizerSettings,
    pub(super) camera_yaw: f32,
    pub(super) camera_pitch: f32,
    pub(super) camera_radius: f32,
    pub(super) free_camera: bool,
    pub(super) camera_position: Vec3,
    pub(super) snapshot: RenderSceneSnapshot,
}

impl GlowShared {
    pub(super) fn ensure_renderer(&mut self, gl: &Arc<egui_glow::glow::Context>, retry: bool) {
        if retry {
            self.renderer = None;
            self.last_error = None;
        }
        if let Some(renderer) = &self.renderer {
            if Arc::ptr_eq(&renderer.gl, gl) {
                return;
            }
            self.renderer = None;
            self.last_error = None;
        }
        if self.last_error.is_some() {
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

    pub(super) fn retry_renderer(&mut self) {
        self.renderer = None;
        self.last_error = None;
    }
}

impl Default for VisualizerUiState {
    fn default() -> Self {
        let (model_request_sender, model_request_receiver) = crossbeam_channel::bounded(4);
        let (model_event_sender, model_event_receiver) = crossbeam_channel::unbounded();
        std::thread::Builder::new()
            .name("visualizer-model-loader".to_string())
            .spawn(move || {
                while let Ok(request) = model_request_receiver.recv() {
                    let ModelWorkerRequest {
                        key,
                        path,
                        generation,
                    } = request;
                    let _ = model_event_sender.send(ModelWorkerEvent::Started {
                        key: key.clone(),
                        generation,
                    });
                    let started = std::time::Instant::now();
                    let result = crate::stage_assets::load_prepared_model(&path)
                        .map_err(|error| format!("Failed to prepare managed GLB {path:?}: {error}"));
                    let stats = result.as_ref().ok().map(|prepared| prepared.stats);
                    tracing::info!(
                        asset = %key,
                        elapsed_ms = started.elapsed().as_millis(),
                        success = result.is_ok(),
                        triangles = stats.map_or(0, |stats| stats.triangles),
                        texture_bytes = stats.map_or(0, |stats| stats.texture_bytes),
                        textures_downscaled = stats.map_or(0, |stats| stats.textures_downscaled),
                        "Visualizer model preparation finished"
                    );
                    let _ = model_event_sender.send(ModelWorkerEvent::Finished {
                        key,
                        generation,
                        result,
                    });
                }
            })
            .expect("visualizer model loader thread must start");
        Self {
            stage: StageScene::default(),
            editor: EditorState::default(),
            import_dialog: None,
            import_receiver: None,
            import_error: None,
            model_cache: HashMap::new(),
            model_states: HashMap::new(),
            model_request_sender,
            model_event_receiver,
            model_generation: 0,
            settings: VisualizerSettings::default(),
            camera_yaw: DEFAULT_CAMERA_YAW,
            camera_pitch: DEFAULT_CAMERA_PITCH,
            camera_radius: DEFAULT_CAMERA_RADIUS,
            free_camera: false,
            camera_position: Vec3::new(0.0, 0.0, 0.0),
            drag_start_yaw: None,
            drag_start_pitch: None,
            drag_last_pos: None,
            render_targets: HashMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_targets_are_stable_and_isolated_per_visualizer_instance() {
        let mut state = VisualizerUiState::default();
        let main = state.render_target(Id::new("main_visualizer"));
        let main_again = state.render_target(Id::new("main_visualizer"));
        let external = state.render_target(Id::new("external_visualizer"));

        assert!(Arc::ptr_eq(&main.shared, &main_again.shared));
        assert!(Arc::ptr_eq(&main.frame, &main_again.frame));
        assert!(!Arc::ptr_eq(&main.shared, &external.shared));
        assert!(!Arc::ptr_eq(&main.frame, &external.frame));
    }
}
