use std::u16;

use blaulicht_shared::{
    AnimationSpeedModifier, ControlEvent, ControlEventMessage, EventOriginator,
};
use egui_plot::{Line, Plot, PlotPoints};
use strum::IntoEnumIterator;

use crate::{
    app::BlaulichtApp,
    dmx::{animation::{AnimationSpecBody, MathematicalBaseFunction, PhaserDuration, PhaserKind}, EngineState},
};

impl BlaulichtApp {
    fn animation_overview(&mut self, ui: &mut egui::Ui, ctx: &Context, dmx_engine: &EngineState) {
        let panel_width = 100.0;
        let panel_padding = 2.0;

        ui.allocate_ui_with_layout(
            egui::vec2(panel_width, ui.available_height()), // fixed width, max height
            egui::Layout::top_down(egui::Align::Center),
            |ui| {
                let number_of_items_total = dmx_engine.scenes.len();
                const ITEMS_PER_PAGE: usize = 7;
                let total_pages = number_of_items_total / ITEMS_PER_PAGE;

                ui.set_min_width(panel_width);

                ui.vertical_centered(|ui| {
                    let scene_panel_page_button_sizes =
                        ButtonSize::Medium.with_width(ButtonSize::Medium.dim().0.x / 2.0);

                    ui.horizontal(|ui| {
                        if components::button(ui, false, "◀", scene_panel_page_button_sizes)
                            && self.scene_page_index > 0
                        {
                            self.scene_page_index -= 1;
                        }

                        if components::button(ui, false, "▶", scene_panel_page_button_sizes)
                            && self.scene_page_index < total_pages
                        {
                            self.scene_page_index += 1;
                        }
                    });

                    ui.add_space(5.0);

                    ui.label(format!("Page {} / {total_pages}", self.scene_page_index));
                    ui.label(format!("Scenes: {number_of_items_total}"));
                });

                ui.add_space(5.0);

                let start = self.scene_page_index * ITEMS_PER_PAGE;
                let page_items = dmx_engine.scenes.iter().skip(start).take(ITEMS_PER_PAGE);

                for (scene_id, scene) in page_items {
                    let is_selected = dmx_engine.current_scene_focus == *scene_id;

                    let label = format!("{scene_id} | {}", scene.name);
                    if components::button(
                        ui,
                        is_selected,
                        &label,
                        ButtonSize::Large
                            .with_width(panel_width - 2.0 * panel_padding)
                            .with_font_size(12.0),
                    ) {
                        // Toggle group selection
                        if !is_selected {
                            self.data
                                .event_bus_connection
                                .send(ControlEventMessage::new(
                                    EventOriginator::Web,
                                    ControlEvent::SetSceneFocus(*scene_id),
                                ));
                        };
                    }

                    // let rect =
                    //     ui.allocate_exact_size(egui::vec2(180.0, 60.0), egui::Sense::click());
                    // let painter = ui.painter();
                    // let bg_color = if is_selected {
                    //     egui::Color32::from_rgb(60, 120, 200)
                    // } else {
                    //     egui::Color32::from_gray(40)
                    // };
                    // painter.rect_filled(rect.0, 6.0, bg_color);
                    //
                    // // let fixture_count = group.fixtures.len();
                    // painter.text(
                    //     rect.0.left_top() + egui::vec2(12.0, 8.0),
                    //     egui::Align2::LEFT_TOP,
                    //     &name,
                    //     egui::FontId::proportional(12.0),
                    //     egui::Color32::WHITE,
                    // );
                    //
                    // painter.text(
                    //     rect.0.left_center() - egui::vec2(-12.0, 8.0),
                    //     egui::Align2::LEFT_CENTER,
                    //     format!("TODO: overlay or not"),
                    //     egui::FontId::proportional(12.0),
                    //     egui::Color32::GRAY,
                    // );
                    // painter.text(
                    //     rect.0.left_bottom() - egui::vec2(-12.0, 8.0),
                    //     egui::Align2::LEFT_BOTTOM,
                    //     format!("Changes: {}", scene.sink.changeset.len()),
                    //     egui::FontId::proportional(12.0),
                    //     egui::Color32::GRAY,
                    // );

                    // if rect.1.clicked() {}
                    ui.add_space(5.0);
                }
            },
        );
    }

