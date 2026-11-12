use std::sync::Arc;

use egui::{Color32, Context, Pos2, Rect, Sense, Stroke, Vec2};
use egui_glow::CallbackFn;

use crate::app::components::dropdown;
use crate::app::page::PageRenderContext;
use crate::app::view_state::ScreenId;
use crate::app::BlaulichtApp;
use blaulicht_engine::client::EngineClient;
use blaulicht_engine::hub::{CommandSeq, CreatedId, HostCommand, ModelBytes};
use blaulicht_engine::stage::{
    FixturePlacement, FixtureTrussAttachment, StageObject, StageObjectKind, StagePatch,
    TrussConnection, TrussEndpointRef, TrussPart, TrussProfile, TrussSpec,
};
use blaulicht_engine::state::NUM_DMX_UNIVERSES;

use super::constants::{MAX_ROOM_DIMENSION, MIN_ROOM_DIMENSION};
use super::data::{collect_fixtures, fixture_pose, Pose, RenderSceneSnapshot};
use super::math::{
    mat4_look_at, mat4_perspective, mat4_rotation_euler, mat4_transform_dir, project_point, Vec3,
};
use super::state::{
    EditorSnapshot, EditorTool, EditorView, SelectionKey, VisualizerMode, DEFAULT_CAMERA_PITCH,
    DEFAULT_CAMERA_RADIUS, DEFAULT_CAMERA_YAW,
};

impl BlaulichtApp {
    /// Selects `fixtures` in the visualizer's top-down editor and opens it, so
    /// setup pages can point out where a group or fixture is.
    pub(crate) fn show_fixtures_in_visualizer(
        &mut self,
        fixtures: impl IntoIterator<Item = (u8, u8)>,
    ) {
        let editor = &mut self.visualizer_ui_state.editor;
        editor.selection = fixtures
            .into_iter()
            .map(|(gid, fid)| SelectionKey::Fixture(gid, fid))
            .collect();
        editor.mode = VisualizerMode::Edit;
        editor.view = EditorView::Top;
        editor.tool = EditorTool::Select;
        self.navbar
            .navigate_to(blaulicht_shared::AppPage::Visualizer);
    }

