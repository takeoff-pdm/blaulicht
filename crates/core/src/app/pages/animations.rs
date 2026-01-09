use crate::{
    app::{
        components::{self, button, ButtonSize, Dialog, HFader, Pagination},
        BlaulichtApp,
    },
    dmx::{animation::phaser, EngineState},
};
use blaulicht_shared::{
    AnimationSpec, AnimationSpecBody, AnimationSpecBodyAudioVolume, AnimationSpecBodyBeat,
    AnimationSpecBodyFrequencies, AnimationSpecBodyKind, AnimationSpecBodyPhaser,
    AnimationSpeedModifier, FixtureProperty, MathematicalBaseFunction, PhaserDuration, PhaserKind,
    SyncMode,
};
use egui::{Color32, Context, FontId, Key, Label, RichText, TextEdit, Vec2};
use egui_plot::{Line, Plot, PlotPoints};
use strum::IntoEnumIterator;

const ANIMATIONS_PER_PAGE: usize = 5;

pub struct AnimationUI {
    pub pagination: Pagination,
    pub selected_animation_id: Option<u8>,
    pub create_open: bool,
    pub delete_confirm_open: bool,

    pub new_name: String,
    pub new_mode: AnimationSpecBodyKind,
    pub new_prop: FixtureProperty,

    pub clamp_min: u16,
    pub clamp_max: u16,
    pub base_function: MathematicalBaseFunction,
    pub sync_mode: SyncMode,
    pub timing: PhaserDuration,
    pub timing_pin_to_beat: bool,
}

impl Default for AnimationUI {
    fn default() -> Self {
        Self {
            pagination: Pagination::default().with_items_per_page(ANIMATIONS_PER_PAGE),
            selected_animation_id: None,
            create_open: false,
            delete_confirm_open: false,
            new_name: "Anim #".to_string(),
            new_mode: AnimationSpecBodyKind::Phaser,
            new_prop: FixtureProperty::Alpha,
            clamp_min: 0,
            clamp_max: 255,
            base_function: MathematicalBaseFunction::Sin,
            sync_mode: SyncMode::Synced,
            timing: PhaserDuration::Fixed(1000),
            timing_pin_to_beat: false,
        }
    }
}

impl BlaulichtApp {
    fn render_delete_animation_dialog(&mut self, ctx: &Context, ui: &mut egui::Ui) {
        if !self.animation_ui_state.delete_confirm_open {
            return;
        }

        Dialog::new("Confirm Deletion".to_string(), egui::vec2(200.0, 100.0))
            .with_backdrop()
            .show(ctx, |ui| {
                ui.heading(RichText::new("Confirm Deletion").strong());

                ui.add_space(12.0);

                ui.horizontal(|ui| {
                    if components::button(ui, false, "Confirm", ButtonSize::Large) {
                        if let Some(id) = self.animation_ui_state.selected_animation_id {
                            let mut dmx_engine = self.data.state.dmx_engine.write().unwrap();
                            dmx_engine.delete_animation(id);
                            self.animation_ui_state.delete_confirm_open = false;
                        }
                    }

                    if components::button(ui, true, "Cancel", ButtonSize::Large) {
                        self.animation_ui_state.delete_confirm_open = false;
                    }
                });
            });
    }

