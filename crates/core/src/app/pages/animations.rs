use crate::{
    app::{
        components::{self, button, ButtonSize, Dialog, HFader, Numberpad, Pagination},
        BlaulichtApp,
    },
    dmx::{animation::phaser, EngineState},
};
use blaulicht_shared::{
    AnimationSpec, AnimationSpecBody, AnimationSpecBodyBeat, AnimationSpecBodyKind,
    AnimationSpeedModifier, AnimationTemplate, FixtureProperty, FrequencyNormalization,
    MathematicalBaseFunction, PhaserDuration, PhaserKind, SyncMode,
};
use egui::{Color32, Context, FontId, Key, Label, RichText, TextEdit, Vec2};
use egui_plot::{GridMark, Line, Plot, PlotPoints};
use strum::IntoEnumIterator;

const ANIMATIONS_PER_PAGE: usize = 5;

pub struct AnimationUI {
    pub pagination: Pagination,

    pub selected_animation_id: Option<u8>,
    pub selected_animation_id_before: Option<u8>,

    pub create_open: bool,
    pub delete_confirm_open: bool,

    pub new_name: String,
    pub new_mode: AnimationSpecBodyKind,
    pub new_prop: FixtureProperty,
    pub new_mode_dialog_open: bool,
    pub new_prop_dialog_open: bool,

    pub edit_state: AnimationEditState,
}

impl Default for AnimationUI {
    fn default() -> Self {
        Self {
            pagination: Pagination::default().with_items_per_page(ANIMATIONS_PER_PAGE),
            selected_animation_id: None,
            selected_animation_id_before: None,
            create_open: false,
            delete_confirm_open: false,
            new_name: "Anim #".to_string(),
            new_mode: AnimationSpecBodyKind::Phaser,
            new_prop: FixtureProperty::Alpha,
            new_mode_dialog_open: false,
            new_prop_dialog_open: false,
            edit_state: AnimationEditState::default(),
        }
    }
}

impl AnimationUI {
    fn close_selection_dialogs(&mut self) {
        self.new_mode_dialog_open = false;
        self.new_prop_dialog_open = false;
    }
}

impl BlaulichtApp {
    fn render_delete_animation_dialog(&mut self, ctx: &Context, _ui: &mut egui::Ui) {
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

    fn render_add_animation_dialog(&mut self, ctx: &Context, _ui: &mut egui::Ui) {
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

                    let type_button_size = BUTTON_SIZE.with_width(140.0);
                    if components::button(
                        ui,
                        self.animation_ui_state.new_mode_dialog_open,
                        &self.animation_ui_state.new_mode.to_string(),
                        type_button_size,
                    ) {
                        self.animation_ui_state.new_mode_dialog_open = true;
                    }

                    let (new_mode, mode_changed) = components::selection_dialog(
                        ctx,
                        AnimationSpecBodyKind::iter(),
                        self.animation_ui_state.new_mode,
                        &mut self.animation_ui_state.new_mode_dialog_open,
                        "Select Animation Type".to_string(),
                    );

                    if mode_changed {
                        self.animation_ui_state.new_mode = new_mode;
                    }
                });

                ui.horizontal(|ui| {
                    ui.add_sized([LABEL_W, cell_h], Label::new("Prop:"));

                    let prop_button_size = BUTTON_SIZE.with_width(140.0);
                    if components::button(
                        ui,
                        self.animation_ui_state.new_prop_dialog_open,
                        &self.animation_ui_state.new_prop.to_string(),
                        prop_button_size,
                    ) {
                        self.animation_ui_state.new_prop_dialog_open = true;
                    }

                    let (new_prop, prop_changed) = components::selection_dialog(
                        ctx,
                        FixtureProperty::iter(),
                        self.animation_ui_state.new_prop,
                        &mut self.animation_ui_state.new_prop_dialog_open,
                        "Select Fixture Property".to_string(),
                    );

                    if prop_changed {
                        self.animation_ui_state.new_prop = new_prop;
                    }
                });

                ui.separator();