    pub fn visualizer_ui(
        &mut self,
        ui: &mut egui::Ui,
        _ctx: &Context,
        screen_id: ScreenId,
        render_context: PageRenderContext,
    ) {
        self.sync_visualizer_stage();
        self.visualizer_import_dialog(ui.ctx());
        // Desktop: page bar and panes (`pages.html#visualizer`). Tiles on
        // dynamic screens (with sub-page tabs) and the kiosk keep the layout
        // below.
        if self.layout.is_desktop() && !render_context.shows_subpage_tabs() {
            self.visualizer_desktop(ui, screen_id, render_context);
            return;
        }
        ui.heading("Visualizer");
        ui.add_space(4.0);

        let spacing = ui.spacing().item_spacing;
        ui.spacing_mut().item_spacing = egui::vec2(6.0, 4.0);

        ui.horizontal_wrapped(|ui| {
            ui.selectable_value(
                &mut self.visualizer_ui_state.editor.mode,
                VisualizerMode::View,
                "View",
            );
            ui.selectable_value(
                &mut self.visualizer_ui_state.editor.mode,
                VisualizerMode::Edit,
                "Edit",
            );
            if self.visualizer_ui_state.editor.mode == VisualizerMode::Edit {
                ui.separator();
                ui.selectable_value(
                    &mut self.visualizer_ui_state.editor.view,
                    EditorView::Perspective,
                    "3D",
                );
                ui.selectable_value(
                    &mut self.visualizer_ui_state.editor.view,
                    EditorView::Top,
                    "Top",
                );
                ui.separator();
                ui.selectable_value(
                    &mut self.visualizer_ui_state.editor.tool,
                    EditorTool::Select,
                    "Select",
                );
                ui.selectable_value(
                    &mut self.visualizer_ui_state.editor.tool,
                    EditorTool::Translate,
                    "Move",
                );
                ui.selectable_value(
                    &mut self.visualizer_ui_state.editor.tool,
                    EditorTool::Rotate,
                    "Rotate",
                );
                ui.selectable_value(
                    &mut self.visualizer_ui_state.editor.tool,
                    EditorTool::Scale,
                    "Scale",
                );
            }
        });

        if self.visualizer_ui_state.editor.mode == VisualizerMode::Edit {
            self.visualizer_editor_toolbar(ui);
        }

        self.visualizer_ui_state.settings.show_room = self.visualizer_ui_state.stage.room.visible;
        self.visualizer_ui_state.settings.room_width = self.visualizer_ui_state.stage.room.width;
        self.visualizer_ui_state.settings.room_depth = self.visualizer_ui_state.stage.room.depth;
        self.visualizer_ui_state.settings.room_height = self.visualizer_ui_state.stage.room.height;

        let mut room_changed = false;
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(&mut self.visualizer_ui_state.settings.show_grid, "Grid");
            ui.checkbox(&mut self.visualizer_ui_state.settings.show_axes, "Axes");
            ui.checkbox(&mut self.visualizer_ui_state.settings.show_beams, "Beams");
            ui.checkbox(&mut self.visualizer_ui_state.settings.show_bodies, "Bodies");
            ui.checkbox(&mut self.visualizer_ui_state.settings.show_labels, "Labels");
            if ui
                .checkbox(&mut self.visualizer_ui_state.settings.show_room, "Room")
                .changed()
            {
                self.visualizer_ui_state.stage.room.visible =
                    self.visualizer_ui_state.settings.show_room;
                room_changed = true;
            }
            ui.checkbox(
                &mut self.visualizer_ui_state.settings.show_diagnostics,
                "Diagnostics",
            );
            if ui.small_button("Reset Cam").clicked() {
                reset_camera(&mut self.visualizer_ui_state);
            }
        });

        self.sync_visualizer_models();
        let universes: Vec<Option<[u8; 513]>> = (0..NUM_DMX_UNIVERSES)
            .map(|universe| self.client.with_dmx_universe(universe, |buffer| *buffer))
            .collect();
        let snapshot = self.client.with_show(|show| {
            collect_fixtures(
                show,
                &universes,
                &self.visualizer_ui_state.stage,
                &self.visualizer_ui_state.fixture_drafts,
                &self.visualizer_ui_state.model_cache,
            )
        });
        if ui.small_button("Fit View").clicked() {
            self.visualizer_ui_state.camera_yaw = DEFAULT_CAMERA_YAW;
            self.visualizer_ui_state.camera_pitch = DEFAULT_CAMERA_PITCH;
            self.visualizer_ui_state.camera_radius =
                (snapshot.bounds.radius * 1.5).clamp(3.0, 60.0);
            self.visualizer_ui_state.free_camera = false;
            self.visualizer_ui_state.camera_position = Vec3::new(0.0, 0.0, 0.0);
        }

        ui.horizontal_wrapped(|ui| {
            ui.label("Bright");
            let mut brightness_pct =
                (self.visualizer_ui_state.settings.brightness * 100.0).round() as i32;
            if ui
                .add_sized(
                    [180.0, 18.0],
                    egui::Slider::new(&mut brightness_pct, 50..=500).show_value(false),
                )
                .changed()
            {
                self.visualizer_ui_state.settings.brightness =
                    (brightness_pct as f32 / 100.0).clamp(0.5, 5.0);
            }
            ui.label(format!("{brightness_pct}%"));
            if self.visualizer_ui_state.settings.show_room {
                ui.separator();
                ui.label("Width");
                if ui
                    .add_sized(
                        [110.0, 18.0],
                        egui::Slider::new(
                            &mut self.visualizer_ui_state.settings.room_width,
                            MIN_ROOM_DIMENSION..=MAX_ROOM_DIMENSION,
                        )
                        .suffix(" m"),
                    )
                    .changed()
                {
                    self.visualizer_ui_state.stage.room.width =
                        self.visualizer_ui_state.settings.room_width;
                    room_changed = true;
                }
                ui.label("Depth");
                if ui
                    .add_sized(
                        [110.0, 18.0],
                        egui::Slider::new(
                            &mut self.visualizer_ui_state.settings.room_depth,
                            MIN_ROOM_DIMENSION..=MAX_ROOM_DIMENSION,
                        )
                        .suffix(" m"),
                    )
                    .changed()
                {
                    self.visualizer_ui_state.stage.room.depth =
                        self.visualizer_ui_state.settings.room_depth;
                    room_changed = true;
                }
                if self.visualizer_ui_state.editor.mode == VisualizerMode::Edit {
                    ui.label("Height");
                    if ui
                        .add_sized(
                            [110.0, 18.0],
                            egui::Slider::new(
                                &mut self.visualizer_ui_state.settings.room_height,
                                MIN_ROOM_DIMENSION..=50.0,
                            )
                            .suffix(" m"),
                        )
                        .changed()
                    {
                        self.visualizer_ui_state.stage.room.height =
                            self.visualizer_ui_state.settings.room_height;
                        room_changed = true;
                    }
                }
            }
        });
        if room_changed {
            self.commit_visualizer_stage();
        }

        ui.add_space(6.0);

        if self.visualizer_ui_state.editor.mode == VisualizerMode::Edit {
            self.visualizer_editor_inspector(ui);
            if self.visualizer_ui_state.editor.view == EditorView::Top {
                self.visualizer_top_ui(ui);
                ui.spacing_mut().item_spacing = spacing;
                return;
            }
        }

        self.visualizer_canvas(ui, snapshot, screen_id, render_context, true);

        ui.spacing_mut().item_spacing = spacing;
    }

    /// The 3D view, filling what is left of `ui`, with its status lines
    /// below when `status_below` (the desktop shows them in the Stage pane,
    /// see [`Self::visualizer_status`]). Returns the canvas id.
    fn visualizer_canvas(
        &mut self,
        ui: &mut egui::Ui,
        snapshot: RenderSceneSnapshot,
        screen_id: ScreenId,
        render_context: PageRenderContext,
        status_below: bool,
    ) -> egui::Id {
        let minimum_canvas_height = if render_context.is_dynamic() && render_context.short {
            80.0
        } else {
            120.0
        };
        let canvas_size = Vec2::new(
            ui.available_width(),
            ui.available_height().max(minimum_canvas_height),
        );
        let (rect, _response) = ui.allocate_exact_size(canvas_size, Sense::hover());
        let canvas_id = ui.make_persistent_id("visualizer_canvas");
        let response = ui.interact(rect, canvas_id, Sense::drag());

        if response.clicked() {
            response.request_focus();
        }

        let focused = response.has_focus() || response.hovered();
        let reset_requested = ui.input(|input| focused && input.key_pressed(egui::Key::Escape));
        if reset_requested {
            reset_camera(&mut self.visualizer_ui_state);
        }

        if focused {
            let (dt, modifiers, w, a, s, d, space) = ui.input(|input| {
                (
                    input.unstable_dt.clamp(1.0 / 240.0, 0.1),
                    input.modifiers,
                    input.key_down(egui::Key::W),
                    input.key_down(egui::Key::A),
                    input.key_down(egui::Key::S),
                    input.key_down(egui::Key::D),
                    input.key_down(egui::Key::Space),
                )
            });
            let down = modifiers.ctrl;
            let movement_requested = w || a || s || d || space || down;
            if movement_requested && !self.visualizer_ui_state.free_camera && !reset_requested {
                enter_free_camera(&mut self.visualizer_ui_state);
            }

            let forward = camera_forward(
                self.visualizer_ui_state.camera_yaw,
                self.visualizer_ui_state.camera_pitch,
            );
            let right = Vec3::new(
                self.visualizer_ui_state.camera_yaw.cos(),
                0.0,
                self.visualizer_ui_state.camera_yaw.sin(),
            );
            let mut movement = Vec3::new(0.0, 0.0, 0.0);
            if w {
                movement = movement.add(forward);
            }
            if s {
                movement = movement.sub(forward);
            }
            if d {
                movement = movement.add(right);
            }
            if a {
                movement = movement.sub(right);
            }
            if space {
                movement.y += 1.0;
            }
            if down {
                movement.y -= 1.0;
            }
            if movement.norm() > 0.0 && !reset_requested {
                let speed = if modifiers.shift { 24.0 } else { 8.0 } * dt;
                self.visualizer_ui_state.camera_position = self
                    .visualizer_ui_state
                    .camera_position
                    .add(movement.normalize().scale(speed));
            }
        }

        let perspective_edit = self.visualizer_ui_state.editor.mode == VisualizerMode::Edit;
        let camera_button = if perspective_edit {
            egui::PointerButton::Secondary
        } else {
            egui::PointerButton::Primary
        };
        let projected_items = project_scene_items(rect, &snapshot, &self.visualizer_ui_state);
        let projected_endpoints = project_selected_truss_endpoints(
            rect,
            &self.visualizer_ui_state,
            &self.visualizer_ui_state.editor.selection,
        );

        if perspective_edit && response.clicked_by(egui::PointerButton::Primary) {
            if let Some(pointer) = response.interact_pointer_pos() {
                let hit =
                    hit_test_projected(pointer, &projected_items, &self.visualizer_ui_state.stage);
                let shift = ui.input(|input| input.modifiers.shift);
                match hit {
                    Some(hit) => {
                        if !shift {
                            self.visualizer_ui_state.editor.selection.clear();
                        }
                        self.visualizer_ui_state.editor.selection.insert(hit);
                    }
                    None if !shift => self.visualizer_ui_state.editor.selection.clear(),
                    None => {}
                }
            }
        }

        if perspective_edit && response.drag_started_by(egui::PointerButton::Primary) {
            if let Some(pointer) = response.interact_pointer_pos() {
                let hit =
                    hit_test_projected(pointer, &projected_items, &self.visualizer_ui_state.stage);
                let shift = ui.input(|input| input.modifiers.shift);
                if let Some(hit) = hit {
                    if !shift && !self.visualizer_ui_state.editor.selection.contains(&hit) {
                        self.visualizer_ui_state.editor.selection.clear();
                    }
                    self.visualizer_ui_state.editor.selection.insert(hit);
                    if self.visualizer_ui_state.editor.tool != EditorTool::Select {
                        self.visualizer_ui_state.editor.drag_before =
                            Some(self.capture_visualizer_snapshot());
                        self.visualizer_ui_state.editor.drag_last_pointer = Some(pointer);
                    }
                } else if !shift {
                    self.visualizer_ui_state.editor.selection.clear();
                }
            }
        }

        if perspective_edit && response.dragged_by(egui::PointerButton::Primary) {
            if let (Some(pointer), Some(last)) = (
                response.interact_pointer_pos(),
                self.visualizer_ui_state.editor.drag_last_pointer,
            ) {
                let delta = pointer - last;
                let meters_per_pixel =
                    (self.visualizer_ui_state.camera_radius * 2.0 * (22.5_f32.to_radians()).tan()
                        / rect.height().max(1.0))
                    .clamp(0.001, 1.0);
                self.transform_visualizer_selection(
                    self.visualizer_ui_state.editor.tool,
                    delta,
                    meters_per_pixel,
                );
                self.visualizer_ui_state.editor.drag_last_pointer = Some(pointer);
            }
        }

        if perspective_edit && response.drag_stopped_by(egui::PointerButton::Primary) {
            if self
                .visualizer_ui_state
                .editor
                .drag_last_pointer
                .take()
                .is_some()
            {
                self.snap_visualizer_selection();
                if let Some(before) = self.visualizer_ui_state.editor.drag_before.take() {
                    self.visualizer_ui_state.editor.push_undo(before);
                }
                self.commit_visualizer_stage();
            }
        }

        if response.drag_started_by(camera_button) {
            self.visualizer_ui_state.drag_start_yaw = Some(self.visualizer_ui_state.camera_yaw);
            self.visualizer_ui_state.drag_start_pitch = Some(self.visualizer_ui_state.camera_pitch);
            self.visualizer_ui_state.drag_last_pos = response.interact_pointer_pos();
        }

        if response.dragged_by(camera_button) {
            if let (Some(start_yaw), Some(start_pitch)) = (
                self.visualizer_ui_state.drag_start_yaw,
                self.visualizer_ui_state.drag_start_pitch,
            ) {
                if let Some(pos) = response.interact_pointer_pos() {
                    if let Some(last_pos) = self.visualizer_ui_state.drag_last_pos {
                        let delta = pos - last_pos;
                        let rotate_speed = 0.02;
                        self.visualizer_ui_state.camera_yaw += delta.x * rotate_speed;
                        self.visualizer_ui_state.camera_pitch =
                            (self.visualizer_ui_state.camera_pitch - delta.y * rotate_speed)
                                .clamp(-1.5, 1.2);
                    } else {
                        self.visualizer_ui_state.camera_yaw = start_yaw;
                        self.visualizer_ui_state.camera_pitch = start_pitch;
                    }
                    self.visualizer_ui_state.drag_last_pos = Some(pos);
                }
            }
        }

        if response.drag_stopped_by(camera_button) {
            self.visualizer_ui_state.drag_start_yaw = None;
            self.visualizer_ui_state.drag_start_pitch = None;
            self.visualizer_ui_state.drag_last_pos = None;
        }

        if response.hovered() {
            let zoom_delta = ui.input(|i| i.smooth_scroll_delta.y);
            if zoom_delta.abs() > 0.0 {
                if self.visualizer_ui_state.free_camera {
                    let forward = camera_forward(
                        self.visualizer_ui_state.camera_yaw,
                        self.visualizer_ui_state.camera_pitch,
                    );
                    self.visualizer_ui_state.camera_position = self
                        .visualizer_ui_state
                        .camera_position
                        .add(forward.scale(zoom_delta * 0.02));
                } else {
                    self.visualizer_ui_state.camera_radius =
                        (self.visualizer_ui_state.camera_radius - zoom_delta * 0.02)
                            .clamp(2.0, 80.0);
                }
            }
        }

        // A screen can contain multiple visualizer tiles. Each callback needs its
        // own GL resources and frame snapshot so one tile cannot overwrite another.
        let render_target = self
            .visualizer_ui_state
            .render_target(egui::Id::new((screen_id, canvas_id)));
        {
            if let Ok(mut frame) = render_target.frame.lock() {
                frame.settings = self.visualizer_ui_state.settings;
                frame.camera_yaw = self.visualizer_ui_state.camera_yaw;
                frame.camera_pitch = self.visualizer_ui_state.camera_pitch;
                frame.camera_radius = self.visualizer_ui_state.camera_radius;
                frame.free_camera = self.visualizer_ui_state.free_camera;
                frame.camera_position = self.visualizer_ui_state.camera_position;
                if snapshot.error.is_some() && !frame.snapshot.fixtures.is_empty() {
                    frame.snapshot.error = snapshot.error;
                    frame.snapshot.skipped_fixtures = snapshot.skipped_fixtures;
                } else {
                    frame.snapshot = snapshot;
                }
            }
        }

        let shared = render_target.shared.clone();
        let frame = render_target.frame.clone();
        let callback = CallbackFn::new(move |info, painter| {
            let Ok(frame) = frame.lock() else {
                return;
            };
            let settings = frame.settings;
            let yaw = frame.camera_yaw;
            let pitch = frame.camera_pitch;
            let radius = frame.camera_radius;
            let free_camera = frame.free_camera;
            let camera_position = frame.camera_position;
            let snapshot = frame.snapshot.clone();
            drop(frame);
            let Ok(mut shared) = shared.lock() else {
                return;
            };
            shared.ensure_renderer(painter.gl(), false);
            let runtime_error = if let Some(renderer) = shared.renderer.as_mut() {
                renderer.render(
                    painter.gl(),
                    &settings,
                    yaw,
                    pitch,
                    radius,
                    free_camera,
                    camera_position,
                    &snapshot,
                    info,
                );
                renderer.take_runtime_error()
            } else {
                None
            };
            if let Some(error) = runtime_error {
                shared.renderer = None;
                shared.last_error = Some(error);
            }
        });

        ui.painter().add(egui::Shape::Callback(egui::PaintCallback {
            rect,
            callback: Arc::new(callback),
        }));
        if perspective_edit {
            for (key, center) in &projected_items {
                if self.visualizer_ui_state.editor.selection.contains(key) {
                    ui.painter().circle_stroke(
                        *center,
                        11.0,
                        Stroke::new(2.0_f32, Color32::LIGHT_BLUE),
                    );
                }
            }
            for endpoint in projected_endpoints {
                ui.painter()
                    .circle_filled(endpoint, 4.0, Color32::LIGHT_GREEN);
                ui.painter()
                    .circle_stroke(endpoint, 6.0, Stroke::new(1.0_f32, Color32::BLACK));
            }
        }

        let shared_status = render_target
            .shared
            .lock()
            .ok()
            .map(|shared| (shared.last_error.clone(),));
        let gpu_progress = render_target
            .shared
            .lock()
            .ok()
            .and_then(|shared| {
                shared
                    .renderer
                    .as_ref()
                    .map(|renderer| renderer.imported_model_progress())
            })
            .unwrap_or_default();
        let cpu_loading = self
            .visualizer_ui_state
            .model_states
            .values()
            .filter(|state| {
                matches!(
                    state,
                    super::state::ModelLoadState::Queued | super::state::ModelLoadState::Preparing
                )
            })
            .count();
        let cpu_failure = self
            .visualizer_ui_state
            .model_states
            .iter()
            .find_map(|(key, state)| match state {
                super::state::ModelLoadState::Failed(error) => Some((key.clone(), error.clone())),
                _ => None,
            });
        let gpu_failure = gpu_progress.failed_assets.first().cloned();
        let asset_failure = cpu_failure.or(gpu_failure);
        let mut remove_assets = None;
        if let Some((key, error)) = asset_failure {
            let asset_name = self
                .visualizer_ui_state
                .stage
                .assets
                .values()
                .find(|asset| asset.content_hash == key)
                .map(|asset| asset.original_name.clone())
                .unwrap_or_else(|| "Imported model".to_string());
            egui::Area::new(canvas_id.with("asset_error"))
                .order(egui::Order::Foreground)
                .fixed_pos(rect.center() - egui::vec2(180.0, 70.0))
                .show(ui.ctx(), |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.set_width(360.0);
                        ui.strong(format!("Could not load {asset_name}"));
                        ui.colored_label(Color32::LIGHT_RED, &error);
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            if ui.button("Retry").clicked() {
                                self.visualizer_ui_state.model_states.remove(&key);
                                self.visualizer_ui_state.model_cache.remove(&key);
                                if let Ok(mut shared) = render_target.shared.lock() {
                                    if let Some(renderer) = shared.renderer.as_mut() {
                                        renderer.retry_imported_model(&key);
                                    }
                                }
                                self.visualizer_ui_state.import_error = None;
                            }
                            if ui.button("Remove from Stage").clicked() {
                                remove_assets = Some(key.clone());
                            }
                        });
                    });
                });
        } else if cpu_loading > 0 || gpu_progress.uploading_assets > 0 {
            let (phase, progress) = if cpu_loading > 0 {
                (format!("Preparing {cpu_loading} model(s)..."), None)
            } else {
                let fraction = if gpu_progress.total_steps == 0 {
                    0.0
                } else {
                    gpu_progress.completed_steps as f32 / gpu_progress.total_steps as f32
                };
                (
                    format!("Uploading {} model(s)...", gpu_progress.uploading_assets),
                    Some(fraction),
                )
            };
            egui::Area::new(canvas_id.with("asset_loading"))
                .order(egui::Order::Foreground)
                .fixed_pos(rect.center() - egui::vec2(130.0, 34.0))
                .show(ui.ctx(), |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.set_width(260.0);
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.strong(phase);
                        });
                        if let Some(progress) = progress {
                            ui.add(egui::ProgressBar::new(progress.clamp(0.0, 1.0)));
                        }
                    });
                });
        }
        if let Some(key) = remove_assets {
            let asset_ids: Vec<u64> = self
                .visualizer_ui_state
                .stage
                .assets
                .iter()
                .filter_map(|(id, asset)| (asset.content_hash == key).then_some(*id))
                .collect();
            self.visualizer_ui_state.model_states.remove(&key);
            self.visualizer_ui_state.model_cache.remove(&key);
            self.visualizer_ui_state.editor.selection.clear();
            self.visualizer_ui_state.import_error = None;
            self.send_stage_command(HostCommand::RemoveStageAssets(asset_ids), |_, _| {});
        }
        let frame_status = render_target.frame.lock().ok().map(|frame| {
            (
                frame.snapshot.skipped_fixtures,
                frame.snapshot.fixtures.len(),
                frame.snapshot.error.clone(),
            )
        });
        if status_below {
            if let Some((err,)) = shared_status {
                ui.add_space(6.0);
                if let Some(err) = err {
                    ui.colored_label(egui::Color32::LIGHT_RED, format!("Renderer error: {err}"));
                    if ui.small_button("Retry Renderer").clicked() {
                        if let Ok(mut shared) = render_target.shared.lock() {
                            shared.retry_renderer();
                        }
                    }
                }
                if let Some((skipped, fixture_count, Some(err))) = frame_status {
                    ui.colored_label(egui::Color32::YELLOW, format!("Scene data: {err}"));
                    if self.visualizer_ui_state.settings.show_diagnostics {
                        ui.label(format!("Fixtures: {fixture_count}, skipped: {skipped}"));
                    }
                } else if let Some((skipped, fixture_count, None)) = frame_status {
                    if self.visualizer_ui_state.settings.show_diagnostics {
                        ui.label(format!("Fixtures: {fixture_count}, skipped: {skipped}"));
                    }
                }
            } else if let Some((skipped, fixture_count, snapshot_error)) = frame_status {
                ui.add_space(6.0);
                if let Some(err) = snapshot_error {
                    ui.colored_label(egui::Color32::YELLOW, format!("Scene data: {err}"));
                }
                if self.visualizer_ui_state.settings.show_diagnostics {
                    ui.label(format!("Fixtures: {fixture_count}, skipped: {skipped}"));
                }
            }
        }
        canvas_id
    }

    /// Keeps the working copy of the stage in step with the engine: replaced
    /// by the engine's stage when it changed, unless a drag is running or a
    /// reply to the editor's own command is outstanding (then the working
    /// copy is ahead and must not snap back).
    fn sync_visualizer_stage(&mut self) {
        let state = &mut self.visualizer_ui_state;
        if let Some(seq) = state.stage_reply {
            if self.reply_handlers.is_waiting(seq) {
                return;
            }
            // A refusal leaves the working copy ahead of the engine.
            state.stage_reply = None;
            state.stage_revision = None;
        }
        if state.editor.drag_before.is_some() {
            return;
        }
        state.fixture_drafts.clear();
        state.fixture_drafts_sent.clear();
        let known = state.stage_revision;
        let fresh = self.client.with_show(|show| {
            (Some(show.stage_revision()) != known)
                .then(|| (show.stage_revision(), show.stage().clone()))
        });
        if let Some((revision, stage)) = fresh {
            state.stage_base = stage.clone();
            state.stage = stage;
            state.stage_revision = Some(revision);
        }
    }

    /// Sends what changed in the working copy (and the fixtures moved by
    /// hand) since the last send, as one `HostCommand::PatchStage`.
    fn commit_visualizer_stage(&mut self) {
        let state = &mut self.visualizer_ui_state;
        let mut patch = StagePatch::between(&state.stage_base, &state.stage);
        patch.fixtures = state
            .fixture_drafts
            .iter()
            .filter(|(key, pose)| state.fixture_drafts_sent.get(*key) != Some(*pose))
            .map(|(&key, &pose)| placement(key, pose))
            .collect();
        if patch.is_empty() {
            return;
        }
        state.stage_base = state.stage.clone();
        state.fixture_drafts_sent = state.fixture_drafts.clone();
        self.send_stage_command(HostCommand::PatchStage(Box::new(patch)), |_, _| {});
    }

    /// Sends a stage command (a refusal shows as a popup) and holds the
    /// working copy until its reply.
    fn send_stage_command(
        &mut self,
        command: HostCommand,
        on_ok: impl FnOnce(&mut Self, Option<CreatedId>) + 'static,
    ) -> CommandSeq {
        let seq = self.send_command(command, on_ok);
        self.visualizer_ui_state.stage_reply = Some(seq);
        seq
    }

    /// Where fixture `key` is drawn ([`fixture_pose`]), and its name.
    fn visualizer_fixture(&self, key: (u8, u8)) -> Option<(String, Pose)> {
        let state = &self.visualizer_ui_state;
        self.client.with_show(|show| {
            let fixture = show.0.groups.get(&key.0)?.fixtures.get(&key.1)?;
            Some((
                fixture.name.clone(),
                fixture_pose(&state.stage, &state.fixture_drafts, key, fixture),
            ))
        })
    }

    /// Moves fixtures in the editor: `edit` changes each one's drawn pose,
    /// and the fixture comes off its truss. Sent with the next commit.
    fn draft_fixture_poses(&mut self, keys: &[(u8, u8)], edit: impl Fn(&mut Pose)) {
        let poses: Vec<_> = keys
            .iter()
            .filter_map(|&key| Some((key, self.visualizer_fixture(key)?.1)))
            .collect();
        for (key, mut pose) in poses {
            self.detach_fixture_from_truss(key.0, key.1);
            edit(&mut pose);
            self.visualizer_ui_state.fixture_drafts.insert(key, pose);
        }
    }

    fn capture_visualizer_snapshot(&self) -> EditorSnapshot {
        let state = &self.visualizer_ui_state;
        let fixtures = self.client.with_show(|show| {
            show.0
                .groups
                .iter()
                .flat_map(|(gid, group)| {
                    group.fixtures.iter().map(move |(fid, fixture)| {
                        let key = (*gid, *fid);
                        (
                            key,
                            fixture_pose(&state.stage, &state.fixture_drafts, key, fixture),
                        )
                    })
                })
                .collect()
        });
        EditorSnapshot {
            stage: state.stage.clone(),
            fixtures,
        }
    }

    /// Undo and redo replace the whole stage (`HostCommand::RestoreStage`):
    /// the engine changes the stage only by command, so nothing it did on
    /// its own is lost.
    fn restore_visualizer_snapshot(&mut self, snapshot: EditorSnapshot) {
        let state = &mut self.visualizer_ui_state;
        state.stage = snapshot.stage.clone();
        state.stage_base = snapshot.stage.clone();
        state.fixture_drafts = snapshot.fixtures.clone();
        state.fixture_drafts_sent = snapshot.fixtures.clone();
        let fixtures = snapshot
            .fixtures
            .into_iter()
            .map(|(key, pose)| placement(key, pose))
            .collect();
        self.send_stage_command(
            HostCommand::RestoreStage {
                stage: Box::new(snapshot.stage),
                fixtures,
            },
            |_, _| {},
        );
    }

    fn detach_fixture_from_truss(&mut self, gid: u8, fid: u8) -> bool {
        let previous_len = self.visualizer_ui_state.stage.fixture_attachments.len();
        self.visualizer_ui_state
            .stage
            .fixture_attachments
            .retain(|attachment| attachment.group_id != gid || attachment.fixture_id != fid);
        previous_len != self.visualizer_ui_state.stage.fixture_attachments.len()
    }

    fn visualizer_undo(&mut self) {
        let Some(previous) = self.visualizer_ui_state.editor.undo.pop() else {
            return;
        };
        let current = self.capture_visualizer_snapshot();
        self.visualizer_ui_state.editor.redo.push(current);
        self.restore_visualizer_snapshot(previous);
    }

    fn visualizer_redo(&mut self) {
        let Some(next) = self.visualizer_ui_state.editor.redo.pop() else {
            return;
        };
        let current = self.capture_visualizer_snapshot();
        self.visualizer_ui_state.editor.undo.push(current);
        self.restore_visualizer_snapshot(next);
    }

    fn visualizer_editor_toolbar(&mut self, ui: &mut egui::Ui) {
        let keyboard_undo = ui.input(|input| {
            input.modifiers.command && input.key_pressed(egui::Key::Z) && !input.modifiers.shift
        });
        let keyboard_redo = ui.input(|input| {
            input.modifiers.command
                && (input.key_pressed(egui::Key::Y)
                    || (input.modifiers.shift && input.key_pressed(egui::Key::Z)))
        });
        if keyboard_undo {
            self.visualizer_undo();
        }
        if keyboard_redo {
            self.visualizer_redo();
        }

        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    !self.visualizer_ui_state.editor.undo.is_empty(),
                    egui::Button::new("Undo"),
                )
                .clicked()
            {
                self.visualizer_undo();
            }
            if ui
                .add_enabled(
                    !self.visualizer_ui_state.editor.redo.is_empty(),
                    egui::Button::new("Redo"),
                )
                .clicked()
            {
                self.visualizer_redo();
            }

            ui.separator();
            ui.checkbox(&mut self.visualizer_ui_state.editor.snap_enabled, "Snap");
            if self.visualizer_ui_state.editor.snap_enabled {
                ui.add(
                    egui::DragValue::new(&mut self.visualizer_ui_state.editor.position_snap)
                        .range(0.01..=10.0)
                        .speed(0.05)
                        .suffix(" m"),
                );
                ui.add(
                    egui::DragValue::new(&mut self.visualizer_ui_state.editor.rotation_snap)
                        .range(1.0..=90.0)
                        .speed(1.0)
                        .suffix(" deg"),
                );
            }

            ui.separator();
            ui.menu_button("Add", |ui| self.visualizer_add_menu(ui));
            #[cfg(feature = "file-dialog")]
            let showfile = self
                .client
                .with_show(|show| show.showfile().map(|path| path.to_path_buf()));
            let importing = self.visualizer_importing();
            #[cfg(feature = "file-dialog")]
            if ui
                .add_enabled(
                    showfile.is_some() && !importing,
                    egui::Button::new("Import GLB"),
                )
                .clicked()
            {
                self.visualizer_open_import(showfile);
            }
            if importing {
                ui.spinner();
            }
            if let Some(error) = &self.visualizer_ui_state.import_error {
                ui.colored_label(Color32::LIGHT_RED, error);
            }

            let sel = self.visualizer_selection_info();
            if ui
                .add_enabled(sel.can_attach(), egui::Button::new("Attach"))
                .on_disabled_hover_text("Select one fixture and one truss")
                .clicked()
            {
                self.visualizer_attach(&sel);
            }
            if ui
                .add_enabled(
                    self.visualizer_can_detach(&sel),
                    egui::Button::new("Detach"),
                )
                .clicked()
            {
                self.visualizer_detach(&sel);
            }
            if ui
                .add_enabled(sel.only_objects(), egui::Button::new("Duplicate"))
                .clicked()
            {
                self.visualizer_duplicate(&sel);
            }
            if ui
                .add_enabled(sel.only_objects(), egui::Button::new("Delete"))
                .clicked()
            {
                self.visualizer_delete(sel);
            }
        });
    }

    /// The Add menu's entries: stage objects and truss parts.
    fn visualizer_add_menu(&mut self, ui: &mut egui::Ui) {
        for (label, kind) in [
            ("Platform", StageObjectKind::Platform),
            ("Wall", StageObjectKind::Wall),
            ("Speaker", StageObjectKind::Speaker),
            ("Screen", StageObjectKind::Screen),
            ("Box", StageObjectKind::Box),
        ] {
            if ui.button(label).clicked() {
                let before = self.capture_visualizer_snapshot();
                let index = self.visualizer_ui_state.stage.objects.len() + 1;
                self.add_visualizer_object(StageObject::preset(kind, index), true);
                self.visualizer_ui_state.editor.push_undo(before);
                ui.close();
            }
        }
        ui.separator();
        ui.menu_button("Truss", |ui| {
            for profile in TrussProfile::ALL {
                ui.menu_button(profile.label(), |ui| {
                    for part in TrussPart::ALL {
                        if ui.button(part.label()).clicked() {
                            let index = self.visualizer_ui_state.stage.objects.len() + 1;
                            let truss = StageObject::truss_preset(
                                index,
                                TrussSpec {
                                    profile,
                                    part,
                                    ..Default::default()
                                },
                            );
                            let before = self.capture_visualizer_snapshot();
                            self.add_visualizer_object(truss, true);
                            self.visualizer_ui_state.editor.push_undo(before);
                            ui.close();
                        }
                    }
                });
            }
        });
    }

    /// Whether a GLB import is being read or waits for the engine.
    fn visualizer_importing(&self) -> bool {
        self.visualizer_ui_state.import_receiver.is_some()
            || self
                .visualizer_ui_state
                .import_reply
                .is_some_and(|seq| self.reply_handlers.is_waiting(seq))
    }

    /// Opens the GLB file dialog next to the showfile.
    #[cfg(feature = "file-dialog")]
    fn visualizer_open_import(&mut self, showfile: Option<std::path::PathBuf>) {
        let mut dialog = egui_file_dialog::FileDialog::new()
            .add_file_filter_extensions("Binary glTF", vec!["glb"])
            .default_file_filter("Binary glTF")
            .as_modal(true)
            .default_size(egui::vec2(850.0, 540.0));
        if let Some(parent) = showfile
            .as_deref()
            .and_then(|path| path.parent().map(|parent| parent.to_path_buf()))
        {
            dialog = dialog.initial_directory(parent);
        }
        dialog.pick_file();
        self.visualizer_ui_state.import_dialog = Some(dialog);
    }

    fn visualizer_selection_info(&self) -> VisualizerSelection {
        let selection = &self.visualizer_ui_state.editor.selection;
        let objects: Vec<u64> = selection
            .iter()
            .filter_map(|key| match key {
                SelectionKey::Object(id) => Some(*id),
                SelectionKey::Fixture(_, _) => None,
            })
            .collect();
        let fixture = selection.iter().find_map(|key| match key {
            SelectionKey::Fixture(gid, fid) => Some((*gid, *fid)),
            SelectionKey::Object(_) => None,
        });
        let truss = objects
            .iter()
            .copied()
            .find(|id| self.visualizer_ui_state.stage.truss_spec(*id).is_some());
        VisualizerSelection {
            count: selection.len(),
            objects,
            fixture,
            truss,
        }
    }

    fn visualizer_can_detach(&self, sel: &VisualizerSelection) -> bool {
        sel.fixture.is_some_and(|key| {
            self.visualizer_ui_state
                .stage
                .fixture_attachments
                .iter()
                .any(|attachment| attachment.group_id == key.0 && attachment.fixture_id == key.1)
        })
    }

    /// Mounts the selected fixture on the selected truss, at the nearest
    /// point.
    fn visualizer_attach(&mut self, sel: &VisualizerSelection) {
        let (Some((gid, fid)), Some(truss_id)) = (sel.fixture, sel.truss) else {
            return;
        };
        let fixture_position = self
            .visualizer_fixture((gid, fid))
            .map(|(_, (position, _))| Vec3::new(position[0], position[1], position[2]));
        let mount = fixture_position.and_then(|position| {
            let object = self.visualizer_ui_state.stage.objects.get(&truss_id)?;
            let spec = self.visualizer_ui_state.stage.truss_spec(truss_id)?;
            super::truss::nearest_mount(spec, &object.transform, position)
        });
        if let Some((branch, distance_m, _)) = mount {
            let before = self.capture_visualizer_snapshot();
            self.detach_fixture_from_truss(gid, fid);
            self.visualizer_ui_state
                .stage
                .fixture_attachments
                .push(FixtureTrussAttachment {
                    group_id: gid,
                    fixture_id: fid,
                    truss_id,
                    branch,
                    distance_m,
                    rotation_offset: [0.0; 3],
                });
            self.visualizer_ui_state.editor.push_undo(before);
            self.commit_visualizer_stage();
        }
    }

    fn visualizer_detach(&mut self, sel: &VisualizerSelection) {
        let Some((gid, fid)) = sel.fixture else {
            return;
        };
        let before = self.capture_visualizer_snapshot();
        self.detach_fixture_from_truss(gid, fid);
        self.visualizer_ui_state.editor.push_undo(before);
        self.commit_visualizer_stage();
    }

    /// Copies the selected objects half a metre aside; the copies become
    /// the selection.
    fn visualizer_duplicate(&mut self, sel: &VisualizerSelection) {
        let before = self.capture_visualizer_snapshot();
        self.visualizer_ui_state.editor.selection.clear();
        for &id in &sel.objects {
            if let Some(mut object) = self.visualizer_ui_state.stage.objects.get(&id).cloned() {
                object.transform.translation[0] += 0.5;
                object.transform.translation[2] += 0.5;
                self.add_visualizer_object(object, false);
            }
        }
        self.visualizer_ui_state.editor.push_undo(before);
    }

    fn visualizer_delete(&mut self, sel: VisualizerSelection) {
        let before = self.capture_visualizer_snapshot();
        for id in sel.objects.iter().copied() {
            self.visualizer_ui_state.stage.remove_object(id);
            self.visualizer_ui_state.stage_base.remove_object(id);
        }
        self.visualizer_ui_state.editor.selection.clear();
        self.visualizer_ui_state.editor.push_undo(before);
        self.send_stage_command(HostCommand::RemoveStageObjects(sel.objects), |_, _| {});
    }

    /// Adds `object` to the stage; once the engine has it, it joins the
    /// selection (`alone`: replaces it). The caller records the undo step.
    fn add_visualizer_object(&mut self, object: StageObject, alone: bool) {
        self.send_stage_command(HostCommand::AddStageObject(object), move |app, created| {
            if let Some(CreatedId::StageObject(id)) = created {
                let selection = &mut app.visualizer_ui_state.editor.selection;
                if alone {
                    selection.clear();
                }
                selection.insert(SelectionKey::Object(id));
            }
        });
    }

    /// Model import: the picked file is read and parsed on a worker thread,
    /// then its contents go to the engine (`HostCommand::ImportStageModel`),
    /// which stores it next to the showfile.
    fn visualizer_import_dialog(&mut self, ctx: &Context) {
        #[cfg(not(feature = "file-dialog"))]
        let _ = ctx;
        #[cfg(feature = "file-dialog")]
        if let Some(mut dialog) = self.visualizer_ui_state.import_dialog.take() {
            dialog.update(ctx);
            if let Some(source) = dialog.take_picked() {
                let (sender, receiver) = std::sync::mpsc::channel();
                self.visualizer_ui_state.import_receiver = Some(receiver);
                self.visualizer_ui_state.import_error = None;
                std::thread::spawn(move || {
                    let result = crate::stage_assets::read_glb_for_import(&source)
                        .map_err(|error| format!("{error:#}"));
                    let _ = sender.send(result);
                });
            } else {
                self.visualizer_ui_state.import_dialog = Some(dialog);
            }
        }

        let result = self
            .visualizer_ui_state
            .import_receiver
            .as_ref()
            .and_then(|receiver| receiver.try_recv().ok());
        if let Some(result) = result {
            self.visualizer_ui_state.import_receiver = None;
            match result {
                Ok((file_name, bytes)) => {
                    let before = self.capture_visualizer_snapshot();
                    let command = HostCommand::ImportStageModel {
                        file_name,
                        bytes: ModelBytes(bytes.into()),
                    };
                    let seq = self.send_stage_command(command, move |app, created| {
                        if let Some(CreatedId::StageObject(id)) = created {
                            let editor = &mut app.visualizer_ui_state.editor;
                            editor.selection.clear();
                            editor.selection.insert(SelectionKey::Object(id));
                            editor.push_undo(before);
                        }
                    });
                    self.visualizer_ui_state.import_reply = Some(seq);
                }
                Err(error) => self.visualizer_ui_state.import_error = Some(error),
            }
        }
    }

    fn sync_visualizer_models(&mut self) {
        while let Ok(event) = self.visualizer_ui_state.model_event_receiver.try_recv() {
            match event {
                super::state::ModelWorkerEvent::Started { key, generation }
                    if generation == self.visualizer_ui_state.model_generation =>
                {
                    self.visualizer_ui_state
                        .model_states
                        .insert(key, super::state::ModelLoadState::Preparing);
                }
                super::state::ModelWorkerEvent::Finished {
                    key,
                    generation,
                    result,
                } if generation == self.visualizer_ui_state.model_generation => match result {
                    Ok(prepared) => {
                        let stats = prepared.stats;
                        self.visualizer_ui_state
                            .model_cache
                            .insert(key.clone(), Arc::new(prepared));
                        self.visualizer_ui_state
                            .model_states
                            .insert(key, super::state::ModelLoadState::Ready(stats));
                    }
                    Err(error) => {
                        self.visualizer_ui_state
                            .model_states
                            .insert(key, super::state::ModelLoadState::Failed(error.clone()));
                        self.visualizer_ui_state.import_error = Some(error);
                    }
                },
                _ => {}
            }
        }

        let showfile = self
            .client
            .with_show(|show| show.showfile().map(|path| path.to_path_buf()));
        let Some(showfile) = showfile else {
            return;
        };
        let assets: Vec<_> = self
            .visualizer_ui_state
            .stage
            .assets
            .values()
            .cloned()
            .collect();
        for asset in assets {
            let key = asset.content_hash.clone();
            if self.visualizer_ui_state.model_cache.contains_key(&key)
                || self.visualizer_ui_state.model_states.contains_key(&key)
            {
                continue;
            }
            let path = match blaulicht_engine::stage_assets::resolve_asset(&showfile, &asset) {
                Ok(path) => path,
                Err(error) => {
                    let error = error.to_string();
                    self.visualizer_ui_state
                        .model_states
                        .insert(key, super::state::ModelLoadState::Failed(error.clone()));
                    self.visualizer_ui_state.import_error = Some(error);
                    continue;
                }
            };
            let request = super::state::ModelWorkerRequest {
                key: key.clone(),
                path,
                generation: self.visualizer_ui_state.model_generation,
            };
            match self
                .visualizer_ui_state
                .model_request_sender
                .try_send(request)
            {
                Ok(()) => {
                    self.visualizer_ui_state
                        .model_states
                        .insert(key, super::state::ModelLoadState::Queued);
                }
                Err(crossbeam_channel::TrySendError::Full(_)) => break,
                Err(crossbeam_channel::TrySendError::Disconnected(_)) => {
                    let error = "Visualizer model loader is unavailable".to_string();
                    self.visualizer_ui_state
                        .model_states
                        .insert(key, super::state::ModelLoadState::Failed(error.clone()));
                    self.visualizer_ui_state.import_error = Some(error);
                }
            }
        }
    }

    fn visualizer_selection_list(&mut self, ui: &mut egui::Ui) {
        let shift = ui.input(|input| input.modifiers.shift);
        let mut clicked = None;
        let selection = &self.visualizer_ui_state.editor.selection;
        self.client.with_show(|show| {
            for (gid, group) in &show.0.groups {
                ui.horizontal_wrapped(|ui| {
                    ui.strong(&group.name);
                    for (fid, fixture) in &group.fixtures {
                        let key = SelectionKey::Fixture(*gid, *fid);
                        if ui
                            .selectable_label(selection.contains(&key), &fixture.name)
                            .clicked()
                        {
                            clicked = Some(key);
                        }
                    }
                });
            }
        });
        if !self.visualizer_ui_state.stage.objects.is_empty() {
            ui.horizontal_wrapped(|ui| {
                ui.strong("Objects");
                for (id, object) in &self.visualizer_ui_state.stage.objects {
                    let key = SelectionKey::Object(*id);
                    if ui
                        .selectable_label(selection.contains(&key), &object.name)
                        .clicked()
                    {
                        clicked = Some(key);
                    }
                }
            });
        }

        if let Some(key) = clicked {
            let selection = &mut self.visualizer_ui_state.editor.selection;
            let was_selected = selection.contains(&key);
            if !shift {
                selection.clear();
            }
            if was_selected {
                selection.remove(&key);
            } else {
                selection.insert(key);
            }
        }
    }

    fn visualizer_editor_inspector(&mut self, ui: &mut egui::Ui) {
        // One button per fixture would bury the viewport on large shows, so the
        // list is collapsed by default and scrolls within a fixed height.
        let selected_count = self.visualizer_ui_state.editor.selection.len();
        egui::CollapsingHeader::new(format!("Selection ({selected_count} selected)"))
            .id_salt("visualizer_selection_list")
            .default_open(false)
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(140.0)
                    .auto_shrink([false, true])
                    .show(ui, |ui| self.visualizer_selection_list(ui));
            });

        let only = if self.visualizer_ui_state.editor.selection.len() == 1 {
            self.visualizer_ui_state
                .editor
                .selection
                .iter()
                .next()
                .copied()
        } else {
            None
        };
        match only {
            Some(SelectionKey::Object(id)) => self.visualizer_object_inspector(ui, id),
            Some(SelectionKey::Fixture(gid, fid)) => {
                self.visualizer_fixture_inspector(ui, gid, fid)
            }
            // Keep the fixture inspector's row height so selecting a fixture
            // does not shift the viewport out from under the pointer.
            None => {
                ui.horizontal(|ui| {
                    ui.set_min_height(ui.spacing().interact_size.y);
                    ui.weak(match self.visualizer_ui_state.editor.selection.len() {
                        0 => "Nothing selected".to_string(),
                        n => format!("{n} items selected"),
                    });
                });
            }
        }
    }

    fn visualizer_object_inspector(&mut self, ui: &mut egui::Ui, id: u64) {
        let Some(mut object) = self.visualizer_ui_state.stage.objects.get(&id).cloned() else {
            self.visualizer_ui_state
                .editor
                .selection
                .remove(&SelectionKey::Object(id));
            return;
        };
        let before_object = object.clone();
        let connected = self.visualizer_ui_state.stage.is_truss_connected(id);
        if dropdown::is_desktop(ui) {
            desktop_object_form(ui, id, &mut object, connected);
        } else {
            kiosk_object_row(ui, id, &mut object, connected);
        }
        if matches!(object.kind, StageObjectKind::ProceduralTruss(_)) {
            ui.horizontal(|ui| {
                if ui.button("Select Connected").clicked() {
                    for connected_id in self.visualizer_ui_state.stage.connected_trusses(id) {
                        self.visualizer_ui_state
                            .editor
                            .selection
                            .insert(SelectionKey::Object(connected_id));
                    }
                }
                if ui
                    .add_enabled(connected, egui::Button::new("Disconnect"))
                    .clicked()
                {
                    let before = self.capture_visualizer_snapshot();
                    self.visualizer_ui_state.stage.disconnect_truss(id);
                    self.visualizer_ui_state.editor.push_undo(before);
                    self.commit_visualizer_stage();
                }
            });
        }
        object.transform.sanitize();
        if object != before_object {
            let before = self.capture_visualizer_snapshot();
            let connected_ids = self.visualizer_ui_state.stage.connected_trusses(id);
            let translation_delta = [
                object.transform.translation[0] - before_object.transform.translation[0],
                object.transform.translation[1] - before_object.transform.translation[1],
                object.transform.translation[2] - before_object.transform.translation[2],
            ];
            let rotation_delta = [
                object.transform.rotation[0] - before_object.transform.rotation[0],
                object.transform.rotation[1] - before_object.transform.rotation[1],
                object.transform.rotation[2] - before_object.transform.rotation[2],
            ];
            self.visualizer_ui_state.stage.objects.insert(id, object);
            if connected_ids.len() > 1 {
                let pivot = Vec3::new(
                    before_object.transform.translation[0],
                    before_object.transform.translation[1],
                    before_object.transform.translation[2],
                );
                let rotation_matrix = mat4_rotation_euler(Vec3::new(
                    rotation_delta[0].to_radians(),
                    rotation_delta[1].to_radians(),
                    rotation_delta[2].to_radians(),
                ));
                for connected_id in connected_ids {
                    if connected_id == id {
                        continue;
                    }
                    if let Some(connected_object) = self
                        .visualizer_ui_state
                        .stage
                        .objects
                        .get_mut(&connected_id)
                    {
                        let offset = Vec3::new(
                            connected_object.transform.translation[0] - pivot.x,
                            connected_object.transform.translation[1] - pivot.y,
                            connected_object.transform.translation[2] - pivot.z,
                        );
                        let rotated = mat4_transform_dir(rotation_matrix, offset);
                        connected_object.transform.translation = [
                            pivot.x + rotated.x + translation_delta[0],
                            pivot.y + rotated.y + translation_delta[1],
                            pivot.z + rotated.z + translation_delta[2],
                        ];
                        for (rotation, delta) in connected_object
                            .transform
                            .rotation
                            .iter_mut()
                            .zip(rotation_delta)
                        {
                            *rotation += delta;
                        }
                    }
                }
            }
            self.visualizer_ui_state.stage.sanitize();
            self.visualizer_ui_state.editor.push_undo(before);
            self.commit_visualizer_stage();
        }
    }

    fn visualizer_fixture_inspector(&mut self, ui: &mut egui::Ui, gid: u8, fid: u8) {
        let Some((name, (old_pos, old_rotation))) = self.visualizer_fixture((gid, fid)) else {
            self.visualizer_ui_state
                .editor
                .selection
                .remove(&SelectionKey::Fixture(gid, fid));
            return;
        };
        let mut pos = old_pos;
        let mut rotation = old_rotation;
        ui.horizontal_wrapped(|ui| {
            ui.label(&name);
            for (axis, label) in ["X", "Y", "Z"].into_iter().enumerate() {
                ui.label(label);
                ui.add(
                    egui::DragValue::new(&mut pos[axis])
                        .speed(0.05)
                        .suffix(" m"),
                );
            }
            for (axis, label) in ["RX", "RY", "RZ"].into_iter().enumerate() {
                ui.label(label);
                ui.add(
                    egui::DragValue::new(&mut rotation[axis])
                        .speed(0.5)
                        .suffix(" deg"),
                );
            }
        });
        if pos != old_pos || rotation != old_rotation {
            let before = self.capture_visualizer_snapshot();
            self.draft_fixture_poses(&[(gid, fid)], |pose| {
                for axis in 0..3 {
                    pose.0[axis] = finite_or(pos[axis], old_pos[axis]);
                    pose.1[axis] = finite_or(rotation[axis], old_rotation[axis]);
                }
            });
            self.visualizer_ui_state.editor.push_undo(before);
            self.commit_visualizer_stage();
        }
    }

    fn visualizer_top_ui(&mut self, ui: &mut egui::Ui) {
        let size = Vec2::new(ui.available_width(), ui.available_height().max(160.0));
        // The desktop also shows the top view in View mode, read-only.
        let editable = self.visualizer_ui_state.editor.mode == VisualizerMode::Edit;
        let (rect, response) = ui.allocate_exact_size(size, Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, Color32::from_rgb(13, 14, 20));

        if response.hovered() {
            let scroll = ui.input(|input| input.smooth_scroll_delta.y);
            if scroll.abs() > 0.0 {
                self.visualizer_ui_state.editor.top_zoom =
                    (self.visualizer_ui_state.editor.top_zoom * (1.0 + scroll * 0.0015))
                        .clamp(0.2, 8.0);
            }
        }

        let room = &self.visualizer_ui_state.stage.room;
        let pixels_per_meter = ((rect.width() / room.width.max(2.0))
            .min(rect.height() / room.depth.max(2.0))
            * 0.9
            * self.visualizer_ui_state.editor.top_zoom)
            .max(1.0);
        let center = self.visualizer_ui_state.editor.top_center;
        let to_screen = |x: f32, z: f32| {
            Pos2::new(
                rect.center().x + (x - center[0]) * pixels_per_meter,
                rect.center().y + (z - center[1]) * pixels_per_meter,
            )
        };
        let to_world = |pos: Pos2| {
            [
                center[0] + (pos.x - rect.center().x) / pixels_per_meter,
                center[1] + (pos.y - rect.center().y) / pixels_per_meter,
            ]
        };

        if response.drag_started_by(egui::PointerButton::Secondary) {
            self.visualizer_ui_state.editor.drag_last_pointer = response.interact_pointer_pos();
        }
        if response.dragged_by(egui::PointerButton::Secondary) {
            if let (Some(pointer), Some(last)) = (
                response.interact_pointer_pos(),
                self.visualizer_ui_state.editor.drag_last_pointer,
            ) {
                let delta = pointer - last;
                self.visualizer_ui_state.editor.top_center[0] -= delta.x / pixels_per_meter;
                self.visualizer_ui_state.editor.top_center[1] -= delta.y / pixels_per_meter;
                self.visualizer_ui_state.editor.drag_last_pointer = Some(pointer);
            }
        }
        if response.drag_stopped_by(egui::PointerButton::Secondary) {
            self.visualizer_ui_state.editor.drag_last_pointer = None;
        }

        let room_rect = Rect::from_center_size(
            to_screen(0.0, 0.0),
            Vec2::new(room.width * pixels_per_meter, room.depth * pixels_per_meter),
        );
        painter.rect_filled(room_rect, 0.0, Color32::from_rgb(25, 27, 34));
        painter.rect_stroke(
            room_rect,
            0.0,
            Stroke::new(1.0_f32, Color32::from_gray(105)),
            egui::StrokeKind::Inside,
        );
        let grid_step = if pixels_per_meter > 35.0 { 1.0 } else { 2.0 };
        let half_w = room.width * 0.5;
        let half_d = room.depth * 0.5;
        let mut x = (-half_w / grid_step).ceil() * grid_step;
        while x <= half_w {
            painter.line_segment(
                [to_screen(x, -half_d), to_screen(x, half_d)],
                Stroke::new(0.5_f32, Color32::from_gray(55)),
            );
            x += grid_step;
        }
        let mut z = (-half_d / grid_step).ceil() * grid_step;
        while z <= half_d {
            painter.line_segment(
                [to_screen(-half_w, z), to_screen(half_w, z)],
                Stroke::new(0.5_f32, Color32::from_gray(55)),
            );
            z += grid_step;
        }

        let state = &self.visualizer_ui_state;
        let fixtures: Vec<((u8, u8), String, [f32; 2])> = self.client.with_show(|show| {
            show.0
                .groups
                .iter()
                .flat_map(|(gid, group)| {
                    group.fixtures.iter().map(move |(fid, fixture)| {
                        let key = (*gid, *fid);
                        let (position, _) =
                            fixture_pose(&state.stage, &state.fixture_drafts, key, fixture);
                        (key, fixture.name.clone(), [position[0], position[2]])
                    })
                })
                .collect()
        });

        for (id, object) in &self.visualizer_ui_state.stage.objects {
            if !object.visible {
                continue;
            }
            let footprint = object_footprint(object);
            let object_rect = Rect::from_center_size(
                to_screen(
                    object.transform.translation[0],
                    object.transform.translation[2],
                ),
                Vec2::new(
                    footprint[0] * pixels_per_meter,
                    footprint[1] * pixels_per_meter,
                ),
            );
            let selected = self
                .visualizer_ui_state
                .editor
                .selection
                .contains(&SelectionKey::Object(*id));
            let object_color = Color32::from_rgb(object.color[0], object.color[1], object.color[2]);
            if let StageObjectKind::ProceduralTruss(spec) = object.kind {
                if spec.part == TrussPart::BasePlate {
                    painter.rect_filled(object_rect, 1.0, object_color);
                    painter.rect_stroke(
                        object_rect,
                        1.0,
                        Stroke::new(
                            if selected { 2.0_f32 } else { 1.0_f32 },
                            if selected {
                                Color32::LIGHT_BLUE
                            } else {
                                Color32::from_gray(145)
                            },
                        ),
                        egui::StrokeKind::Inside,
                    );
                } else {
                    let outer_width = (blaulicht_engine::stage::TRUSS_PROFILE_SIZE_M
                        * pixels_per_meter)
                        .clamp(5.0, 18.0);
                    for branch in super::truss::world_branches(spec, &object.transform) {
                        let points = [
                            to_screen(branch.start.x, branch.start.z),
                            to_screen(branch.end.x, branch.end.z),
                        ];
                        painter.line_segment(
                            points,
                            Stroke::new(
                                outer_width + if selected { 3.0 } else { 1.0 },
                                if selected {
                                    Color32::LIGHT_BLUE
                                } else {
                                    Color32::from_gray(145)
                                },
                            ),
                        );
                        painter.line_segment(points, Stroke::new(outer_width, object_color));
                        painter.line_segment(points, Stroke::new(1.0_f32, Color32::from_gray(45)));
                    }
                }
            } else {
                painter.rect_filled(object_rect, 1.0, object_color);
                painter.rect_stroke(
                    object_rect,
                    1.0,
                    Stroke::new(
                        if selected { 2.0_f32 } else { 1.0_f32 },
                        if selected {
                            Color32::LIGHT_BLUE
                        } else {
                            Color32::from_gray(145)
                        },
                    ),
                    egui::StrokeKind::Inside,
                );
            }
            if selected {
                if let StageObjectKind::ProceduralTruss(spec) = object.kind {
                    for endpoint in super::truss::world_endpoints(spec, &object.transform) {
                        let center = to_screen(endpoint.position.x, endpoint.position.z);
                        painter.circle_filled(center, 4.0, Color32::LIGHT_GREEN);
                        painter.circle_stroke(center, 6.0, Stroke::new(1.0_f32, Color32::BLACK));
                    }
                }
            }
        }
        for (id, _name, position) in &fixtures {
            let center = to_screen(position[0], position[1]);
            let selected = self
                .visualizer_ui_state
                .editor
                .selection
                .contains(&SelectionKey::Fixture(id.0, id.1));
            painter.circle_filled(center, 7.0, Color32::from_rgb(240, 105, 80));
            painter.circle_stroke(
                center,
                8.0,
                Stroke::new(
                    if selected { 3.0_f32 } else { 1.0_f32 },
                    if selected {
                        Color32::LIGHT_BLUE
                    } else {
                        Color32::BLACK
                    },
                ),
            );
        }

        if let (Some(start), Some(pointer)) = (
            self.visualizer_ui_state.editor.box_select_start,
            response.interact_pointer_pos(),
        ) {
            let selection_rect = Rect::from_two_pos(start, pointer);
            painter.rect_filled(
                selection_rect,
                0.0,
                Color32::from_rgba_unmultiplied(90, 170, 240, 24),
            );
            painter.rect_stroke(
                selection_rect,
                0.0,
                Stroke::new(1.0_f32, Color32::LIGHT_BLUE),
                egui::StrokeKind::Inside,
            );
        }

        if editable && response.drag_started_by(egui::PointerButton::Primary) {
            if let Some(pointer) = response.interact_pointer_pos() {
                let hit = hit_test_top(
                    pointer,
                    pixels_per_meter,
                    &fixtures,
                    &self.visualizer_ui_state.stage,
                    &to_screen,
                );
                let shift = ui.input(|input| input.modifiers.shift);
                if let Some(hit) = hit {
                    let was_selected = self.visualizer_ui_state.editor.selection.contains(&hit);
                    if !shift && !was_selected {
                        self.visualizer_ui_state.editor.selection.clear();
                    }
                    self.visualizer_ui_state.editor.selection.insert(hit);
                    // In Select mode, dragging an already-selected item moves
                    // the selection; other drags still box-select.
                    if self.visualizer_ui_state.editor.tool != EditorTool::Select || was_selected {
                        self.visualizer_ui_state.editor.drag_before =
                            Some(self.capture_visualizer_snapshot());
                        self.visualizer_ui_state.editor.drag_last_world = Some(to_world(pointer));
                        self.visualizer_ui_state.editor.drag_last_pointer = Some(pointer);
                    }
                } else {
                    if !shift {
                        self.visualizer_ui_state.editor.selection.clear();
                    }
                    if self.visualizer_ui_state.editor.tool == EditorTool::Select {
                        self.visualizer_ui_state.editor.box_select_start = Some(pointer);
                        self.visualizer_ui_state.editor.box_select_additive = shift;
                    }
                }
            }
        }
        if editable && response.clicked_by(egui::PointerButton::Primary) {
            if let Some(pointer) = response.interact_pointer_pos() {
                let hit = hit_test_top(
                    pointer,
                    pixels_per_meter,
                    &fixtures,
                    &self.visualizer_ui_state.stage,
                    &to_screen,
                );
                let shift = ui.input(|input| input.modifiers.shift);
                match hit {
                    Some(hit) => {
                        if !shift {
                            self.visualizer_ui_state.editor.selection.clear();
                        }
                        self.visualizer_ui_state.editor.selection.insert(hit);
                    }
                    None if !shift => self.visualizer_ui_state.editor.selection.clear(),
                    None => {}
                }
            }
        }
        if editable && response.dragged_by(egui::PointerButton::Primary) {
            if let Some(pointer) = response.interact_pointer_pos() {
                match self.visualizer_ui_state.editor.tool {
                    // Select only has `drag_last_world` set when the drag
                    // started on an already-selected item.
                    EditorTool::Translate | EditorTool::Select => {
                        if let Some(last) = self.visualizer_ui_state.editor.drag_last_world {
                            let world = to_world(pointer);
                            self.move_visualizer_selection(world[0] - last[0], world[1] - last[1]);
                            self.visualizer_ui_state.editor.drag_last_world = Some(world);
                        }
                    }
                    EditorTool::Rotate | EditorTool::Scale => {
                        if let Some(last) = self.visualizer_ui_state.editor.drag_last_pointer {
                            self.transform_visualizer_selection(
                                self.visualizer_ui_state.editor.tool,
                                pointer - last,
                                1.0 / pixels_per_meter,
                            );
                            self.visualizer_ui_state.editor.drag_last_pointer = Some(pointer);
                        }
                    }
                }
            }
        }
        if editable && response.drag_stopped_by(egui::PointerButton::Primary) {
            if let (Some(start), Some(pointer)) = (
                self.visualizer_ui_state.editor.box_select_start.take(),
                response.interact_pointer_pos(),
            ) {
                let selection_rect = Rect::from_two_pos(start, pointer);
                if !self.visualizer_ui_state.editor.box_select_additive {
                    self.visualizer_ui_state.editor.selection.clear();
                }
                for (id, object) in &self.visualizer_ui_state.stage.objects {
                    if object.visible
                        && !object.locked
                        && selection_rect.contains(to_screen(
                            object.transform.translation[0],
                            object.transform.translation[2],
                        ))
                    {
                        self.visualizer_ui_state
                            .editor
                            .selection
                            .insert(SelectionKey::Object(*id));
                    }
                }
                for (id, _, position) in &fixtures {
                    if selection_rect.contains(to_screen(position[0], position[1])) {
                        self.visualizer_ui_state
                            .editor
                            .selection
                            .insert(SelectionKey::Fixture(id.0, id.1));
                    }
                }
            }
            let transformed = self
                .visualizer_ui_state
                .editor
                .drag_last_pointer
                .take()
                .is_some();
            self.visualizer_ui_state.editor.drag_last_world = None;
            if transformed {
                self.snap_visualizer_selection();
                if let Some(before) = self.visualizer_ui_state.editor.drag_before.take() {
                    self.visualizer_ui_state.editor.push_undo(before);
                }
                self.commit_visualizer_stage();
            }
        }
    }

    fn transform_visualizer_selection(
        &mut self,
        tool: EditorTool,
        delta: Vec2,
        meters_per_pixel: f32,
    ) {
        match tool {
            EditorTool::Select => {}
            EditorTool::Translate => {
                let yaw = self.visualizer_ui_state.camera_yaw;
                let right = delta.x * meters_per_pixel;
                let forward = delta.y * meters_per_pixel;
                self.move_visualizer_selection(
                    right * yaw.cos() - forward * yaw.sin(),
                    right * yaw.sin() + forward * yaw.cos(),
                );
            }
            EditorTool::Rotate => {
                let degrees = delta.x * 0.5;
                let selection: Vec<_> = self
                    .visualizer_ui_state
                    .editor
                    .selection
                    .iter()
                    .copied()
                    .collect();
                let fixtures: Vec<_> = selection
                    .iter()
                    .filter_map(|key| match key {
                        SelectionKey::Fixture(gid, fid) => Some((*gid, *fid)),
                        SelectionKey::Object(_) => None,
                    })
                    .collect();
                self.draft_fixture_poses(&fixtures, |pose| pose.1[1] += degrees);
                let objects = self.expanded_selected_objects();
                self.rotate_stage_objects(&objects, degrees);
            }
            EditorTool::Scale => {
                let factor = (delta.x * 0.01).exp().clamp(0.1, 10.0);
                let selected: Vec<u64> = self
                    .visualizer_ui_state
                    .editor
                    .selection
                    .iter()
                    .filter_map(|key| match key {
                        SelectionKey::Object(id) => Some(*id),
                        SelectionKey::Fixture(_, _) => None,
                    })
                    .collect();
                for id in selected {
                    if let Some(object) = self.visualizer_ui_state.stage.objects.get_mut(&id) {
                        if !object.locked
                            && !matches!(object.kind, StageObjectKind::ProceduralTruss(_))
                        {
                            for value in &mut object.transform.scale {
                                *value = (*value * factor).clamp(0.01, 1_000.0);
                            }
                        }
                    }
                }
            }
        }
    }

    fn move_visualizer_selection(&mut self, dx: f32, dz: f32) {
        let selection: Vec<SelectionKey> = self
            .visualizer_ui_state
            .editor
            .selection
            .iter()
            .copied()
            .collect();
        let fixtures: Vec<_> = selection
            .iter()
            .filter_map(|key| match key {
                SelectionKey::Fixture(gid, fid) => Some((*gid, *fid)),
                SelectionKey::Object(_) => None,
            })
            .collect();
        self.draft_fixture_poses(&fixtures, |pose| {
            pose.0[0] += dx;
            pose.0[2] += dz;
        });
        for id in self.expanded_selected_objects() {
            if let Some(object) = self.visualizer_ui_state.stage.objects.get_mut(&id) {
                if !object.locked {
                    object.transform.translation[0] += dx;
                    object.transform.translation[2] += dz;
                }
            }
        }
    }

    fn expanded_selected_objects(&self) -> std::collections::BTreeSet<u64> {
        let mut result = std::collections::BTreeSet::new();
        for id in self
            .visualizer_ui_state
            .editor
            .selection
            .iter()
            .filter_map(|key| match key {
                SelectionKey::Object(id) => Some(*id),
                SelectionKey::Fixture(_, _) => None,
            })
        {
            if self.visualizer_ui_state.stage.truss_spec(id).is_some() {
                result.extend(self.visualizer_ui_state.stage.connected_trusses(id));
            } else {
                result.insert(id);
            }
        }
        result
    }

    fn rotate_stage_objects(&mut self, ids: &std::collections::BTreeSet<u64>, degrees: f32) {
        if ids.is_empty() {
            return;
        }
        let (sum_x, sum_z, count) = ids.iter().fold((0.0, 0.0, 0_u32), |acc, id| {
            let Some(object) = self.visualizer_ui_state.stage.objects.get(id) else {
                return acc;
            };
            (
                acc.0 + object.transform.translation[0],
                acc.1 + object.transform.translation[2],
                acc.2 + 1,
            )
        });
        if count == 0 {
            return;
        }
        let pivot = [sum_x / count as f32, sum_z / count as f32];
        let radians = degrees.to_radians();
        let (sin, cos) = radians.sin_cos();
        for id in ids {
            if let Some(object) = self.visualizer_ui_state.stage.objects.get_mut(id) {
                if object.locked {
                    continue;
                }
                let x = object.transform.translation[0] - pivot[0];
                let z = object.transform.translation[2] - pivot[1];
                object.transform.translation[0] = pivot[0] + x * cos + z * sin;
                object.transform.translation[2] = pivot[1] - x * sin + z * cos;
                object.transform.rotation[1] += degrees;
            }
        }
    }

    fn snap_visualizer_selection(&mut self) {
        if !self.visualizer_ui_state.editor.snap_enabled {
            return;
        }
        let position_step = self.visualizer_ui_state.editor.position_snap.max(0.01);
        let rotation_step = self.visualizer_ui_state.editor.rotation_snap.max(0.1);
        let scale_step = self.visualizer_ui_state.editor.scale_snap.max(0.01);
        let tool = self.visualizer_ui_state.editor.tool;
        let selection: Vec<SelectionKey> = self
            .visualizer_ui_state
            .editor
            .selection
            .iter()
            .copied()
            .collect();
        let connected_trusses: std::collections::BTreeSet<_> = selection
            .iter()
            .filter_map(|key| match key {
                SelectionKey::Object(id)
                    if self.visualizer_ui_state.stage.is_truss_connected(*id) =>
                {
                    Some(*id)
                }
                _ => None,
            })
            .collect();
        let fixtures: Vec<(u8, u8)> = selection
            .iter()
            .filter_map(|key| match key {
                SelectionKey::Fixture(gid, fid) => Some((*gid, *fid)),
                SelectionKey::Object(_) => None,
            })
            .collect();
        match tool {
            // A Select drag can only move the selection.
            EditorTool::Translate | EditorTool::Select => {
                self.draft_fixture_poses(&fixtures, |pose| {
                    pose.0[0] = snap(pose.0[0], position_step);
                    pose.0[2] = snap(pose.0[2], position_step);
                });
            }
            EditorTool::Rotate => {
                self.draft_fixture_poses(&fixtures, |pose| {
                    pose.1[1] = snap(pose.1[1], rotation_step);
                });
            }
            EditorTool::Scale => {}
        }
        for key in selection {
            let SelectionKey::Object(id) = key else {
                continue;
            };
            if let Some(object) = self.visualizer_ui_state.stage.objects.get_mut(&id) {
                if connected_trusses.contains(&id) {
                    continue;
                }
                match tool {
                    EditorTool::Translate | EditorTool::Select => {
                        object.transform.translation[0] =
                            snap(object.transform.translation[0], position_step);
                        object.transform.translation[2] =
                            snap(object.transform.translation[2], position_step);
                    }
                    EditorTool::Rotate => {
                        object.transform.rotation[1] =
                            snap(object.transform.rotation[1], rotation_step);
                    }
                    EditorTool::Scale => {
                        if !matches!(object.kind, StageObjectKind::ProceduralTruss(_)) {
                            for value in &mut object.transform.scale {
                                *value = snap(*value, scale_step).max(0.01);
                            }
                        }
                    }
                }
            }
        }
        if matches!(tool, EditorTool::Translate | EditorTool::Rotate) {
            self.snap_truss_endpoints();
        }
    }

    fn snap_truss_endpoints(&mut self) {
        const SNAP_DISTANCE_M: f32 = 0.2;
        const SNAP_ANGLE_DEG: f32 = 25.0;
        let moving = self.expanded_selected_objects();
        if moving.is_empty()
            || moving
                .iter()
                .any(|id| self.visualizer_ui_state.stage.truss_spec(*id).is_none())
        {
            return;
        }
        let occupied: std::collections::BTreeSet<_> = self
            .visualizer_ui_state
            .stage
            .truss_connections
            .iter()
            .flat_map(|connection| [connection.a, connection.b])
            .collect();
        let mut best: Option<(f32, TrussEndpointRef, TrussEndpointRef, Vec3, Vec3, f32)> = None;
        for moving_id in &moving {
            let Some(moving_object) = self.visualizer_ui_state.stage.objects.get(moving_id) else {
                continue;
            };
            let Some(moving_spec) = self.visualizer_ui_state.stage.truss_spec(*moving_id) else {
                continue;
            };
            for (moving_endpoint, moving_world) in
                super::truss::world_endpoints(moving_spec, &moving_object.transform)
                    .into_iter()
                    .enumerate()
            {
                let moving_ref = TrussEndpointRef {
                    object_id: *moving_id,
                    endpoint: moving_endpoint as u8,
                };
                if occupied.contains(&moving_ref) {
                    continue;
                }
                for (fixed_id, fixed_object) in &self.visualizer_ui_state.stage.objects {
                    if moving.contains(fixed_id) {
                        continue;
                    }
                    let Some(fixed_spec) = self.visualizer_ui_state.stage.truss_spec(*fixed_id)
                    else {
                        continue;
                    };
                    if fixed_spec.profile != moving_spec.profile {
                        continue;
                    }
                    for (fixed_endpoint, fixed_world) in
                        super::truss::world_endpoints(fixed_spec, &fixed_object.transform)
                            .into_iter()
                            .enumerate()
                    {
                        let fixed_ref = TrussEndpointRef {
                            object_id: *fixed_id,
                            endpoint: fixed_endpoint as u8,
                        };
                        if occupied.contains(&fixed_ref) {
                            continue;
                        }
                        let distance = moving_world.position.sub(fixed_world.position).norm();
                        if distance > SNAP_DISTANCE_M {
                            continue;
                        }
                        let moving_angle = moving_world.direction.z.atan2(moving_world.direction.x);
                        let target_direction = fixed_world.direction.scale(-1.0);
                        if (moving_world.direction.y - target_direction.y).abs() > 0.1 {
                            continue;
                        }
                        let moving_horizontal =
                            moving_world.direction.x.hypot(moving_world.direction.z);
                        let target_horizontal = target_direction.x.hypot(target_direction.z);
                        if (moving_horizontal < 0.01) != (target_horizontal < 0.01) {
                            continue;
                        }
                        let target_angle = target_direction.z.atan2(target_direction.x);
                        let yaw_delta = if moving_horizontal < 0.01 {
                            0.0
                        } else {
                            normalize_angle(moving_angle - target_angle)
                        };
                        if yaw_delta.to_degrees().abs() > SNAP_ANGLE_DEG {
                            continue;
                        }
                        if best.as_ref().is_none_or(|current| distance < current.0) {
                            best = Some((
                                distance,
                                moving_ref,
                                fixed_ref,
                                moving_world.position,
                                fixed_world.position,
                                yaw_delta,
                            ));
                        }
                    }
                }
            }
        }
        let Some((_, moving_ref, fixed_ref, pivot, target, yaw_delta)) = best else {
            return;
        };
        let (sin, cos) = yaw_delta.sin_cos();
        for id in &moving {
            if let Some(object) = self.visualizer_ui_state.stage.objects.get_mut(id) {
                let x = object.transform.translation[0] - pivot.x;
                let z = object.transform.translation[2] - pivot.z;
                object.transform.translation[0] = pivot.x + x * cos + z * sin;
                object.transform.translation[2] = pivot.z - x * sin + z * cos;
                object.transform.rotation[1] += yaw_delta.to_degrees();
                object.transform.translation[0] += target.x - pivot.x;
                object.transform.translation[1] += target.y - pivot.y;
                object.transform.translation[2] += target.z - pivot.z;
            }
        }
        self.visualizer_ui_state.stage.truss_connections.push(
            TrussConnection {
                a: moving_ref,
                b: fixed_ref,
            }
            .normalized(),
        );
        self.visualizer_ui_state.stage.sanitize();
    }
}