    fn render_add_animation_dialog(&mut self, ctx: &Context, ui: &mut egui::Ui) {
        if self.animation_ui_state.create_open {
            const BUTTON_SIZE: ButtonSize = ButtonSize::Medium;
            let cell_h = BUTTON_SIZE.dim().0.y;
            const LABEL_W: f32 = 120.0;

            // TODO: replace with dimensions
            let dialog_width = 570.0;
            let dialog_height = 330.0;

            Dialog::new(
                "Add Animation".to_string(),
                egui::vec2(dialog_width, dialog_height),
            )
            .with_backdrop()
            .show(ctx, |ui| {
                ui.spacing_mut().interact_size = egui::vec2(44.0, 36.0);

                // Name
                ui.horizontal(|ui| {
                    ui.add_sized([LABEL_W, cell_h], Label::new("Name:"));

                    ui.add(
                        TextEdit::singleline(&mut self.animation_ui_state.new_name)
                            .font(FontId::proportional(BUTTON_SIZE.dim().1))
                            .min_size(Vec2::new(0.0, BUTTON_SIZE.dim().1)),
                    );
                });

                ui.separator();

                // Body selector
                ui.horizontal(|ui| {
                    ui.add_sized([LABEL_W, cell_h], Label::new("Type:"));

                    egui::ComboBox::from_id_salt("add_anim_kind_combo")
                        .width(140.0)
                        .selected_text(self.animation_ui_state.new_mode.to_string())
                        .show_ui(ui, |ui| {
                            for animation_spec in AnimationSpecBodyKind::iter() {
                                if components::button(
                                    ui,
                                    animation_spec == self.animation_ui_state.new_mode,
                                    &animation_spec.to_string(),
                                    ButtonSize::Medium.with_width(140.0),
                                ) {
                                    self.animation_ui_state.new_mode = animation_spec;
                                }
                            }
                        });
                });

                ui.horizontal(|ui| {
                    ui.add_sized([LABEL_W, cell_h], Label::new("Prop:"));

                    let kinds: Vec<_> = FixtureProperty::iter().collect();
                    egui::ComboBox::from_id_salt("add_anim_prop_combo")
                        .width(140.0)
                        .selected_text(format!("{}", self.animation_ui_state.new_prop))
                        .show_ui(ui, |ui| {
                            for prop in &kinds {
                                if components::button(
                                    ui,
                                    self.animation_ui_state.new_prop == *prop,
                                    &format!("{prop}"),
                                    ButtonSize::Medium.with_width(140.0), // TODO: uniform
                                                                          // width
                                ) {
                                    self.animation_ui_state.new_prop = *prop;
                                }
                            }
                        });
                });

                ui.separator();

                ui.horizontal(|ui| {
                    if components::button(ui, false, "Cancel", ButtonSize::Medium) {
                        self.animation_ui_state.create_open = false;
                    }

                    let mut button_pressed =
                        components::button(ui, true, "Create", ButtonSize::Medium);

                    ctx.input(|input| {
                        if input.key_pressed(Key::Enter) {
                            button_pressed = true;
                        }
                    });

                    if button_pressed {
                        let mut dmx_engine = self.data.state.dmx_engine.write().unwrap();

                        let base_name = std::mem::take(&mut self.animation_ui_state.new_name);
                        let body = AnimationSpecBody::from(self.animation_ui_state.new_mode);

                        let len = dmx_engine.0.animations.len();
                        dmx_engine.0.animations.insert(
                            len as u8,
                            AnimationSpec {
                                name: base_name,
                                body,
                                property: self.animation_ui_state.new_prop,
                                sync: SyncMode::Synced,
                            },
                        );

                        self.animation_ui_state.create_open = false;
                    }
                });
            });
        }
    }

    fn animation_pagination(&mut self, ui: &mut egui::Ui, dmx_engine: &EngineState) {
        // TODO: can we do this without the allocation?
        let raw_items: Vec<_> = dmx_engine.0.animations.iter().collect();
        let paginated_animations = self
            .animation_ui_state
            .pagination
            .prepare_current_page_items(&raw_items);

        let panel_padding = 2.0;

        let (mut changed, mut selected_id) = (false, None);
        let selected_animation_id = self.animation_ui_state.selected_animation_id;
        let page_width = self.animation_ui_state.pagination.width();

        self.animation_ui_state.pagination.ui(ui, |ui| {
            for (id, animation) in paginated_animations {
                let id = **id;

                let label = format!("{id} | {}", animation.name);
                let is_selected = selected_animation_id == Some(id);
                if components::button(
                    ui,
                    is_selected,
                    &label,
                    ButtonSize::Large
                        .with_width(page_width - 2.0 * panel_padding)
                        .with_font_size(12.0),
                ) {
                    // Toggle group selection
                    if !is_selected {
                        selected_id = Some(id);
                        changed = true;
                    };
                }

                ui.add_space(5.0);
            }
        });

        if changed {
            self.animation_ui_state.selected_animation_id = selected_id;
        }
    }