                ui.horizontal(|ui| {
                    if components::button(ui, false, "Cancel", ButtonSize::Medium) {
                        self.animation_ui_state.create_open = false;
                        self.animation_ui_state.close_selection_dialogs();
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

                        let len = dmx_engine.0.animation_templates.len();
                        dmx_engine.0.animation_templates.insert(
                            len as u8,
                            AnimationTemplate {
                                spec: AnimationSpec {
                                    name: base_name,
                                    body,
                                    property: self.animation_ui_state.new_prop,
                                },
                            },
                        );

                        self.animation_ui_state.create_open = false;
                        self.animation_ui_state.close_selection_dialogs();
                    }
                });
            });
        }
    }

    fn animation_pagination(&mut self, ui: &mut egui::Ui, dmx_engine: &EngineState) {
        // TODO: can we do this without the allocation?
        let raw_items: Vec<_> = dmx_engine.0.animation_templates.iter().collect();
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

                let label = format!("{id} | {}", animation.spec.name);

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

                                if !self.animation_ui_state.create_open {
                                    self.animation_ui_state.close_selection_dialogs();
                                }
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
                                        // Load new value.
                                        if self.animation_ui_state.selected_animation_id
                                            != self.animation_ui_state.selected_animation_id_before
                                        {
                                            let dmx_engine =
                                                self.data.state.dmx_engine.read().unwrap();
                                            let anim = dmx_engine
                                                .0
                                                .animation_templates
                                                .get(&id)
                                                .unwrap()
                                                .clone();
                                            self.animation_ui_state
                                                .edit_state
                                                .load_state(anim.spec);

                                            self.animation_ui_state.selected_animation_id_before =
                                                self.animation_ui_state.selected_animation_id;
                                        }

                                        if let Some(value_changed) =
                                            self.animation_ui_state.edit_state.show(ui, ctx)
                                        {
                                            let mut dmx_engine =
                                                self.data.state.dmx_engine.write().unwrap();

                                            let anim = dmx_engine
                                                .0
                                                .animation_templates
                                                .get_mut(&id)
                                                .unwrap();

                                            anim.spec = value_changed;
                                        }
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
}

pub struct AnimationEditState {
    pub working_state: AnimationSpec,
    pub math_base_fn_dialog_open: bool,
    pub sync_mode_dialog_open: bool,
    pub speed_numberpad: Numberpad,
    pub clamp_min_numberpad: Numberpad,
    pub clamp_max_numberpad: Numberpad,
    pub freq_gate_numberpad: Numberpad,
    pub freq_boost_numberpad: Numberpad,
    pub freq_min_numberpad: Numberpad,
    pub freq_max_numberpad: Numberpad,
    pub freq_normalization_dialog_open: bool,
}

impl Default for AnimationEditState {
    fn default() -> Self {
        Self {
            working_state: AnimationSpec {
                name: "FOO".to_string(),
                body: AnimationSpecBody::AudioBeat(AnimationSpecBodyBeat {}),
                property: FixtureProperty::Alpha,
            },
            math_base_fn_dialog_open: false,
            sync_mode_dialog_open: false,
            speed_numberpad: Numberpad::new()
                .dialog_title("speed-num")
                .range(0.0, 30000.0),
            clamp_min_numberpad: Numberpad::new()
                .dialog_title("clamp-min-num")
                .range(0.0, 360.0),
            clamp_max_numberpad: Numberpad::new()
                .dialog_title("clamp-max-num")
                .range(0.0, 360.0),
            freq_gate_numberpad: Numberpad::new()
                .dialog_title("freq-gate-num")
                .range(0.0, 255.0),
            freq_boost_numberpad: Numberpad::new()
                .dialog_title("freq-boost-num")
                .range(0.0, 255.0),
            freq_min_numberpad: Numberpad::new()
                .dialog_title("freq-min-num")
                .range(0.0, 20_000.0),
            freq_max_numberpad: Numberpad::new()
                .dialog_title("freq-max-num")
                .range(0.0, 20_000.0),
            freq_normalization_dialog_open: false,
        }
    }
}

impl AnimationEditState {
    pub fn load_state(&mut self, spec: AnimationSpec) {
        println!("UI load state");
        self.working_state = spec;
        self.sync_mode_dialog_open = false;
        self.math_base_fn_dialog_open = false;
        self.speed_numberpad.close();
        self.freq_gate_numberpad.close();
        self.freq_boost_numberpad.close();
        self.freq_min_numberpad.close();
        self.freq_max_numberpad.close();
        self.freq_normalization_dialog_open = false;
    }