    pub fn animations_ui(&mut self, ui: &mut egui::Ui) {
        ui.heading("Animations");
        ui.separator();

        // let groups = dmx_engine.groups();
        // let dmx_engine = self.data.state.dmx_engine.read().unwrap();

        let animations = {
            let dmx_engine = self.data.state.dmx_engine.read().unwrap().clone();
            dmx_engine.animations.clone()
        };

        ui.horizontal(|ui| {
            // ui.horizontal(|ui| {
            // Left: groups list
            ui.vertical(|ui| {
                ui.label("Groups:");
                ui.add_space(8.0);
                for (animation_id, animation) in animations.iter() {
                    let is_selected = self.animation_page.selected_animation == Some(*animation_id);

                    // if is_selected {
                    //     selected_groups.push(group_id);
                    // }

                    let rect =
                        ui.allocate_exact_size(egui::vec2(180.0, 48.0), egui::Sense::click());
                    let painter = ui.painter();
                    let bg_color = if is_selected {
                        egui::Color32::from_rgb(60, 120, 200)
                    } else {
                        egui::Color32::from_gray(40)
                    };
                    painter.rect_filled(rect.0, 6.0, bg_color);
                    let name = format!("[{}]: {}", animation_id, animation.name);
                    painter.text(
                        rect.0.left_top() + egui::vec2(12.0, 8.0),
                        egui::Align2::LEFT_TOP,
                        &name,
                        egui::FontId::proportional(16.0),
                        egui::Color32::WHITE,
                    );

                    if rect.1.clicked() {
                        // Toggle group selection
                        if !is_selected {
                            self.animation_page.selected_animation = Some(*animation_id);
                        };
                    }
                    ui.add_space(8.0);
                }
                // self.selected_fixture_group = selected_group;
            });
        });

        match self.animation_page.selected_animation {
            Some(id) => {
                let animation = animations.get(&id).unwrap();

                const RENDER_WIDTH: usize = 3;

                let plot_points = match &animation.body {
                    AnimationSpecBody::Phaser(animation_spec_body_phaser) => (0..(360)
                        * RENDER_WIDTH)
                        .map(|x| {
                            let y = animation_spec_body_phaser.generate(x as f32);
                            [x as f64, y as f64]
                        })
                        .collect::<PlotPoints<'_>>(),
                    AnimationSpecBody::AudioVolume(animation_spec_body_audio_volume) => todo!(),
                    AnimationSpecBody::Beat(animation_spec_body_beat) => todo!(),
                    AnimationSpecBody::Wasm(animation_spec_body_wasm) => todo!(),
                };

                ui.vertical(|ui| {
                    ui.label(format!("Animation: {}", animation.name));

                    ui.vertical(|ui| {
                        let line = Line::new("animation", plot_points);
                        Plot::new("animation_plot")
                            .height(128.0)
                            .width(512.0)
                            .view_aspect(1.0)
                            .default_y_bounds(-1.0, 260.0)
                            .show(ui, |plot_ui| plot_ui.line(line));
                    });
                    // ui.horizontal(|ui| {

                    ui.horizontal(|ui| {
                        ui.label("Write something: ");

                        egui::ComboBox::from_label("Audio Device")
                            .selected_text(format!("{:?}", self.animation_page.base_function))
                            .show_ui(ui, |ui| {
                                for func in MathematicalBaseFunction::iter() {
                                    ui.selectable_value(
                                        &mut self.animation_page.base_function,
                                        func,
                                        func.to_string(),
                                    );
                                }
                            });

                        ui.add(
                            egui::DragValue::new(&mut self.animation_page.clamp_min)
                                .speed(0.1) // How fast dragging changes the value
                                // .clamp_range(0.0..=100.0) // Min/max range
                                .range(0.0..=u16::MAX as f32)
                                .prefix("Value: ") // Prefix text
                                .suffix(" units"), // Suffix text
                        );
                        ui.add(
                            egui::DragValue::new(&mut self.animation_page.clamp_max)
                                .speed(0.1) // How fast dragging changes the value
                                // .clamp_range(0.0..=100.0) // Min/max range
                                .range(0.0..=u16::MAX as f32)
                                .prefix("Value: ") // Prefix text
                                .suffix(" units"), // Suffix text
                        );

                        if ui.button("Toggle Timing").clicked() {
                            self.animation_page.timing = match self.animation_page.timing {
                                PhaserDuration::Fixed(_) => {
                                    PhaserDuration::Beat(AnimationSpeedModifier::_1)
                                }
                                PhaserDuration::Beat(_) => PhaserDuration::Fixed(1000),
                            }
                        }

                        match self.animation_page.timing {
                            PhaserDuration::Beat(ref mut value) => {
                                let mut index = value.as_index();
                                let max_index = AnimationSpeedModifier::ALL.len() - 1;

                                ui.label(format!("Beats: {}", value.as_str()));
                                if ui
                                    .add(egui::Slider::new(&mut index, 0..=max_index).text("Enum"))
                                    .changed()
                                {
                                    *value = AnimationSpeedModifier::from_index(index);
                                }
                            }
                            PhaserDuration::Fixed(ref mut value) => {
                                ui.add(
                                    egui::DragValue::new(value)
                                        .speed(1) // How fast dragging changes the value
                                        // .clamp_range(0.0..=100.0) // Min/max range
                                        .range(0..=10000)
                                        .prefix("Speed: ") // Prefix text
                                        .suffix(" millis"), // Suffix text
                                );
                            }
                        };

                        if ui.button("Apply").clicked() {
                            let mut engine = self.data.state.dmx_engine.write().unwrap();
                            // TODO: use a message bus instead.
                            let animation = engine.animations.get_mut(&id).unwrap();
                            match &mut animation.body {
                                AnimationSpecBody::Phaser(animation_spec_body_phaser) => {
                                    match &mut animation_spec_body_phaser.kind {
                                        PhaserKind::Mathematical(ref mut mathematical_phaser) => {
                                            mathematical_phaser.base =
                                                self.animation_page.base_function;
                                            mathematical_phaser.amplitude_min =
                                                self.animation_page.clamp_min;
                                            mathematical_phaser.amplitude_max =
                                                self.animation_page.clamp_max;
                                            animation_spec_body_phaser.time_total =
                                                self.animation_page.timing;
                                        }
                                        PhaserKind::Keyframed(keyframed_phaser) => todo!(),
                                    }
                                }
                                _ => todo!(),
                            };
                        }
                    });
                });
            }
            None => {
                ui.label("No Animation Selected");
            }
        }
    }
}