    pub fn animations_ui(&mut self, ctx: &Context, ui: &mut egui::Ui) {
        self.render_add_animation_dialog(ctx, ui);
        self.render_delete_animation_dialog(ctx, ui);

        let animations = {
            let dmx_engine = self.data.state.dmx_engine.read().unwrap().clone();
            dmx_engine.0.animations.clone()
        };

        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), ui.available_height()),
            egui::Layout::left_to_right(egui::Align::Min),
            |ui| {
                let dmx_engine = { self.data.state.dmx_engine.read().unwrap().clone() };
                self.animation_pagination(ui, &dmx_engine);

                ui.separator();

                ui.allocate_ui_with_layout(
                    egui::vec2(ui.available_width(), ui.available_height()),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.horizontal(|ui| {
                            if button(
                                ui,
                                self.animation_ui_state.create_open,
                                "Create",
                                ButtonSize::Medium,
                            ) {
                                self.animation_ui_state.create_open =
                                    !self.animation_ui_state.create_open;
                            }

                            if button(
                                ui,
                                self.animation_ui_state.create_open,
                                "Delete",
                                ButtonSize::Medium,
                            ) {
                                self.animation_ui_state.delete_confirm_open =
                                    !self.animation_ui_state.delete_confirm_open;
                            }
                        });

                        ui.separator();

                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.heading("Animation Details");
                                ui.add_space(4.0);

                                match self.animation_ui_state.selected_animation_id {
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
                                                AnimationSpecBody::AudioBeat(beat) => {
                                                    self.anim_beat_ui(beat, ui)
                                                }
                                                AnimationSpecBody::BeatClock(beat) => {
                                                    self.anim_beat_clock_ui(beat, ui)
                                                }
                                                AnimationSpecBody::Wasm(
                                                    animation_spec_body_wasm,
                                                ) => todo!(),
                                            }
                                        });
                                    }
                                    None => {
                                        ui.label("No Animation Selected");
                                    }
                                }
                            });
                        });
                    },
                );
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

            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.horizontal(|ui| {
                        const BUTTON_SIZE: ButtonSize = ButtonSize::Medium;
                        let cell_h = BUTTON_SIZE.dim().0.y;
                        const LABEL_W: f32 = 120.0;

                        // TODO: why is this whole dropdown not as large as a button?
                        ui.add_sized([LABEL_W, cell_h], Label::new("Sync:"));

                        egui::ComboBox::from_id_salt("anim_sync_mode")
                            .width(140.0)
                            .selected_text(format!("{:?}", self.animation_ui_state.sync_mode))
                            .show_ui(ui, |ui| {
                                for mode in SyncMode::iter() {
                                    if components::button(
                                        ui,
                                        false,
                                        &mode.to_string(),
                                        ButtonSize::Medium.with_width(140.0),
                                    ) {
                                        self.animation_ui_state.sync_mode = mode;
                                    }
                                }
                            });
                    });
                });

                ui.horizontal(|ui| {
                    egui::ComboBox::from_label("FN")
                        .selected_text(format!("{:?}", self.animation_ui_state.base_function))
                        .show_ui(ui, |ui| {
                            for func in MathematicalBaseFunction::iter() {
                                ui.selectable_value(
                                    &mut self.animation_ui_state.base_function,
                                    func,
                                    func.to_string(),
                                );
                            }
                        });

                    if components::button(ui, false, "Toggle Timing", ButtonSize::Medium) {
                        self.animation_ui_state.timing = match self.animation_ui_state.timing {
                            PhaserDuration::Fixed(_) => {
                                PhaserDuration::Beat(AnimationSpeedModifier::_1)
                            }
                            PhaserDuration::Beat(_) => PhaserDuration::Fixed(1000),
                        }
                    }

                    match self.animation_ui_state.timing {
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
                                self.animation_ui_state.timing_pin_to_beat,
                                "BEAT RST",
                                ButtonSize::Medium,
                            ) {
                                self.animation_ui_state.timing_pin_to_beat =
                                    !self.animation_ui_state.timing_pin_to_beat;
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
                        egui::DragValue::new(&mut self.animation_ui_state.clamp_min)
                            .speed(0.1) // How fast dragging changes the value
                            // .clamp_range(0.0..=100.0) // Min/max range
                            .range(0.0..=u16::MAX as f32)
                            .prefix("Value: ") // Prefix text
                            .suffix(" units"), // Suffix text
                    );
                    ui.add(
                        egui::DragValue::new(&mut self.animation_ui_state.clamp_max)
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

                    animation.sync = self.animation_ui_state.sync_mode;

                    match &mut animation.body {
                        AnimationSpecBody::Phaser(animation_spec_body_phaser) => {
                            match &mut animation_spec_body_phaser.kind {
                                PhaserKind::Mathematical(ref mut mathematical_phaser) => {
                                    mathematical_phaser.base =
                                        self.animation_ui_state.base_function;
                                    mathematical_phaser.amplitude_min =
                                        self.animation_ui_state.clamp_min;
                                    mathematical_phaser.amplitude_max =
                                        self.animation_ui_state.clamp_max;
                                    animation_spec_body_phaser.time_total =
                                        self.animation_ui_state.timing;
                                    animation_spec_body_phaser.pin_to_beat =
                                        self.animation_ui_state.timing_pin_to_beat;
                                }
                                PhaserKind::Keyframed(_) => {
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

    fn anim_audio_ui(&mut self, _audio: &AnimationSpecBodyAudioVolume, ui: &mut egui::Ui) {
        ui.label("[AUDIO]");
    }

    fn anim_freq_ui(&mut self, _beat: &AnimationSpecBodyFrequencies, ui: &mut egui::Ui) {
        ui.label("[AUDIO-FREQ]");
    }

    fn anim_beat_ui(&mut self, _beat: &AnimationSpecBodyBeat, ui: &mut egui::Ui) {
        ui.label("[AUDIO-BEAT]");
    }

    fn anim_beat_clock_ui(&mut self, _beat: &AnimationSpecBodyBeat, ui: &mut egui::Ui) {
        ui.label("[BEAT-CLOCK]");
    }
}