/// Width of the desktop Selection and Stage column.
const DESKTOP_SIDE_W: f32 = 280.0;

impl BlaulichtApp {
    /// Desktop: a 40px page bar with the mode, tools and view toggles, the
    /// stage view on the left and Selection and Stage panes on the right.
    fn visualizer_desktop(
        &mut self,
        ui: &mut egui::Ui,
        screen_id: ScreenId,
        render_context: PageRenderContext,
    ) {
        use crate::app::components::studio;
        use crate::app::pages::setup::duo;
        use crate::app::pages::system::desk;
        use crate::app::ui_tokens::{studio as tokens, PAGE_BAR_HEIGHT, TARGET_MIN, TARGET_SHOW};
        use egui::{pos2, vec2};

        let t = tokens(ui.ctx());
        let full = desk::page_rect(ui);
        let gap = 2.0;
        let bar_rect = Rect::from_min_size(full.min, vec2(full.width(), PAGE_BAR_HEIGHT));
        let side = Rect::from_min_max(
            pos2(full.right() - DESKTOP_SIDE_W, bar_rect.bottom() + gap),
            full.max,
        );
        let view_rect = Rect::from_min_max(
            pos2(full.left(), bar_rect.bottom() + gap),
            pos2(side.left() - gap, full.bottom()),
        );

        // Keyboard undo / redo, as in the kiosk's edit toolbar.
        let editing = self.visualizer_ui_state.editor.mode == VisualizerMode::Edit;
        if editing {
            let (undo, redo) = ui.input(|input| {
                let cmd = input.modifiers.command;
                (
                    cmd && input.key_pressed(egui::Key::Z) && !input.modifiers.shift,
                    cmd && (input.key_pressed(egui::Key::Y)
                        || (input.modifiers.shift && input.key_pressed(egui::Key::Z))),
                )
            });
            if undo {
                self.visualizer_undo();
            }
            if redo {
                self.visualizer_redo();
            }
        }

        self.visualizer_ui_state.settings.show_room = self.visualizer_ui_state.stage.room.visible;
        self.visualizer_ui_state.settings.room_width = self.visualizer_ui_state.stage.room.width;
        self.visualizer_ui_state.settings.room_depth = self.visualizer_ui_state.stage.room.depth;
        self.visualizer_ui_state.settings.room_height = self.visualizer_ui_state.stage.room.height;

        self.sync_visualizer_models();
        let universes: Vec<Option<[u8; 513]>> = (0..NUM_DMX_UNIVERSES)
            .map(|universe| self.client.with_dmx_universe(universe, |buffer| *buffer))
            .collect();
        let snapshot = self.client.with_show(|show| {
            collect_fixtures(
                show,
                &universes,
                &self.visualizer_ui_state.stage,
                &self.visualizer_ui_state.fixture_drafts,
                &self.visualizer_ui_state.model_cache,
            )
        });

        // The page bar.
        let mut room_changed = false;
        let mut fit = false;
        let mut reset = false;
        let top_view = self.visualizer_ui_state.editor.view == EditorView::Top;
        let mut toggle_top = false;
        let mut undo = false;
        let mut redo = false;
        let mut bar = desk::pane_at(ui, bar_rect, t.pane, "visualizer-bar");
        studio::page_bar(
            &mut bar,
            |ui| {
                // `segmented` lays out in a nested row; give it the bar
                // buttons' height so it centres like them.
                ui.spacing_mut().interact_size.y = TARGET_SHOW;
                let editor = &mut self.visualizer_ui_state.editor;
                let mut mode = usize::from(editor.mode == VisualizerMode::Edit);
                if studio::segmented(ui, &mut mode, &["View", "Edit"], TARGET_SHOW).changed()
                    || mode != usize::from(editor.mode == VisualizerMode::Edit)
                {
                    editor.mode = if mode == 1 {
                        VisualizerMode::Edit
                    } else {
                        VisualizerMode::View
                    };
                }
                if editor.mode == VisualizerMode::Edit {
                    divider(ui, t.line);
                    const TOOLS: [EditorTool; 4] = [
                        EditorTool::Select,
                        EditorTool::Translate,
                        EditorTool::Rotate,
                        EditorTool::Scale,
                    ];
                    let mut tool = TOOLS.iter().position(|t| *t == editor.tool).unwrap_or(0);
                    studio::segmented(
                        ui,
                        &mut tool,
                        &["Select", "Move", "Rotate", "Scale"],
                        TARGET_SHOW,
                    );
                    editor.tool = TOOLS[tool];
                    if studio::mini_button(ui, "Snap", editor.snap_enabled, TARGET_SHOW)
                        .on_hover_text("Snap moves and rotations to the steps in the Stage pane")
                        .clicked()
                    {
                        editor.snap_enabled = !editor.snap_enabled;
                    }
                    let can_undo = !editor.undo.is_empty();
                    let can_redo = !editor.redo.is_empty();
                    let add = studio::mini_button(ui, "Add", false, TARGET_SHOW);
                    egui::Popup::menu(&add).show(|ui| self.visualizer_add_menu(ui));
                    undo = ui
                        .add_enabled_ui(can_undo, |ui| {
                            studio::mini_button(ui, "Undo", false, TARGET_SHOW)
                        })
                        .inner
                        .clicked();
                    redo = ui
                        .add_enabled_ui(can_redo, |ui| {
                            studio::mini_button(ui, "Redo", false, TARGET_SHOW)
                        })
                        .inner
                        .clicked();
                }
                divider(ui, t.line);
                let settings = &mut self.visualizer_ui_state.settings;
                for (label, on) in [
                    ("Room", &mut settings.show_room),
                    ("Grid", &mut settings.show_grid),
                    ("Axes", &mut settings.show_axes),
                    ("Beams", &mut settings.show_beams),
                    ("Bodies", &mut settings.show_bodies),
                    ("Labels", &mut settings.show_labels),
                ] {
                    if studio::mini_button(ui, label, *on, TARGET_SHOW).clicked() {
                        *on = !*on;
                        if label == "Room" {
                            room_changed = true;
                        }
                    }
                }
            },
            |ui| {
                // As in the mockup: a Top toggle beside the camera buttons,
                // reachable in View and Edit mode.
                if studio::mini_button(ui, "Top", top_view, TARGET_SHOW)
                    .on_hover_text("Switch between the 3D view and the top view")
                    .clicked()
                {
                    toggle_top = true;
                }
                fit = studio::mini_button(ui, "Fit view", false, TARGET_SHOW).clicked();
                reset = studio::mini_button(ui, "Reset cam", false, TARGET_SHOW).clicked();
            },
        );
        if undo {
            self.visualizer_undo();
        }
        if redo {
            self.visualizer_redo();
        }
        if room_changed {
            self.visualizer_ui_state.stage.room.visible =
                self.visualizer_ui_state.settings.show_room;
        }
        if toggle_top {
            self.visualizer_ui_state.editor.view = if top_view {
                EditorView::Perspective
            } else {
                EditorView::Top
            };
        }
        if reset {
            reset_camera(&mut self.visualizer_ui_state);
        }
        if fit {
            self.visualizer_ui_state.camera_yaw = DEFAULT_CAMERA_YAW;
            self.visualizer_ui_state.camera_pitch = DEFAULT_CAMERA_PITCH;
            self.visualizer_ui_state.camera_radius =
                (snapshot.bounds.radius * 1.5).clamp(3.0, 60.0);
            self.visualizer_ui_state.free_camera = false;
            self.visualizer_ui_state.camera_position = Vec3::new(0.0, 0.0, 0.0);
        }

        // The stage view.
        let editing = self.visualizer_ui_state.editor.mode == VisualizerMode::Edit;
        let top = self.visualizer_ui_state.editor.view == EditorView::Top;
        let mut view = desk::pane_at(ui, view_rect, t.pane, "visualizer-view");
        studio::pane_header(
            &mut view,
            None,
            if top {
                "Stage, top view"
            } else {
                "Stage, 3D view"
            },
            false,
            |_| {},
        );
        let body = view.available_rect_before_wrap().shrink(8.0);
        let mut canvas = view.new_child(
            egui::UiBuilder::new()
                .id_salt("visualizer-canvas-body")
                .max_rect(body)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        canvas.set_clip_rect(body.intersect(view.clip_rect()));
        canvas.spacing_mut().item_spacing = vec2(6.0, 4.0);
        let canvas_id = if top {
            self.visualizer_top_ui(&mut canvas);
            None
        } else {
            Some(self.visualizer_canvas(&mut canvas, snapshot, screen_id, render_context, false))
        };

        // Selection.
        let sel_h = (side.height() * 0.55).round();
        let sel_rect = Rect::from_min_size(side.min, vec2(side.width(), sel_h));
        let stage_rect = Rect::from_min_max(pos2(side.left(), sel_rect.bottom() + gap), side.max);
        let mut sel_ui = desk::pane_at(ui, sel_rect, t.pane, "visualizer-selection");
        let sel = self.visualizer_selection_info();
        let mut action = None;
        let can_detach = self.visualizer_can_detach(&sel);
        studio::pane_header(&mut sel_ui, None, "Selection", false, |_| {});
        duo::body(&mut sel_ui, None, |ui| {
            ui.spacing_mut().interact_size.y = TARGET_MIN;
            if !editing {
                duo::note(
                    ui,
                    "Switch to Edit to select and place fixtures and objects.",
                );
                return;
            }
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                for (i, (label, enabled)) in [
                    ("Attach", sel.can_attach()),
                    ("Detach", can_detach),
                    ("Duplicate", sel.only_objects()),
                    ("Delete", sel.only_objects()),
                ]
                .into_iter()
                .enumerate()
                {
                    let r = ui
                        .add_enabled_ui(enabled, |ui| {
                            studio::mini_button(ui, label, false, TARGET_MIN)
                        })
                        .inner;
                    let r = if i == 0 {
                        r.on_disabled_hover_text("Select one fixture and one truss")
                    } else {
                        r
                    };
                    if r.clicked() {
                        action = Some(i);
                    }
                }
            });
            ui.add_space(6.0);
            egui::ScrollArea::vertical()
                .id_salt("visualizer-selection-scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| self.visualizer_editor_inspector(ui));
        });
        match action {
            Some(0) => self.visualizer_attach(&sel),
            Some(1) => self.visualizer_detach(&sel),
            Some(2) => self.visualizer_duplicate(&sel),
            Some(3) => self.visualizer_delete(sel),
            _ => {}
        }

        // Stage.
        let mut stage_ui = desk::pane_at(ui, stage_rect, t.pane, "visualizer-stage");
        studio::pane_header(&mut stage_ui, None, "Stage", false, |_| {});
        let object_count = self.visualizer_ui_state.stage.objects.len();
        let mut retry = false;
        let status = canvas_id.map(|id| self.visualizer_status(screen_id, id));
        duo::body(&mut stage_ui, None, |ui| {
            ui.spacing_mut().interact_size.y = TARGET_MIN;
            let settings = &mut self.visualizer_ui_state.settings;
            let mut dims: Vec<(&str, &mut f32, f32)> = vec![
                ("Width", &mut settings.room_width, MAX_ROOM_DIMENSION),
                ("Depth", &mut settings.room_depth, MAX_ROOM_DIMENSION),
            ];
            if editing {
                dims.push(("Height", &mut settings.room_height, 50.0));
            }
            for (label, value, max) in dims {
                duo::form_row(ui, label, |ui| {
                    let mut v = f64::from(*value);
                    if studio::num_field(
                        ui,
                        &mut v,
                        0.5,
                        f64::from(MIN_ROOM_DIMENSION)..=f64::from(max),
                        1,
                        64.0,
                    )
                    .changed()
                    {
                        *value = v as f32;
                        room_changed = true;
                    }
                    unit(ui, "m");
                });
            }
            duo::form_row(ui, "Brightness", |ui| {
                let mut pct = f64::from((settings.brightness * 100.0).round());
                if studio::num_field(ui, &mut pct, 10.0, 50.0..=500.0, 0, 64.0).changed() {
                    settings.brightness = (pct as f32 / 100.0).clamp(0.5, 5.0);
                }
                unit(ui, "%");
            });
            duo::form_row(ui, "Diagnostics", |ui| {
                studio::switch(ui, &mut settings.show_diagnostics);
            });
            if editing {
                let editor = &mut self.visualizer_ui_state.editor;
                duo::form_row(ui, "Snap steps", |ui| {
                    let mut pos = f64::from(editor.position_snap);
                    if studio::num_field(ui, &mut pos, 0.05, 0.01..=10.0, 2, 64.0).changed() {
                        editor.position_snap = pos as f32;
                    }
                    unit(ui, "m");
                    let mut rot = f64::from(editor.rotation_snap);
                    if studio::num_field(ui, &mut rot, 1.0, 1.0..=90.0, 0, 52.0).changed() {
                        editor.rotation_snap = rot as f32;
                    }
                    unit(ui, "deg");
                });
            }
            duo::form_row(ui, "Objects", |ui| duo::text(ui, object_count.to_string()));
            if editing {
                self.visualizer_desktop_import(ui);
            }
            if let Some(lines) = &status {
                ui.add_space(6.0);
                for (warning, line) in &lines.lines {
                    if *warning {
                        desk::error_text(ui, line);
                    } else {
                        duo::note(ui, line.as_str());
                    }
                }
                if lines.renderer_error {
                    retry = studio::mini_button(ui, "Retry renderer", false, TARGET_MIN).clicked();
                }
            }
        });
        if retry {
            if let Some(id) = canvas_id {
                let target = self
                    .visualizer_ui_state
                    .render_target(egui::Id::new((screen_id, id)));
                if let Ok(mut shared) = target.shared.lock() {
                    shared.retry_renderer();
                };
            }
        }
        if room_changed {
            let settings = self.visualizer_ui_state.settings;
            let room = &mut self.visualizer_ui_state.stage.room;
            room.width = settings.room_width;
            room.depth = settings.room_depth;
            room.height = settings.room_height;
            room.visible = settings.show_room;
            self.commit_visualizer_stage();
        }
    }

