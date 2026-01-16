use crate::app::{
    components::{self, ButtonSize, Knob, SpeedKnob},
    BlaulichtApp,
};
use blaulicht_shared::{
    scene::Scene, AnimationSpeedModifier, ControlEvent, ControlEventMessage, EventOriginator,
};
use egui::{Color32, Context, FontId, Frame, Key, Margin, Pos2, RichText, Sense, TextEdit, Vec2};

const DEFAULT_NEW_VIEW_NAME: &str = "New View";
const VIEWS_PER_PAGE: usize = 5;

pub struct ViewPerfUI {
    overlay_picker_open: bool,
    overlay_selection: Option<u8>,
}

impl Default for ViewPerfUI {
    fn default() -> Self {
        Self {
            overlay_picker_open: false,
            overlay_selection: None,
        }
    }
}

const VIEW_PERFORMANCE_SCENE_WIDTH: f32 = 110.0;
const VIEW_PERFORMANCE_SCENE_HEIGHT: f32 = 82.0;

impl BlaulichtApp {
    fn draw_circle(ui: &mut egui::Ui, flag: bool) {
        // Decide color
        let color = if flag { Color32::GREEN } else { Color32::RED };

        let rect = ui.min_rect(); // content rect
        let painter = ui.painter();

        // Decide circle size
        let radius = 4.0;

        // Choose which corner: top-right
        let pos = Pos2::new(rect.right() - radius - 2.0, rect.top() + radius + 2.0);

        // Draw circle
        painter.circle_filled(pos, radius, color);
    }

    fn render_scene_performance(&mut self, ui: &mut egui::Ui, scene: (u8, &Scene), is_base: bool) {
        egui::Frame::NONE
            .fill(ui.visuals().widgets.active.bg_fill)
            .inner_margin(Margin::symmetric(5, 5))
            .show(ui, |ui| {
                ui.set_width(VIEW_PERFORMANCE_SCENE_WIDTH);
                ui.set_height(VIEW_PERFORMANCE_SCENE_HEIGHT);

                ui.vertical_centered(|ui| {
                    ui.set_width(ui.available_width());

                    let label = format!("{} ({})", &scene.1.name, scene.0);
                    ui.label(label);

                    let master_alpha = scene.1.sink.master_alpha_fader;
                    Self::draw_circle(ui, master_alpha > 0);

                    ui.add_space(5.0);

                    ui.columns(3, |columns| {
                        columns[0].vertical_centered(|ui| {
                            ui.add_space(5.0);
                            let mut master_alpha = scene.1.sink.master_alpha_fader as f32;
                            if ui.add(Knob::new(&mut master_alpha, 0.0..=100.0)).changed() {
                                self.data
                                    .event_bus_connection
                                    .send(ControlEventMessage::new(
                                        EventOriginator::Web,
                                        ControlEvent::SetSceneMasterAlpha(
                                            scene.0,
                                            master_alpha as u8,
                                        ),
                                    ));
                            }
                        });

                        columns[1].vertical_centered(|ui| {
                            let rst_enabled = scene.1.sink.master_alpha_fader != 100
                                || scene.1.sink.master_speed != AnimationSpeedModifier::_1;

                            if components::button(
                                ui,
                                rst_enabled,
                                "RST",
                                ButtonSize::Small.with_width(22.0),
                            ) {
                                self.data
                                    .event_bus_connection
                                    .send(ControlEventMessage::new(
                                        EventOriginator::Web,
                                        ControlEvent::Transaction(vec![
                                            ControlEvent::SetSceneMasterSpeed(
                                                scene.0,
                                                AnimationSpeedModifier::_1,
                                            ),
                                            ControlEvent::SetSceneMasterAlpha(scene.0, 100),
                                        ]),
                                    ));
                            }

                            ui.add_space(5.0);

                            let is_off = scene.1.sink.master_alpha_fader == 0;

                            if components::button(
                                ui,
                                !is_off,
                                "OFF",
                                ButtonSize::Small.with_width(22.0),
                            ) {
                                self.data
                                    .event_bus_connection
                                    .send(ControlEventMessage::new(
                                        EventOriginator::Web,
                                        ControlEvent::SetSceneMasterAlpha(scene.0, 0),
                                    ));
                            }

                            ui.add_space(5.0);

                            if components::button(
                                ui,
                                !is_base,
                                "DEL",
                                ButtonSize::Small.with_width(22.0),
                            ) {
                                self.data
                                    .event_bus_connection
                                    .send(ControlEventMessage::new(
                                        EventOriginator::Web,
                                        ControlEvent::RemoveOverlayScene(scene.0),
                                    ));
                            }
                        });

                        columns[2].vertical_centered(|ui| {
                            ui.add_space(5.0);
                            let mut speed = scene.1.sink.master_speed;
                            if ui.add(SpeedKnob::new(&mut speed)).changed() {
                                self.data
                                    .event_bus_connection
                                    .send(ControlEventMessage::new(
                                        EventOriginator::Web,
                                        ControlEvent::SetSceneMasterSpeed(scene.0, speed),
                                    ));
                            }
                        });
                    });

                    // ui.horizontal_centered(|ui| {
                    //     ui.add_space(25.0);
                    // });
                });
            });
    }

