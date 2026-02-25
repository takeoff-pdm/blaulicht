use std::sync::Arc;

use egui::{Context, Sense, Vec2};
use egui_glow::CallbackFn;

use crate::app::BlaulichtApp;

use super::data::collect_fixtures;

impl BlaulichtApp {
    pub fn visualizer_ui(&mut self, ui: &mut egui::Ui, _ctx: &Context) {
        ui.heading("Visualizer");
        ui.add_space(4.0);

        let spacing = ui.spacing().item_spacing;
        ui.spacing_mut().item_spacing = egui::vec2(6.0, 4.0);

        ui.horizontal(|ui| {
            ui.checkbox(&mut self.visualizer_ui_state.settings.show_grid, "Grid");
            ui.checkbox(&mut self.visualizer_ui_state.settings.show_axes, "Axes");
            if ui.small_button("Reset Cam").clicked() {
                self.visualizer_ui_state.camera_yaw = 0.0;
                self.visualizer_ui_state.camera_pitch = -0.3;
                self.visualizer_ui_state.camera_radius = 12.0;
            }
        });

        ui.horizontal(|ui| {
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
        });

        ui.add_space(6.0);

        let canvas_size = Vec2::new(ui.available_width(), (ui.available_height()).max(120.0));
        let (rect, _response) = ui.allocate_exact_size(canvas_size, Sense::hover());
        let canvas_id = ui.make_persistent_id("visualizer_canvas");
        let response = ui.interact(rect, canvas_id, Sense::drag());

        if response.drag_started_by(egui::PointerButton::Primary) {
            self.visualizer_ui_state.drag_start_yaw = Some(self.visualizer_ui_state.camera_yaw);
            self.visualizer_ui_state.drag_start_pitch = Some(self.visualizer_ui_state.camera_pitch);
            self.visualizer_ui_state.drag_last_pos = response.interact_pointer_pos();
        }

        if response.dragged_by(egui::PointerButton::Primary) {
            if let (Some(start_yaw), Some(start_pitch)) = (
                self.visualizer_ui_state.drag_start_yaw,
                self.visualizer_ui_state.drag_start_pitch,
            ) {
                if let Some(pos) = response.interact_pointer_pos() {
                    if let Some(last_pos) = self.visualizer_ui_state.drag_last_pos {
                        let delta = pos - last_pos;
                        let rotate_speed = 0.02;
                        self.visualizer_ui_state.camera_yaw -= delta.x * rotate_speed;
                        self.visualizer_ui_state.camera_pitch =
                            (self.visualizer_ui_state.camera_pitch + delta.y * rotate_speed)
                                .clamp(-1.5, 1.2);
                    } else {
                        self.visualizer_ui_state.camera_yaw = start_yaw;
                        self.visualizer_ui_state.camera_pitch = start_pitch;
                    }
                    self.visualizer_ui_state.drag_last_pos = Some(pos);
                }
            }
        }

        if response.drag_stopped_by(egui::PointerButton::Primary) {
            self.visualizer_ui_state.drag_start_yaw = None;
            self.visualizer_ui_state.drag_start_pitch = None;
            self.visualizer_ui_state.drag_last_pos = None;
        }

        if response.hovered() {
            let zoom_delta = ui.input(|i| i.smooth_scroll_delta.y);
            if zoom_delta.abs() > 0.0 {
                self.visualizer_ui_state.camera_radius =
                    (self.visualizer_ui_state.camera_radius - zoom_delta * 0.02)
                        .clamp(4.0, 60.0);
            }
        }

        let fixtures = collect_fixtures(&self.data.state.dmx_engine.read().unwrap());
        {
            let mut shared = self.visualizer_ui_state.shared.lock().unwrap();
            shared.settings = self.visualizer_ui_state.settings;
            shared.camera_yaw = self.visualizer_ui_state.camera_yaw;
            shared.camera_pitch = self.visualizer_ui_state.camera_pitch;
            shared.camera_radius = self.visualizer_ui_state.camera_radius;
            shared.time = self.animation_time;
            shared.fixtures = fixtures;
        }

        let shared = self.visualizer_ui_state.shared.clone();
        let callback = CallbackFn::new(move |info, painter| {
            let mut shared = shared.lock().unwrap();
            shared.ensure_renderer(painter.gl());
            let settings = shared.settings;
            let yaw = shared.camera_yaw;
            let pitch = shared.camera_pitch;
            let radius = shared.camera_radius;
            let time = shared.time;
            let fixtures = shared.fixtures.clone();
            if let Some(renderer) = shared.renderer.as_mut() {
                renderer.render(
                    painter.gl(),
                    &settings,
                    yaw,
                    pitch,
                    radius,
                    time,
                    &fixtures,
                    info,
                );
            }
        });

        ui.painter().add(egui::Shape::Callback(egui::PaintCallback {
            rect,
            callback: Arc::new(callback),
        }));

        if let Some(err) = self.visualizer_ui_state.shared.lock().unwrap().last_error.as_ref() {
            ui.add_space(6.0);
            ui.label(format!("Renderer error: {err}"));
        }

        ui.spacing_mut().item_spacing = spacing;
    }
}
