use std::sync::Arc;

use egui::{Color32, Context, Pos2, Rect, Sense, Stroke, Vec2};
use egui_glow::CallbackFn;

use crate::app::page::PageRenderContext;
use crate::app::BlaulichtApp;
use crate::stage::{StageObject, StageObjectKind};
use crate::state::ScreenId;

use super::constants::{MAX_ROOM_DIMENSION, MIN_ROOM_DIMENSION};
use super::data::{collect_fixtures, RenderSceneSnapshot};
use super::math::{mat4_look_at, mat4_perspective, project_point, Vec3};
use super::state::{
    EditorSnapshot, EditorTool, EditorView, SelectionKey, VisualizerMode, DEFAULT_CAMERA_PITCH,
    DEFAULT_CAMERA_RADIUS, DEFAULT_CAMERA_YAW,
};

impl BlaulichtApp {
    pub fn visualizer_ui(
        &mut self,
        ui: &mut egui::Ui,
        _ctx: &Context,
        screen_id: ScreenId,
        render_context: PageRenderContext,
    ) {
        self.visualizer_import_dialog(ui.ctx());
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
        let snapshot = collect_fixtures(
            &self.data.state,
            &self.visualizer_ui_state.stage,
            &self.visualizer_ui_state.model_cache,
        );
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
                ui.add_sized(
                    [110.0, 18.0],
                    egui::Slider::new(
                        &mut self.visualizer_ui_state.settings.room_width,
                        MIN_ROOM_DIMENSION..=MAX_ROOM_DIMENSION,
                    )
                    .suffix(" m"),
                );
                self.visualizer_ui_state.stage.room.width =
                    self.visualizer_ui_state.settings.room_width;
                ui.label("Depth");
                ui.add_sized(
                    [110.0, 18.0],
                    egui::Slider::new(
                        &mut self.visualizer_ui_state.settings.room_depth,
                        MIN_ROOM_DIMENSION..=MAX_ROOM_DIMENSION,
                    )
                    .suffix(" m"),
                );
                self.visualizer_ui_state.stage.room.depth =
                    self.visualizer_ui_state.settings.room_depth;
                if self.visualizer_ui_state.editor.mode == VisualizerMode::Edit {
                    ui.label("Height");
                    ui.add_sized(
                        [110.0, 18.0],
                        egui::Slider::new(
                            &mut self.visualizer_ui_state.settings.room_height,
                            MIN_ROOM_DIMENSION..=50.0,
                        )
                        .suffix(" m"),
                    );
                    self.visualizer_ui_state.stage.room.height =
                        self.visualizer_ui_state.settings.room_height;
                }
            }
        });

        ui.add_space(6.0);

        if self.visualizer_ui_state.editor.mode == VisualizerMode::Edit {
            self.visualizer_editor_inspector(ui);
            if self.visualizer_ui_state.editor.view == EditorView::Top {
                self.visualizer_top_ui(ui);
                ui.spacing_mut().item_spacing = spacing;
                return;
            }
        }

        let minimum_canvas_height =
            if render_context.is_dynamic() && render_context.short {
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
                    super::state::ModelLoadState::Queued
                        | super::state::ModelLoadState::Preparing
                )
            })
            .count();
        let cpu_failure = self
            .visualizer_ui_state
            .model_states
            .iter()
            .find_map(|(key, state)| match state {
                super::state::ModelLoadState::Failed(error) => {
                    Some((key.clone(), error.clone()))
                }
                _ => None,
            });
        let gpu_failure = gpu_progress.failed_assets.first().cloned();
        let asset_failure = cpu_failure.or(gpu_failure);
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
                                let asset_ids: std::collections::BTreeSet<_> = self
                                    .visualizer_ui_state
                                    .stage
                                    .assets
                                    .iter()
                                    .filter_map(|(id, asset)| {
                                        (asset.content_hash == key).then_some(*id)
                                    })
                                    .collect();
                                self.visualizer_ui_state.stage.objects.retain(|_, object| {
                                    !matches!(
                                        object.kind,
                                        StageObjectKind::ImportedModel { asset_id }
                                            if asset_ids.contains(&asset_id)
                                    )
                                });
                                self.visualizer_ui_state
                                    .stage
                                    .assets
                                    .retain(|id, _| !asset_ids.contains(id));
                                self.visualizer_ui_state.model_states.remove(&key);
                                self.visualizer_ui_state.model_cache.remove(&key);
                                self.visualizer_ui_state.editor.selection.clear();
                                self.visualizer_ui_state.import_error = None;
                                self.mark_showfile_dirty();
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
                    format!(
                        "Uploading {} model(s)...",
                        gpu_progress.uploading_assets
                    ),
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
        let frame_status = render_target.frame.lock().ok().map(|frame| {
            (
                frame.snapshot.skipped_fixtures,
                frame.snapshot.fixtures.len(),
                frame.snapshot.error.clone(),
            )
        });
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

        ui.spacing_mut().item_spacing = spacing;
    }

    fn capture_visualizer_snapshot(&self) -> EditorSnapshot {
        let fixtures = self
            .data
            .state
            .dmx_engine
            .read()
            .ok()
            .map(|engine| {
                engine
                    .0
                    .groups
                    .iter()
                    .flat_map(|(gid, group)| {
                        group.fixtures.iter().map(move |(fid, fixture)| {
                            (
                                (*gid, *fid),
                                (fixture.pos.clone(), fixture.rotation.clone()),
                            )
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        EditorSnapshot {
            stage: self.visualizer_ui_state.stage.clone(),
            fixtures,
        }
    }

    fn restore_visualizer_snapshot(&mut self, snapshot: EditorSnapshot) {
        self.visualizer_ui_state.stage = snapshot.stage;
        if let Ok(mut engine) = self.data.state.dmx_engine.write() {
            for ((gid, fid), (pos, rotation)) in snapshot.fixtures {
                if let Some(fixture) = engine
                    .0
                    .groups
                    .get_mut(&gid)
                    .and_then(|group| group.fixtures.get_mut(&fid))
                {
                    fixture.pos = pos;
                    fixture.rotation = rotation;
                }
            }
        }
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
            ui.menu_button("Add", |ui| {
                for (label, kind) in [
                    ("Platform", StageObjectKind::Platform),
                    ("Truss", StageObjectKind::Truss),
                    ("Wall", StageObjectKind::Wall),
                    ("Speaker", StageObjectKind::Speaker),
                    ("Screen", StageObjectKind::Screen),
                    ("Box", StageObjectKind::Box),
                ] {
                    if ui.button(label).clicked() {
                        let before = self.capture_visualizer_snapshot();
                        let index = self.visualizer_ui_state.stage.objects.len() + 1;
                        let id = self
                            .visualizer_ui_state
                            .stage
                            .add_object(StageObject::preset(kind, index));
                        self.visualizer_ui_state.editor.selection.clear();
                        self.visualizer_ui_state
                            .editor
                            .selection
                            .insert(SelectionKey::Object(id));
                        self.visualizer_ui_state.editor.push_undo(before);
                        ui.close();
                    }
                }
            });
            let has_showfile = self
                .data
                .config
                .lock()
                .ok()
                .and_then(|config| config.last_open_showfile.clone())
                .is_some();
            if ui
                .add_enabled(
                    has_showfile && self.visualizer_ui_state.import_receiver.is_none(),
                    egui::Button::new("Import GLB"),
                )
                .clicked()
            {
                let mut dialog = egui_file_dialog::FileDialog::new()
                    .add_file_filter_extensions("Binary glTF", vec!["glb"])
                    .default_file_filter("Binary glTF")
                    .as_modal(true)
                    .default_size(egui::vec2(850.0, 540.0));
                if let Some(parent) = self
                    .data
                    .config
                    .lock()
                    .ok()
                    .and_then(|config| config.last_open_showfile.clone())
                    .and_then(|path| path.parent().map(|parent| parent.to_path_buf()))
                {
                    dialog = dialog.initial_directory(parent);
                }
                dialog.pick_file();
                self.visualizer_ui_state.import_dialog = Some(dialog);
            }
            if self.visualizer_ui_state.import_receiver.is_some() {
                ui.spinner();
            }
            if let Some(error) = &self.visualizer_ui_state.import_error {
                ui.colored_label(Color32::LIGHT_RED, error);
            }

            let selected_objects: Vec<u64> = self
                .visualizer_ui_state
                .editor
                .selection
                .iter()
                .filter_map(|key| match key {
                    SelectionKey::Object(id) => Some(*id),
                    SelectionKey::Fixture(_, _) => None,
                })
                .collect();
            let only_objects = !selected_objects.is_empty()
                && selected_objects.len() == self.visualizer_ui_state.editor.selection.len();
            if ui
                .add_enabled(only_objects, egui::Button::new("Duplicate"))
                .clicked()
            {
                let before = self.capture_visualizer_snapshot();
                let mut new_selection = std::collections::BTreeSet::new();
                for id in selected_objects.iter().copied() {
                    if let Some(mut object) =
                        self.visualizer_ui_state.stage.objects.get(&id).cloned()
                    {
                        object.transform.translation[0] += 0.5;
                        object.transform.translation[2] += 0.5;
                        let new_id = self.visualizer_ui_state.stage.add_object(object);
                        new_selection.insert(SelectionKey::Object(new_id));
                    }
                }
                self.visualizer_ui_state.editor.selection = new_selection;
                self.visualizer_ui_state.editor.push_undo(before);
            }
            if ui
                .add_enabled(only_objects, egui::Button::new("Delete"))
                .clicked()
            {
                let before = self.capture_visualizer_snapshot();
                for id in selected_objects.iter().copied() {
                    self.visualizer_ui_state.stage.remove_object(id);
                }
                self.visualizer_ui_state.editor.selection.clear();
                self.visualizer_ui_state.editor.push_undo(before);
            }
        });
    }

    fn visualizer_import_dialog(&mut self, ctx: &Context) {
        if let Some(mut dialog) = self.visualizer_ui_state.import_dialog.take() {
            dialog.update(ctx);
            if let Some(source) = dialog.take_picked() {
                let showfile = self
                    .data
                    .config
                    .lock()
                    .ok()
                    .and_then(|config| config.last_open_showfile.clone());
                if let Some(showfile) = showfile {
                    let (sender, receiver) = std::sync::mpsc::channel();
                    self.visualizer_ui_state.import_receiver = Some(receiver);
                    self.visualizer_ui_state.import_error = None;
                    std::thread::spawn(move || {
                        let result = crate::stage_assets::import_glb(&source, &showfile)
                            .map_err(|error| error.to_string());
                        let _ = sender.send(result);
                    });
                } else {
                    self.visualizer_ui_state.import_error =
                        Some("Save the showfile before importing models".to_string());
                }
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
                Ok(asset) => {
                    let before = self.capture_visualizer_snapshot();
                    let original_name = asset.original_name.clone();
                    let asset_id = self.visualizer_ui_state.stage.add_asset(asset);
                    let index = self.visualizer_ui_state.stage.objects.len() + 1;
                    let mut object =
                        StageObject::preset(StageObjectKind::ImportedModel { asset_id }, index);
                    object.name = original_name;
                    let object_id = self.visualizer_ui_state.stage.add_object(object);
                    self.visualizer_ui_state.editor.selection.clear();
                    self.visualizer_ui_state
                        .editor
                        .selection
                        .insert(SelectionKey::Object(object_id));
                    self.visualizer_ui_state.editor.push_undo(before);
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
                        self.visualizer_ui_state.model_states.insert(
                            key,
                            super::state::ModelLoadState::Failed(error.clone()),
                        );
                        self.visualizer_ui_state.import_error = Some(error);
                    }
                },
                _ => {}
            }
        }

        let showfile = self
            .data
            .config
            .lock()
            .ok()
            .and_then(|config| config.last_open_showfile.clone());
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
            let path = match crate::stage_assets::resolve_asset(&showfile, &asset) {
                Ok(path) => path,
                Err(error) => {
                    let error = error.to_string();
                    self.visualizer_ui_state.model_states.insert(
                        key,
                        super::state::ModelLoadState::Failed(error.clone()),
                    );
                    self.visualizer_ui_state.import_error = Some(error);
                    continue;
                }
            };
            let request = super::state::ModelWorkerRequest {
                key: key.clone(),
                path,
                generation: self.visualizer_ui_state.model_generation,
            };
            match self.visualizer_ui_state.model_request_sender.try_send(request) {
                Ok(()) => {
                    self.visualizer_ui_state
                        .model_states
                        .insert(key, super::state::ModelLoadState::Queued);
                }
                Err(crossbeam_channel::TrySendError::Full(_)) => break,
                Err(crossbeam_channel::TrySendError::Disconnected(_)) => {
                    let error = "Visualizer model loader is unavailable".to_string();
                    self.visualizer_ui_state.model_states.insert(
                        key,
                        super::state::ModelLoadState::Failed(error.clone()),
                    );
                    self.visualizer_ui_state.import_error = Some(error);
                }
            }
        }
    }

    fn visualizer_editor_inspector(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.label("Selection");
            if let Ok(engine) = self.data.state.dmx_engine.read() {
                for (gid, group) in &engine.0.groups {
                    for (fid, fixture) in &group.fixtures {
                        let key = SelectionKey::Fixture(*gid, *fid);
                        let selected = self.visualizer_ui_state.editor.selection.contains(&key);
                        if ui.selectable_label(selected, &fixture.name).clicked() {
                            if !ui.input(|input| input.modifiers.shift) {
                                self.visualizer_ui_state.editor.selection.clear();
                            }
                            if selected {
                                self.visualizer_ui_state.editor.selection.remove(&key);
                            } else {
                                self.visualizer_ui_state.editor.selection.insert(key);
                            }
                        }
                    }
                }
            }
            for (id, object) in &self.visualizer_ui_state.stage.objects {
                let key = SelectionKey::Object(*id);
                let selected = self.visualizer_ui_state.editor.selection.contains(&key);
                if ui.selectable_label(selected, &object.name).clicked() {
                    if !ui.input(|input| input.modifiers.shift) {
                        self.visualizer_ui_state.editor.selection.clear();
                    }
                    if selected {
                        self.visualizer_ui_state.editor.selection.remove(&key);
                    } else {
                        self.visualizer_ui_state.editor.selection.insert(key);
                    }
                }
            }
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
            None => {}
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
            for (axis, label) in ["SX", "SY", "SZ"].into_iter().enumerate() {
                ui.label(label);
                ui.add(
                    egui::DragValue::new(&mut object.transform.scale[axis])
                        .range(0.01..=1_000.0)
                        .speed(0.05),
                );
            }
        });
        object.transform.sanitize();
        if object != before_object {
            let before = self.capture_visualizer_snapshot();
            self.visualizer_ui_state.stage.objects.insert(id, object);
            self.visualizer_ui_state.editor.push_undo(before);
        }
    }

    fn visualizer_fixture_inspector(&mut self, ui: &mut egui::Ui, gid: u8, fid: u8) {
        let fixture = self
            .data
            .state
            .dmx_engine
            .read()
            .ok()
            .and_then(|engine| engine.0.groups.get(&gid)?.fixtures.get(&fid).cloned());
        let Some(fixture) = fixture else {
            self.visualizer_ui_state
                .editor
                .selection
                .remove(&SelectionKey::Fixture(gid, fid));
            return;
        };
        let mut pos = [fixture.pos.x, fixture.pos.y, fixture.pos.z];
        let mut rotation = [fixture.rotation.x, fixture.rotation.y, fixture.rotation.z];
        let old_pos = pos;
        let old_rotation = rotation;
        ui.horizontal_wrapped(|ui| {
            ui.label(&fixture.name);
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
            if let Ok(mut engine) = self.data.state.dmx_engine.write() {
                if let Some(fixture) = engine
                    .0
                    .groups
                    .get_mut(&gid)
                    .and_then(|group| group.fixtures.get_mut(&fid))
                {
                    fixture.pos.x = finite_or(pos[0], old_pos[0]);
                    fixture.pos.y = finite_or(pos[1], old_pos[1]);
                    fixture.pos.z = finite_or(pos[2], old_pos[2]);
                    fixture.rotation.x = finite_or(rotation[0], old_rotation[0]);
                    fixture.rotation.y = finite_or(rotation[1], old_rotation[1]);
                    fixture.rotation.z = finite_or(rotation[2], old_rotation[2]);
                }
            }
            self.visualizer_ui_state.editor.push_undo(before);
        }
    }

    fn visualizer_top_ui(&mut self, ui: &mut egui::Ui) {
        let size = Vec2::new(ui.available_width(), ui.available_height().max(160.0));
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

        let fixtures: Vec<((u8, u8), String, [f32; 2])> = self
            .data
            .state
            .dmx_engine
            .read()
            .ok()
            .map(|engine| {
                engine
                    .0
                    .groups
                    .iter()
                    .flat_map(|(gid, group)| {
                        group.fixtures.iter().map(move |(fid, fixture)| {
                            (
                                (*gid, *fid),
                                fixture.name.clone(),
                                [fixture.pos.x, fixture.pos.z],
                            )
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        for (id, object) in &self.visualizer_ui_state.stage.objects {
            if !object.visible {
                continue;
            }
            let object_rect = Rect::from_center_size(
                to_screen(
                    object.transform.translation[0],
                    object.transform.translation[2],
                ),
                Vec2::new(
                    object.transform.scale[0] * pixels_per_meter,
                    object.transform.scale[2] * pixels_per_meter,
                ),
            );
            let selected = self
                .visualizer_ui_state
                .editor
                .selection
                .contains(&SelectionKey::Object(*id));
            painter.rect_filled(
                object_rect,
                1.0,
                Color32::from_rgb(object.color[0], object.color[1], object.color[2]),
            );
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

        if response.drag_started_by(egui::PointerButton::Primary) {
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
                    if !shift && !self.visualizer_ui_state.editor.selection.contains(&hit) {
                        self.visualizer_ui_state.editor.selection.clear();
                    }
                    self.visualizer_ui_state.editor.selection.insert(hit);
                    if self.visualizer_ui_state.editor.tool != EditorTool::Select {
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
        if response.clicked_by(egui::PointerButton::Primary) {
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
        if response.dragged_by(egui::PointerButton::Primary) {
            if let Some(pointer) = response.interact_pointer_pos() {
                match self.visualizer_ui_state.editor.tool {
                    EditorTool::Translate => {
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
                    EditorTool::Select => {}
                }
            }
        }
        if response.drag_stopped_by(egui::PointerButton::Primary) {
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
                if let Ok(mut engine) = self.data.state.dmx_engine.write() {
                    for key in selection {
                        match key {
                            SelectionKey::Fixture(gid, fid) => {
                                if let Some(fixture) = engine
                                    .0
                                    .groups
                                    .get_mut(&gid)
                                    .and_then(|group| group.fixtures.get_mut(&fid))
                                {
                                    fixture.rotation.y += degrees;
                                }
                            }
                            SelectionKey::Object(id) => {
                                if let Some(object) =
                                    self.visualizer_ui_state.stage.objects.get_mut(&id)
                                {
                                    if !object.locked {
                                        object.transform.rotation[1] += degrees;
                                    }
                                }
                            }
                        }
                    }
                }
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
                        if !object.locked {
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
        if let Ok(mut engine) = self.data.state.dmx_engine.write() {
            for key in selection {
                match key {
                    SelectionKey::Fixture(gid, fid) => {
                        if let Some(fixture) = engine
                            .0
                            .groups
                            .get_mut(&gid)
                            .and_then(|group| group.fixtures.get_mut(&fid))
                        {
                            fixture.pos.x += dx;
                            fixture.pos.z += dz;
                        }
                    }
                    SelectionKey::Object(id) => {
                        if let Some(object) = self.visualizer_ui_state.stage.objects.get_mut(&id) {
                            if !object.locked {
                                object.transform.translation[0] += dx;
                                object.transform.translation[2] += dz;
                            }
                        }
                    }
                }
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
        if let Ok(mut engine) = self.data.state.dmx_engine.write() {
            for key in selection {
                match key {
                    SelectionKey::Fixture(gid, fid) => {
                        if let Some(fixture) = engine
                            .0
                            .groups
                            .get_mut(&gid)
                            .and_then(|group| group.fixtures.get_mut(&fid))
                        {
                            match tool {
                                EditorTool::Translate => {
                                    fixture.pos.x = snap(fixture.pos.x, position_step);
                                    fixture.pos.z = snap(fixture.pos.z, position_step);
                                }
                                EditorTool::Rotate => {
                                    fixture.rotation.y = snap(fixture.rotation.y, rotation_step);
                                }
                                EditorTool::Select | EditorTool::Scale => {}
                            }
                        }
                    }
                    SelectionKey::Object(id) => {
                        if let Some(object) = self.visualizer_ui_state.stage.objects.get_mut(&id) {
                            match tool {
                                EditorTool::Translate => {
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
                                    for value in &mut object.transform.scale {
                                        *value = snap(*value, scale_step).max(0.01);
                                    }
                                }
                                EditorTool::Select => {}
                            }
                        }
                    }
                }
            }
        }
    }
}

fn hit_test_top(
    pointer: Pos2,
    pixels_per_meter: f32,
    fixtures: &[((u8, u8), String, [f32; 2])],
    stage: &crate::stage::StageScene,
    to_screen: &impl Fn(f32, f32) -> Pos2,
) -> Option<SelectionKey> {
    for (id, object) in stage.objects.iter().rev() {
        if !object.visible || object.locked {
            continue;
        }
        let rect = Rect::from_center_size(
            to_screen(
                object.transform.translation[0],
                object.transform.translation[2],
            ),
            Vec2::new(
                object.transform.scale[0] * pixels_per_meter,
                object.transform.scale[2] * pixels_per_meter,
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

fn hit_test_projected(
    pointer: Pos2,
    items: &[(SelectionKey, Pos2)],
    stage: &crate::stage::StageScene,
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
