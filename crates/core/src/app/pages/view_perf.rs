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
    // pagination: Pagination,
    // add_view_open: bool,
    // delete_view_open: bool,
    // selected_view_id: Option<u8>,
    // delete_view_id: Option<u8>,
    // new_view_name: String,
    // overlay_picker_open: bool,
    // base_picker_open: bool,
    // // For both overlay and base
    // scene_picker_view_id: Option<u8>,
}

impl Default for ViewPerfUI {
    fn default() -> Self {
        Self {
            // pagination: Pagination::default().with_items_per_page(VIEWS_PER_PAGE),
            // add_view_open: false,
            // delete_view_open: false,
            // selected_view_id: None,
            // delete_view_id: None,
            // new_view_name: DEFAULT_NEW_VIEW_NAME.to_string(),
            // overlay_picker_open: false,
            // base_picker_open: false,
            // scene_picker_view_id: None,
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

    pub fn view_perf_ui(&mut self, ui: &mut egui::Ui, ctx: &Context) {
        let dmx_engine = { self.data.state.dmx_engine.read().unwrap().clone() };

        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), ui.available_height()),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.horizontal(|ui| {
                    if components::button(ui, false, "FOOBAR", ButtonSize::Medium) {}

                    if components::button(ui, false, "BAR BAZ", ButtonSize::Medium) {}
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
    }
}