    /// The desktop Stage pane's Import GLB row.
    fn visualizer_desktop_import(&mut self, ui: &mut egui::Ui) {
        use crate::app::components::studio;
        use crate::app::pages::system::desk;
        use crate::app::ui_tokens::TARGET_MIN;
        let importing = self.visualizer_importing();
        #[cfg(feature = "file-dialog")]
        {
            let showfile = self
                .client
                .with_show(|show| show.showfile().map(|path| path.to_path_buf()));
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let enabled = showfile.is_some() && !importing;
                if ui
                    .add_enabled_ui(enabled, |ui| {
                        studio::mini_button(ui, "Import GLB", false, TARGET_MIN)
                    })
                    .inner
                    .clicked()
                {
                    self.visualizer_open_import(showfile);
                }
                if importing {
                    ui.spinner();
                }
            });
        }
        #[cfg(not(feature = "file-dialog"))]
        if importing {
            ui.spinner();
        }
        if let Some(error) = &self.visualizer_ui_state.import_error {
            desk::error_text(ui, error);
        }
    }

    /// The 3D view's status lines (renderer error, scene data problems,
    /// diagnostics) for the desktop Stage pane.
    fn visualizer_status(&mut self, screen_id: ScreenId, canvas_id: egui::Id) -> VisualizerStatus {
        let diagnostics = self.visualizer_ui_state.settings.show_diagnostics;
        let target = self
            .visualizer_ui_state
            .render_target(egui::Id::new((screen_id, canvas_id)));
        let renderer_error = target
            .shared
            .lock()
            .ok()
            .and_then(|shared| shared.last_error.clone());
        let frame = target.frame.lock().ok().map(|frame| {
            (
                frame.snapshot.skipped_fixtures,
                frame.snapshot.fixtures.len(),
                frame.snapshot.error.clone(),
            )
        });
        let mut lines = Vec::new();
        if let Some(err) = &renderer_error {
            lines.push((true, format!("Renderer error: {err}")));
        }
        if let Some((skipped, count, error)) = frame {
            if let Some(err) = error {
                lines.push((true, format!("Scene data: {err}")));
            }
            if diagnostics {
                lines.push((false, format!("Fixtures: {count}, skipped: {skipped}")));
            }
        }
        VisualizerStatus {
            lines,
            renderer_error: renderer_error.is_some(),
        }
    }
}