    pub fn show(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) -> Option<AnimationSpec> {
        let mut apply_clicked = false;

        ui.vertical(|ui| {
            // PATCH: sync properties of the animation that was selected.

            match &self.working_state.body {
                AnimationSpecBody::Phaser(_phaser) => self.anim_phaser_ui(ui, ctx),
                AnimationSpecBody::AudioVolume(_audio) => self.anim_audio_ui(ui),
                AnimationSpecBody::BPMValue(_) => self.anim_bpm_ui(ui),
                AnimationSpecBody::AudioFrequencies(_freq) => self.anim_freq_ui(ui),
                AnimationSpecBody::AudioBeat(_beat) => self.anim_beat_ui(ui),
                AnimationSpecBody::BeatClock(_beat) => self.anim_beat_clock_ui(ui),
                AnimationSpecBody::Wasm(_animation_spec_body_wasm) => todo!(),
            }

            ui.vertical(|ui| {
                if components::button(ui, false, "Apply", ButtonSize::Medium) {
                    apply_clicked = true;
                }
            });
        });

        match apply_clicked {
            true => Some(self.working_state.clone()),
            false => None,
        }
    }

    pub fn anim_phaser_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let AnimationSpecBody::Phaser(ref mut phaser_mut) = &mut self.working_state.body else {
            return;
        };

        const RENDER_WIDTH: usize = 3;

        let plot_points = (0..(360) * RENDER_WIDTH)
            .map(|x| {
                let y = phaser::generate(phaser_mut, x as u64);
                [x as f64, y as f64]
            })
            .collect::<PlotPoints<'_>>();

