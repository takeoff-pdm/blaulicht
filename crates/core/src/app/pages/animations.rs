use crate::{
    app::{
        components::{self, button, ButtonColor, ButtonSize, HFader},
        BlaulichtApp,
    },
    dmx::{
        animation::{generate, phaser},
        EngineState,
    },
};
use blaulicht_shared::{
    AnimationSpec, AnimationSpecBody, AnimationSpecBodyAudioVolume, AnimationSpecBodyBeat,
    AnimationSpecBodyFrequencies, AnimationSpecBodyPhaser, AnimationSpeedModifier, ControlEvent,
    ControlEventMessage, EventOriginator, FixtureProperty, MathematicalBaseFunction,
    MathematicalPhaser, PhaserDuration, PhaserKind, SyncMode,
};
use egui::{Color32, Context, FontId, Key, Label, RichText, TextEdit, Vec2};
use egui_plot::{Line, Plot, PlotPoints};
use std::{time::Duration, u16};
use strum::IntoEnumIterator;

impl BlaulichtApp {
    fn animation_overview(&mut self, ui: &mut egui::Ui, ctx: &Context, dmx_engine: &EngineState) {
        let panel_width = 100.0;
        let panel_padding = 2.0;

        ui.allocate_ui_with_layout(
            egui::vec2(panel_width, ui.available_height()), // fixed width, max height
            egui::Layout::top_down(egui::Align::Center),
            |ui| {
                let number_of_items_total = dmx_engine.0.scenes.len();
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
                let page_items = dmx_engine.0.scenes.iter().skip(start).take(ITEMS_PER_PAGE);

                for (scene_id, scene) in page_items {
                    let is_selected = dmx_engine.0.current_scene_focus == *scene_id;

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

    pub fn animations_ui(&mut self, ctx: &Context, ui: &mut egui::Ui) {
        ui.heading("Animations");

        if self.animation_page.create_open {
            const BUTTON_SIZE: ButtonSize = ButtonSize::Medium;
            let cell_h = BUTTON_SIZE.dim().0.y;
            let width = 570.0;
            let height = 330.0;
            const LABEL_W: f32 = 120.0;

            components::dialog(
                ctx,
                "Add Animation",
                egui::vec2(width, height),
                false,
                |ui| {
                    ui.spacing_mut().interact_size = egui::vec2(44.0, 36.0);

                    // Name
                    ui.horizontal(|ui| {
                        ui.add_sized([LABEL_W, cell_h], Label::new("Name:"));

                        ui.add(
                            TextEdit::singleline(&mut self.animation_page.new_name)
                                .font(FontId::proportional(BUTTON_SIZE.dim().1))
                                .min_size(Vec2::new(0.0, BUTTON_SIZE.dim().1)),
                        );
                    });

                    ui.separator();

                    // Body selector
                    ui.horizontal(|ui| {
                        ui.add_sized([LABEL_W, cell_h], Label::new("Type:"));

                        let kinds = ["PhaserMath", "AudioVolume", "AudioBeat", "BeatClock"]; // TODO: replace with enum iter.
                        egui::ComboBox::from_id_source("add_anim_kind_combo")
                            .width(140.0)
                            .selected_text(self.animation_page.new_mode)
                            .show_ui(ui, |ui| {
                                for (idx, label) in kinds.iter().enumerate() {
                                    ui.selectable_value(
                                        &mut self.animation_page.new_mode,
                                        label,
                                        *label,
                                    );
                                }
                            });

                        ///////
                        // match *label {
                        // };
                    });

                    ui.horizontal(|ui| {
                        ui.add_sized([LABEL_W, cell_h], Label::new("Prop:"));

                        // let kinds = ["PhaserMath", "AudioVolume", "AudioBeat"];
                        let kinds: Vec<_> = FixtureProperty::iter().collect();
                        egui::ComboBox::from_id_source("add_anim_prop_combo")
                            .width(140.0)
                            .selected_text(format!("{}", self.animation_page.new_prop))
                            .show_ui(ui, |ui| {
                                for (idx, prop) in kinds.iter().enumerate() {
                                    ui.selectable_value(
                                        &mut self.animation_page.new_prop,
                                        *prop,
                                        format!("{prop}"),
                                    );
                                }
                            });

                        ///////
                        // match *label {
                        // };
                    });

                    // Model selector depending on kind
                    // ui.horizontal(|ui| {});

                    ui.separator();

                    ui.separator();

                    ui.horizontal(|ui| {
                        if components::button(ui, false, "Cancel", ButtonSize::Medium) {
                            self.add_fixture_open = false;
                        }

                        // let can_create = self.add_fixture_group.is_some()
                        //     && dmx_engine
                        //         .groups()
                        //         .get(&self.add_fixture_group.unwrap())
                        //         .is_some()
                        //     && self.add_fixture_start_addr >= 1
                        //     && self.add_fixture_start_addr <= 512;

                        let mut button_pressed =
                            components::button(ui, true, "Create", ButtonSize::Medium);
                        ctx.input(|input| {
                            if input.key_pressed(Key::Enter) {
                                button_pressed = true;
                            }
                        });

                        // if !can_create {
                        //     button_pressed = false;
                        // }

                        if button_pressed {
                            let base_name = std::mem::take(&mut self.animation_page.new_name);

                            let mut dmx_engine = self.data.state.dmx_engine.write().unwrap();

                            // Build anim type
                            let body = match self.animation_page.new_mode {
                                "AudioVolume" => {
                                    AnimationSpecBody::AudioVolume(AnimationSpecBodyAudioVolume {})
                                }
                                "AudioBeat" => {
                                    AnimationSpecBody::AudioBeat(AnimationSpecBodyBeat {})
                                }
                                "BeatClock" => {
                                    AnimationSpecBody::BeatClock(AnimationSpecBodyBeat {})
                                }
                                "PhaserMath" | _ => {
                                    AnimationSpecBody::Phaser(AnimationSpecBodyPhaser {
                                        kind: PhaserKind::Mathematical(MathematicalPhaser {
                                            base: MathematicalBaseFunction::Sin,
                                            stretch_factor: 1.0,
                                            amplitude_min: 0,
                                            amplitude_max: 255,
                                        }),
                                        pin_to_beat: false,
                                        time_total: PhaserDuration::Fixed(1000),
                                    })
                                }
                            };

                            // let prop = match self.animation_page.new_mode {
                            //     "AudioVolume" => {
                            //         AnimationSpecBody::AudioVolume(AnimationSpecBodyAudioVolume {})
                            //     }
                            //     "AudioBeat" => AnimationSpecBody::Beat(AnimationSpecBodyBeat {}),
                            //     "PhaserMath" | _ => {
                            //         AnimationSpecBody::Phaser(AnimationSpecBodyPhaser {
                            //             kind: PhaserKind::Mathematical(MathematicalPhaser {
                            //                 base: MathematicalBaseFunction::Sin,
                            //                 stretch_factor: 1.0,
                            //                 amplitude_min: 0,
                            //                 amplitude_max: 255,
                            //             }),
                            //             time_total: PhaserDuration::Fixed(1000),
                            //         })
                            //     }
                            // };

                            let len = dmx_engine.0.animations.len();
                            dmx_engine.0.animations.insert(
                                len as u8,
                                AnimationSpec {
                                    name: base_name,
                                    body,
                                    property: self.animation_page.new_prop,
                                    sync: SyncMode::Synced,
                                },
                            );

                            self.animation_page.create_open = false;
                        }
                    });
                },
            );
        }

        if button(
            ui,
            self.animation_page.create_open,
            "Create",
            ButtonSize::Medium,
        ) {
            self.animation_page.create_open = !self.animation_page.create_open;
        }

        ui.separator();

        // let groups = dmx_engine.groups();
        // let dmx_engine = self.data.state.dmx_engine.read().unwrap();

        let animations = {
            let dmx_engine = self.data.state.dmx_engine.read().unwrap().clone();
            dmx_engine.0.animations.clone()
        };

        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), ui.available_height()), // fixed width, max height
            egui::Layout::left_to_right(egui::Align::Min),
            |ui| {
                ui.set_min_height(ui.available_height());
                // ui.horizontal(|ui| {
                // Left: groups list
                ui.set_min_height(ui.available_height());
                egui::ScrollArea::vertical()
                    .auto_shrink([true, false])
                    .stick_to_bottom(false)
                    .show(ui, |ui| {
                        ui.vertical(|ui| {
                            for (animation_id, animation) in animations.iter() {
                                let is_selected =
                                    self.animation_page.selected_animation == Some(*animation_id);

                                // if is_selected {
                                //     selected_groups.push(group_id);
                                // }

                                let clicked = components::clickable(
                                    ui,
                                    is_selected,
                                    ButtonColor::Blue.into(),
                                    ButtonSize::Large.with_width(150.0),
                                    |ui, rect, clr| {
                                        let painter = ui.painter();
                                        let mut name = animation.name.clone();
                                        name.truncate(20);

                                        painter.text(
                                            rect.left_top() + egui::vec2(8.0, 15.0),
                                            egui::Align2::LEFT_TOP,
                                            name,
                                            egui::FontId::proportional(14.0),
                                            egui::Color32::WHITE,
                                        );

                                        painter.text(
                                            rect.left_bottom() + egui::vec2(8.0, -30.0),
                                            egui::Align2::LEFT_TOP,
                                            format!("#{animation_id}"),
                                            egui::FontId::proportional(10.0),
                                            egui::Color32::WHITE,
                                        );
                                    },
                                );

                                if clicked {
                                    // Toggle group selection
                                    if !is_selected {
                                        self.animation_page.selected_animation =
                                            Some(*animation_id);
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

                        ui.vertical(|ui| {
                            ui.label(format!("Animation: {}", animation.name));

                            // PATCH: sync properties of the animation that was selected.

                            match &animation.body {
                                AnimationSpecBody::Phaser(phaser) => {
                                    self.anim_phaser_ui(id, phaser, ui)
                                }
                                AnimationSpecBody::AudioVolume(audio) => {
                                    self.anim_audio_ui(audio, ui)
                                }
                                AnimationSpecBody::AudioFrequencies(freq) => {
                                    self.anim_freq_ui(freq, ui)
                                }
                                AnimationSpecBody::AudioBeat(beat) => self.anim_beat_ui(beat, ui),
                                AnimationSpecBody::BeatClock(beat) => {
                                    self.anim_beat_clock_ui(beat, ui)
                                }
                                AnimationSpecBody::Wasm(animation_spec_body_wasm) => todo!(),
                            }
                        });
                    }
                    None => {
                        ui.label("No Animation Selected");
                    }
                }
            },
        );
    }

    fn anim_phaser_ui(
        &mut self,
        animation_id: u8,
        phaser: &AnimationSpecBodyPhaser,
        ui: &mut egui::Ui,
    ) {
        const RENDER_WIDTH: usize = 3;

        let plot_points = (0..(360) * RENDER_WIDTH)
            .map(|x| {
                let y = phaser::generate(phaser, x as f32);
                [x as f64, y as f64]
            })
            .collect::<PlotPoints<'_>>();

        ui.vertical(|ui| {
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

            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    egui::ComboBox::from_label("SYNC")
                        .selected_text(format!("{:?}", self.animation_page.sync_mode))
                        .show_ui(ui, |ui| {
                            for mode in SyncMode::iter() {
                                ui.selectable_value(
                                    &mut self.animation_page.sync_mode,
                                    mode,
                                    mode.to_string(),
                                );
                            }
                        });
                });

                ui.horizontal(|ui| {
                    egui::ComboBox::from_label("FN")
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

                    if components::button(ui, false, "Toggle Timing", ButtonSize::Medium) {
                        self.animation_page.timing = match self.animation_page.timing {
                            PhaserDuration::Fixed(_) => {
                                PhaserDuration::Beat(AnimationSpeedModifier::_1)
                            }
                            PhaserDuration::Beat(_) => PhaserDuration::Fixed(1000),
                        }
                    }

                    match self.animation_page.timing {
                        PhaserDuration::Beat(ref mut value) => {
                            let mut index = value.as_index() as f32;
                            let max_index = AnimationSpeedModifier::ALL.len() - 1;

                            ui.add_sized(
                                [60.0, 16.0],
                                Label::new(
                                    RichText::new(format!("Beats: {}", value.as_str()))
                                        .color(Color32::LIGHT_RED)
                                        .size(16.0),
                                ),
                            );

                            if ui
                                .add(HFader::new(&mut index, 0.0..=(max_index as f32)))
                                .changed()
                            {
                                *value = AnimationSpeedModifier::from_index(index as usize);
                            }

                            if components::button(
                                ui,
                                self.animation_page.timing_pin_to_beat,
                                "BEAT RST",
                                ButtonSize::Medium,
                            ) {
                                self.animation_page.timing_pin_to_beat =
                                    !self.animation_page.timing_pin_to_beat;
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
                });

                ui.separator();

                ui.horizontal(|ui| {
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
                });

                if components::button(ui, false, "Apply", ButtonSize::Medium) {
                    let mut engine = self.data.state.dmx_engine.write().unwrap();
                    // TODO: use a message bus instead.
                    let animation = engine.0.animations.get_mut(&animation_id).unwrap();

                    animation.sync = self.animation_page.sync_mode;

                    match &mut animation.body {
                        AnimationSpecBody::Phaser(animation_spec_body_phaser) => {
                            match &mut animation_spec_body_phaser.kind {
                                PhaserKind::Mathematical(ref mut mathematical_phaser) => {
                                    mathematical_phaser.base = self.animation_page.base_function;
                                    mathematical_phaser.amplitude_min =
                                        self.animation_page.clamp_min;
                                    mathematical_phaser.amplitude_max =
                                        self.animation_page.clamp_max;
                                    animation_spec_body_phaser.time_total =
                                        self.animation_page.timing;
                                    animation_spec_body_phaser.pin_to_beat =
                                        self.animation_page.timing_pin_to_beat;
                                }
                                PhaserKind::Keyframed(keyframed_phaser) => {
                                    todo!()
                                }
                            }
                        }
                        _ => todo!(),
                    };
                }
            });
        });
    }

    fn anim_audio_ui(&mut self, audio: &AnimationSpecBodyAudioVolume, ui: &mut egui::Ui) {
        ui.label("[AUDIO]");
    }

    fn anim_freq_ui(&mut self, beat: &AnimationSpecBodyFrequencies, ui: &mut egui::Ui) {
        ui.label("[AUDIO-FREQ]");
    }

    fn anim_beat_ui(&mut self, beat: &AnimationSpecBodyBeat, ui: &mut egui::Ui) {
        ui.label("[AUDIO-BEAT]");
    }

    fn anim_beat_clock_ui(&mut self, beat: &AnimationSpecBodyBeat, ui: &mut egui::Ui) {
        ui.label("[BEAT-CLOCK]");
    }
}