/// Status lines under the desktop Stage pane: (is a warning, text).
struct VisualizerStatus {
    lines: Vec<(bool, String)>,
    renderer_error: bool,
}

/// A muted unit after a number field.
fn unit(ui: &mut egui::Ui, text: &str) {
    let t = crate::app::ui_tokens::studio(ui.ctx());
    let f = crate::app::ui_tokens::Density::of(ui.ctx()).font(
        crate::app::ui_tokens::Weight::Regular,
        crate::app::ui_tokens::TEXT,
    );
    ui.label(egui::RichText::new(text).font(f).color(t.muted));
}

/// The kiosk's object inspector: one wrapped row of every field.
fn kiosk_object_row(ui: &mut egui::Ui, id: u64, object: &mut StageObject, connected: bool) {
    ui.horizontal_wrapped(|ui| {
        ui.text_edit_singleline(&mut object.name);
        ui.checkbox(&mut object.visible, "Visible");
        ui.checkbox(&mut object.locked, "Locked");
        for (axis, label) in ["X", "Y", "Z"].into_iter().enumerate() {
            ui.label(label);
            ui.add(
                egui::DragValue::new(&mut object.transform.translation[axis])
                    .speed(0.05)
                    .suffix(" m"),
            );
        }
        for (axis, label) in ["RX", "RY", "RZ"].into_iter().enumerate() {
            ui.label(label);
            ui.add(
                egui::DragValue::new(&mut object.transform.rotation[axis])
                    .speed(0.5)
                    .suffix(" deg"),
            );
        }
        if let StageObjectKind::ProceduralTruss(spec) = &mut object.kind {
            ui.add_enabled_ui(!connected, |ui| {
                if dropdown::is_desktop(ui) {
                    let profiles: Vec<_> =
                        TrussProfile::ALL.iter().map(|p| (*p, p.label())).collect();
                    dropdown::select_value(
                        ui,
                        ("truss-profile", id),
                        &mut spec.profile,
                        &profiles,
                        Some(120.0),
                    );
                    let parts: Vec<_> = TrussPart::ALL.iter().map(|p| (*p, p.label())).collect();
                    dropdown::select_value(
                        ui,
                        ("truss-part", id),
                        &mut spec.part,
                        &parts,
                        Some(120.0),
                    );
                } else {
                    dropdown::kiosk_combo(
                        ui,
                        ("truss-profile", id),
                        None,
                        spec.profile.label(),
                        |ui| {
                            for profile in TrussProfile::ALL {
                                ui.selectable_value(&mut spec.profile, profile, profile.label());
                            }
                        },
                    );
                    dropdown::kiosk_combo(ui, ("truss-part", id), None, spec.part.label(), |ui| {
                        for part in TrussPart::ALL {
                            ui.selectable_value(&mut spec.part, part, part.label());
                        }
                    });
                }
            })
            .response
            .on_disabled_hover_text("Disconnect all endpoints before changing truss type");
            if spec.part == TrussPart::Straight {
                ui.add_enabled(
                    !connected,
                    egui::DragValue::new(&mut spec.length_m)
                        .range(0.25..=50.0)
                        .speed(0.25)
                        .suffix(" m"),
                )
                .on_disabled_hover_text("Disconnect all endpoints before changing length");
                spec.sanitize();
                let breakdown = spec
                    .section_breakdown()
                    .into_iter()
                    .map(|length| format!("{length}m"))
                    .collect::<Vec<_>>()
                    .join(" + ");
                ui.label(breakdown);
            }
        } else {
            for (axis, label) in ["SX", "SY", "SZ"].into_iter().enumerate() {
                ui.label(label);
                ui.add(
                    egui::DragValue::new(&mut object.transform.scale[axis])
                        .range(0.01..=1_000.0)
                        .speed(0.05),
                );
            }
        }
    });
}