        ui.vertical(|ui| {
            match &mut phaser_mut.kind {
                PhaserKind::Mathematical(mathematical_phaser) => {
                    ui.horizontal(|ui| {
                        ui.horizontal(|ui| {
                            const BUTTON_SIZE: ButtonSize = ButtonSize::Medium;
                            let cell_h = BUTTON_SIZE.dim().0.y;
                            const LABEL_W: f32 = 120.0;

                            // TODO: why is this whole dropdown not as large as a button?
                            ui.add_sized([LABEL_W, cell_h], Label::new("Sync:"));

                            if components::button(
                                ui,
                                false,
                                &format!("{:?}", phaser_mut.sync),
                                ButtonSize::Medium.with_width(140.0),
                            ) {
                                self.sync_mode_dialog_open = true;
                            }

                            let options = SyncMode::iter();

                            let (new_sync_mode, changed) = components::selection_dialog(
                                ctx,
                                options,
                                phaser_mut.sync,
                                &mut self.sync_mode_dialog_open,
                                "Select Sync Mode".to_string(),
                            );

                            if changed {
                                phaser_mut.sync = new_sync_mode;
                            }
                        });

                        if components::button(ui, false, "Toggle Timing", ButtonSize::Medium) {
                            phaser_mut.time_total = match phaser_mut.time_total {
                                PhaserDuration::Fixed(_) => {
                                    PhaserDuration::Beat(AnimationSpeedModifier::_1)
                                }
                                PhaserDuration::Beat(_) => PhaserDuration::Fixed(1000),
                            }
                        }

                        match phaser_mut.time_total {
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
                                    phaser_mut.pin_to_beat,
                                    "BEAT RST",
                                    ButtonSize::Medium,
                                ) {
                                    phaser_mut.pin_to_beat = !phaser_mut.pin_to_beat;
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

                                self.speed_numberpad.ui(ui, value);
                            }
                        };
                    });

                    ui.horizontal(|ui| {
                        if components::button(
                            ui,
                            false,
                            &format!("{:?}", mathematical_phaser.base),
                            ButtonSize::Medium,
                        ) {
                            self.math_base_fn_dialog_open = true;
                        }

                        let options = MathematicalBaseFunction::iter();

                        let (new_fn, changed) = components::selection_dialog(
                            ctx,
                            options,
                            mathematical_phaser.base,
                            &mut self.math_base_fn_dialog_open,
                            "Select Base FN".to_string(),
                        );

                        if changed {
                            mathematical_phaser.base = new_fn;
                        }

                        // egui::ComboBox::from_label("FN")
                        //     .selected_text()
                        //     .show_ui(ui, |ui| {
                        //         for func in MathematicalBaseFunction::iter() {
                        //             if components::button(
                        //                 ui,
                        //                 false,
                        //                 &func.to_string(),
                        //                 ButtonSize::Medium.with_width(140.0),
                        //             ) {
                        //                 mathematical_phaser.base = func;
                        //             }
                        //         }
                        //     });
                    });

                    ui.separator();

                    ui.horizontal(|ui| {
                        self.clamp_min_numberpad
                            .ui(ui, &mut mathematical_phaser.amplitude_min);
                        self.clamp_max_numberpad
                            .ui(ui, &mut mathematical_phaser.amplitude_max);
                    });
                }
                PhaserKind::Keyframed(_keyframed_phaser) => todo!(),
            }

            ui.vertical(|ui| {
                let line = Line::new("animation", plot_points);
                Plot::new("animation_plot")
                    .height(128.0)
                    .width(512.0)
                    .view_aspect(1.0)
                    .default_y_bounds(-1.0, 260.0)
                    .x_grid_spacer(|input| {
                        let mut marks = Vec::new();
                        let (min, max) = input.bounds;

                        // Every 90 degrees
                        let start_90 = (min / 90.0).floor() as i64;
                        let end_90 = (max / 90.0).ceil() as i64;

                        for i in start_90..=end_90 {
                            let value = i as f64 * 90.0;
                            let step_size = if i % 4 == 0 { 360.0 } else { 90.0 };

                            marks.push(GridMark { value, step_size });
                        }
                        marks
                    })
                    .show(ui, |plot_ui| plot_ui.line(line));
            });
        });
    }

    fn anim_audio_ui(&mut self, ui: &mut egui::Ui) {
        ui.label("[AUDIO]");
    }

    fn anim_bpm_ui(&mut self, ui: &mut egui::Ui) {
        ui.label("[AUDIO-BPM]");
    }

    fn anim_freq_ui(&mut self, ui: &mut egui::Ui) {
        let AnimationSpecBody::AudioFrequencies(ref mut body) = self.working_state.body else {
            return;
        };

        const BUTTON_SIZE: ButtonSize = ButtonSize::Medium;
        let cell_h = BUTTON_SIZE.dim().0.y;
        const LABEL_W: f32 = 120.0;

        ui.horizontal(|ui| {
            ui.add_sized([LABEL_W, cell_h], Label::new("Normalization:"));

            if components::button(
                ui,
                self.freq_normalization_dialog_open,
                &body.normalization.to_string(),
                BUTTON_SIZE.with_width(140.0),
            ) {
                self.freq_normalization_dialog_open = true;
            }

            let (new_norm, changed) = components::selection_dialog(
                ui.ctx(),
                FrequencyNormalization::iter(),
                body.normalization,
                &mut self.freq_normalization_dialog_open,
                "Select Normalization".to_string(),
            );

            if changed {
                body.normalization = new_norm;
            }
        });

        ui.add_space(12.0);

        ui.horizontal(|ui| {
            ui.add_sized([LABEL_W, cell_h], Label::new("Gate:"));
            self.freq_gate_numberpad.ui(ui, &mut body.gate);

            ui.add_space(32.0);

            ui.add_sized([LABEL_W, cell_h], Label::new("Boost:"));
            self.freq_boost_numberpad.ui(ui, &mut body.boost);
        });

        ui.add_space(12.0);

        ui.horizontal(|ui| {
            ui.add_sized([LABEL_W, cell_h], Label::new("Freq Min:"));
            self.freq_min_numberpad.ui(ui, &mut body.freq_min);

            ui.add_space(32.0);

            ui.add_sized([LABEL_W, cell_h], Label::new("Freq Max:"));
            self.freq_max_numberpad.ui(ui, &mut body.freq_max);
        });

        if body.freq_min > body.freq_max {
            std::mem::swap(&mut body.freq_min, &mut body.freq_max);
        }
    }

    fn anim_beat_ui(&mut self, ui: &mut egui::Ui) {
        ui.label("[AUDIO-BEAT]");
    }

    fn anim_beat_clock_ui(&mut self, ui: &mut egui::Ui) {
        ui.label("[BEAT-CLOCK]");
    }
}