    fn render_view_perf_overlay_picker_dialog(
        &mut self,
        ctx: &Context,
        available_scenes: &[(u8, String)],
    ) {
        if !self.view_perf_ui_state.overlay_picker_open {
            return;
        }

        if available_scenes.is_empty() {
            self.view_perf_ui_state.overlay_picker_open = false;
            self.view_perf_ui_state.overlay_selection = None;
            return;
        }

        let current_selection = self
            .view_perf_ui_state
            .overlay_selection
            .unwrap_or(available_scenes[0].0);

        let (selected_id, changed) = components::id_selection_dialog(
            ctx,
            available_scenes.to_vec(),
            current_selection,
            &mut self.view_perf_ui_state.overlay_picker_open,
            "Select Overlay Scene".to_string(),
        );

        self.view_perf_ui_state.overlay_selection = Some(selected_id);

        if changed {
            self.data
                .event_bus_connection
                .send(ControlEventMessage::new(
                    EventOriginator::Web,
                    ControlEvent::AddOverlayScene(selected_id),
                ));
        }

        if !self.view_perf_ui_state.overlay_picker_open {
            self.view_perf_ui_state.overlay_selection = None;
        }
    }

    pub fn view_perf_ui(&mut self, ui: &mut egui::Ui, ctx: &Context) {
        let dmx_engine = { self.data.state.dmx_engine.read().unwrap().clone() };
        let base_scene = dmx_engine.0.current_scene_focus;
        let current_overlays = dmx_engine.0.current_overlay_scenes.clone();
        let available_overlay_scenes: Vec<(u8, String)> = dmx_engine
            .0
            .scenes
            .iter()
            .filter_map(|(scene_id, scene)| {
                if *scene_id == base_scene {
                    return None;
                }

                if current_overlays.contains(scene_id) {
                    return None;
                }

                Some((*scene_id, format!("{} ({})", scene.name, scene_id)))
            })
            .collect();

        let groups = dmx_engine.groups();
        self.render_dmx_simulation_dialog(ctx, groups);

        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), ui.available_height()),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.horizontal(|ui| {
                    let can_add_overlay = !available_overlay_scenes.is_empty();
                    if components::button(
                        ui,
                        self.view_perf_ui_state.overlay_picker_open,
                        "ADD OVERLAY",
                        ButtonSize::Medium,
                    ) && can_add_overlay
                    {
                        self.view_perf_ui_state.overlay_picker_open = true;
                    }

                    if components::button(ui, false, "BAR BAZ", ButtonSize::Medium) {}

                    ui.label("TODO: allow for boosting the brightness");
                });

                ui.separator();

                // Boolean values indicate whether scene is base.
                let mut scenes = vec![(dmx_engine.0.current_scene_focus, true)];
                scenes.extend(
                    dmx_engine
                        .0
                        .current_overlay_scenes
                        .iter()
                        .map(|s| (*s, false))
                        .collect::<Vec<_>>(),
                );

                // 1. Define your card width (adjust to match your frame content)
                let padding = 10.0;
                let total_card_width = VIEW_PERFORMANCE_SCENE_WIDTH + padding;

                // 2. Calculate how many columns fit in the current window width
                //    We use a minimal check (max(1)) to prevent division by zero or 0 columns.
                let available_width = ui.available_width() - 200.0;
                let cols = (available_width / total_card_width).floor().max(1.0) as usize;

                ui.allocate_ui_with_layout(
                    egui::vec2(ui.available_width(), ui.available_height()),
                    egui::Layout::left_to_right(egui::Align::Min),
                    |ui| {
                        // 3. Use a Grid, which handles alignment perfectly
                        egui::Frame::NONE
                            // .fill(egui::Color32::GREEN) // Debug color
                            .inner_margin(0.0) // Set to 0.0 to see exact Grid boundaries
                            .show(ui, |ui| {
                                egui::Grid::new("scene_grid")
                                    .spacing(egui::vec2(padding, padding)) // Space between cards
                                    .show(ui, |ui| {
                                        ui.set_min_width(500.0);
                                        for (i, (id, is_base)) in scenes.iter().enumerate() {
                                            if i > 0 && i % cols == 0 {
                                                ui.end_row();
                                            }

                                            let scene_data = dmx_engine.get_scene(*id).unwrap();

                                            self.render_scene_performance(
                                                ui,
                                                (*id, scene_data),
                                                *is_base,
                                            );
                                        }
                                    });
                            });

                        ui.separator();

                        ui.label("THINGS ABOUT SCENES HERE");
                    },
                );
            },
        );

        self.render_view_perf_overlay_picker_dialog(ctx, &available_overlay_scenes);
    }
}