/// The desktop's object inspector: label/value rows like the mockup's
/// `pg-frm`, so every axis keeps its value beside it in the 280px pane.
fn desktop_object_form(ui: &mut egui::Ui, id: u64, object: &mut StageObject, connected: bool) {
    use crate::app::pages::setup::duo;
    const AXIS_W: f32 = 52.0;
    let axes = |ui: &mut egui::Ui,
                values: &mut [f32; 3],
                labels: [&str; 3],
                speed: f64,
                suffix: &str,
                range: Option<std::ops::RangeInclusive<f32>>| {
        ui.spacing_mut().item_spacing.x = 3.0;
        for (value, label) in values.iter_mut().zip(labels) {
            let mut drag = egui::DragValue::new(value)
                .speed(speed)
                .prefix(format!("{label} "))
                .suffix(suffix);
            if let Some(range) = range.clone() {
                drag = drag.range(range);
            }
            ui.add_sized([AXIS_W, ui.spacing().interact_size.y], drag);
        }
    };
    duo::form_row(ui, "Name", |ui| {
        ui.add(egui::TextEdit::singleline(&mut object.name).desired_width(f32::INFINITY));
    });
    duo::form_row(ui, "Flags", |ui| {
        ui.checkbox(&mut object.visible, "Visible");
        ui.checkbox(&mut object.locked, "Locked");
    });
    duo::form_row(ui, "Position m", |ui| {
        axes(
            ui,
            &mut object.transform.translation,
            ["X", "Y", "Z"],
            0.05,
            "",
            None,
        );
    });
    duo::form_row(ui, "Rotation deg", |ui| {
        axes(
            ui,
            &mut object.transform.rotation,
            ["X", "Y", "Z"],
            0.5,
            "",
            None,
        );
    });
    if let StageObjectKind::ProceduralTruss(spec) = &mut object.kind {
        let hint = "Disconnect all endpoints before changing truss type";
        duo::form_row(ui, "Profile", |ui| {
            ui.add_enabled_ui(!connected, |ui| {
                let profiles: Vec<_> = TrussProfile::ALL.iter().map(|p| (*p, p.label())).collect();
                dropdown::select_value(
                    ui,
                    ("truss-profile", id),
                    &mut spec.profile,
                    &profiles,
                    Some(150.0),
                );
            })
            .response
            .on_disabled_hover_text(hint);
        });
        duo::form_row(ui, "Part", |ui| {
            ui.add_enabled_ui(!connected, |ui| {
                let parts: Vec<_> = TrussPart::ALL.iter().map(|p| (*p, p.label())).collect();
                dropdown::select_value(ui, ("truss-part", id), &mut spec.part, &parts, Some(150.0));
            })
            .response
            .on_disabled_hover_text(hint);
        });
        if spec.part == TrussPart::Straight {
            duo::form_row(ui, "Length", |ui| {
                ui.add_enabled(
                    !connected,
                    egui::DragValue::new(&mut spec.length_m)
                        .range(0.25..=50.0)
                        .speed(0.25)
                        .suffix(" m"),
                )
                .on_disabled_hover_text("Disconnect all endpoints before changing length");
            });
            spec.sanitize();
            let breakdown = spec
                .section_breakdown()
                .into_iter()
                .map(|length| format!("{length}m"))
                .collect::<Vec<_>>()
                .join(" + ");
            duo::form_row(ui, "Sections", |ui| duo::text(ui, breakdown));
        }
    } else {
        duo::form_row(ui, "Scale", |ui| {
            axes(
                ui,
                &mut object.transform.scale,
                ["X", "Y", "Z"],
                0.05,
                "",
                Some(0.01..=1_000.0),
            );
        });
    }
}

