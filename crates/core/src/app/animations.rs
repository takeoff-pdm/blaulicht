use blaulicht_shared::{
    AnimationSpeedModifier, ControlEvent, ControlEventMessage, EventOriginator,
};
use egui_plot::{Line, Plot, PlotPoints};
use strum::IntoEnumIterator;

use crate::{
    app::BlaulichtApp,
    dmx::animation::{AnimationSpecBody, MathematicalBaseFunction, PhaserDuration, PhaserKind},
};

impl BlaulichtApp {
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
                                .range(0.0..=255.0)
                                .prefix("Value: ") // Prefix text
                                .suffix(" units"), // Suffix text
                        );
                        ui.add(
                            egui::DragValue::new(&mut self.animation_page.clamp_max)
                                .speed(0.1) // How fast dragging changes the value
                                // .clamp_range(0.0..=100.0) // Min/max range
                                .range(0.0..=255.0)
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