/// A 1px vertical divider between groups of bar controls.
fn divider(ui: &mut egui::Ui, color: Color32) {
    let (r, _) = ui.allocate_exact_size(egui::vec2(9.0, 24.0), Sense::hover());
    ui.painter().vline(
        r.center().x.round() + 0.5,
        r.y_range(),
        Stroke::new(1.0_f32, color),
    );
}

/// What the editor's selection holds, for the edit actions.
struct VisualizerSelection {
    count: usize,
    objects: Vec<u64>,
    fixture: Option<(u8, u8)>,
    truss: Option<u64>,
}

impl VisualizerSelection {
    /// One fixture and one truss.
    fn can_attach(&self) -> bool {
        self.count == 2 && self.objects.len() == 1 && self.fixture.is_some() && self.truss.is_some()
    }

    /// Only stage objects, at least one.
    fn only_objects(&self) -> bool {
        !self.objects.is_empty() && self.objects.len() == self.count
    }
}

fn placement((group, fixture): (u8, u8), (position, rotation): Pose) -> FixturePlacement {
    FixturePlacement {
        group,
        fixture,
        position,
        rotation,
    }
}

fn hit_test_top(
    pointer: Pos2,
    pixels_per_meter: f32,
    fixtures: &[((u8, u8), String, [f32; 2])],
    stage: &blaulicht_engine::stage::StageScene,
    to_screen: &impl Fn(f32, f32) -> Pos2,
) -> Option<SelectionKey> {
    for (id, object) in stage.objects.iter().rev() {
        if !object.visible || object.locked {
            continue;
        }
        let footprint = object_footprint(object);
        let rect = Rect::from_center_size(
            to_screen(
                object.transform.translation[0],
                object.transform.translation[2],
            ),
            Vec2::new(
                footprint[0] * pixels_per_meter,
                footprint[1] * pixels_per_meter,
            ),
        );
        if rect.expand(4.0).contains(pointer) {
            return Some(SelectionKey::Object(*id));
        }
    }
    fixtures.iter().rev().find_map(|(id, _, position)| {
        (to_screen(position[0], position[1]).distance(pointer) <= 12.0)
            .then_some(SelectionKey::Fixture(id.0, id.1))
    })
}

fn object_footprint(object: &StageObject) -> [f32; 2] {
    let base = match object.kind {
        StageObjectKind::ProceduralTruss(spec) => super::truss::truss_bounds(spec),
        _ => Vec3::new(
            object.transform.scale[0],
            object.transform.scale[1],
            object.transform.scale[2],
        ),
    };
    let yaw = object.transform.rotation[1].to_radians();
    let (sin, cos) = yaw.sin_cos();
    [
        base.x * cos.abs() + base.z * sin.abs(),
        base.x * sin.abs() + base.z * cos.abs(),
    ]
}

fn project_scene_items(
    rect: Rect,
    snapshot: &RenderSceneSnapshot,
    state: &super::state::VisualizerUiState,
) -> Vec<(SelectionKey, Pos2)> {
    if rect.width() <= 0.0 || rect.height() <= 0.0 {
        return Vec::new();
    }
    let projection = mat4_perspective(
        45.0_f32.to_radians(),
        rect.width() / rect.height(),
        0.1,
        200.0,
    );
    let (eye, target) = if state.free_camera {
        (
            state.camera_position,
            state
                .camera_position
                .add(camera_forward(state.camera_yaw, state.camera_pitch)),
        )
    } else {
        let cp = state.camera_pitch.cos();
        (
            Vec3::new(
                state.camera_radius * state.camera_yaw.sin() * cp,
                state.camera_radius * state.camera_pitch.sin(),
                state.camera_radius * state.camera_yaw.cos() * cp,
            ),
            Vec3::new(0.0, 0.0, 0.0),
        )
    };
    let view = mat4_look_at(eye, target, Vec3::new(0.0, 1.0, 0.0));
    let to_screen = |point: Vec3| {
        let point = project_point(projection, view, point)?;
        Some(Pos2::new(
            rect.left() + (point[0] + 1.0) * 0.5 * rect.width(),
            rect.top() + (1.0 - point[1]) * 0.5 * rect.height(),
        ))
    };
    let mut items = Vec::with_capacity(snapshot.fixtures.len() + snapshot.stage_objects.len());
    for object in snapshot.stage_objects.iter() {
        let center = object.pos.add(Vec3::new(0.0, object.scale.y * 0.5, 0.0));
        if let Some(center) = to_screen(center) {
            items.push((SelectionKey::Object(object.id), center));
        }
    }
    for fixture in snapshot.fixtures.iter() {
        if let Some(center) = to_screen(fixture.pos.add(Vec3::new(0.0, 0.45, 0.0))) {
            items.push((SelectionKey::Fixture(fixture.id.0, fixture.id.1), center));
        }
    }
    items
}

fn project_selected_truss_endpoints(
    rect: Rect,
    state: &super::state::VisualizerUiState,
    selection: &std::collections::BTreeSet<SelectionKey>,
) -> Vec<Pos2> {
    if rect.width() <= 0.0 || rect.height() <= 0.0 {
        return Vec::new();
    }
    let projection = mat4_perspective(
        45.0_f32.to_radians(),
        rect.width() / rect.height(),
        0.1,
        200.0,
    );
    let (eye, target) = if state.free_camera {
        (
            state.camera_position,
            state
                .camera_position
                .add(camera_forward(state.camera_yaw, state.camera_pitch)),
        )
    } else {
        let cp = state.camera_pitch.cos();
        (
            Vec3::new(
                state.camera_radius * state.camera_yaw.sin() * cp,
                state.camera_radius * state.camera_pitch.sin(),
                state.camera_radius * state.camera_yaw.cos() * cp,
            ),
            Vec3::new(0.0, 0.0, 0.0),
        )
    };
    let view = mat4_look_at(eye, target, Vec3::new(0.0, 1.0, 0.0));
    selection
        .iter()
        .filter_map(|key| match key {
            SelectionKey::Object(id) => Some(*id),
            SelectionKey::Fixture(_, _) => None,
        })
        .flat_map(|id| {
            let Some(object) = state.stage.objects.get(&id) else {
                return Vec::new();
            };
            let StageObjectKind::ProceduralTruss(spec) = object.kind else {
                return Vec::new();
            };
            super::truss::world_endpoints(spec, &object.transform)
        })
        .filter_map(|endpoint| {
            let point = project_point(projection, view, endpoint.position)?;
            Some(Pos2::new(
                rect.left() + (point[0] + 1.0) * 0.5 * rect.width(),
                rect.top() + (1.0 - point[1]) * 0.5 * rect.height(),
            ))
        })
        .collect()
}

fn hit_test_projected(
    pointer: Pos2,
    items: &[(SelectionKey, Pos2)],
    stage: &blaulicht_engine::stage::StageScene,
) -> Option<SelectionKey> {
    items
        .iter()
        .filter(|(key, _)| match key {
            SelectionKey::Object(id) => stage.objects.get(id).is_some_and(|object| !object.locked),
            SelectionKey::Fixture(_, _) => true,
        })
        .filter_map(|(key, center)| {
            let distance = center.distance(pointer);
            (distance <= 18.0).then_some((*key, distance))
        })
        .min_by(|left, right| left.1.total_cmp(&right.1))
        .map(|(key, _)| key)
}

fn finite_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value
    } else {
        fallback
    }
}

fn normalize_angle(angle: f32) -> f32 {
    (angle + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}

fn snap(value: f32, step: f32) -> f32 {
    (value / step).round() * step
}

fn reset_camera(state: &mut super::state::VisualizerUiState) {
    state.camera_yaw = DEFAULT_CAMERA_YAW;
    state.camera_pitch = DEFAULT_CAMERA_PITCH;
    state.camera_radius = DEFAULT_CAMERA_RADIUS;
    state.free_camera = false;
    state.camera_position = Vec3::new(0.0, 0.0, 0.0);
    state.drag_start_yaw = None;
    state.drag_start_pitch = None;
    state.drag_last_pos = None;
}

fn enter_free_camera(state: &mut super::state::VisualizerUiState) {
    let yaw = state.camera_yaw;
    let orbit_pitch = state.camera_pitch;
    let cp = orbit_pitch.cos();
    state.camera_position = Vec3::new(
        state.camera_radius * yaw.sin() * cp,
        state.camera_radius * orbit_pitch.sin(),
        state.camera_radius * yaw.cos() * cp,
    );
    state.camera_pitch = -orbit_pitch;
    state.free_camera = true;
}

fn camera_forward(yaw: f32, pitch: f32) -> Vec3 {
    Vec3::new(
        pitch.cos() * yaw.sin(),
        pitch.sin(),
        -pitch.cos() * yaw.cos(),
    )
    .normalize()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_camera_faces_scene_origin() {
        let mut state = super::super::VisualizerUiState::default();
        enter_free_camera(&mut state);
        let target_direction = Vec3::new(0.0, 0.0, 0.0)
            .sub(state.camera_position)
            .normalize();
        assert!(camera_forward(state.camera_yaw, state.camera_pitch).dot(target_direction) > 0.999);
    }

    #[test]
    fn forward_vector_remains_normalized() {
        let forward = camera_forward(2.4, -0.7);
        assert!((forward.norm() - 1.0).abs() < 1e-5);
    }
}
