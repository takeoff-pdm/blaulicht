use crate::{
    app::{
        components::{
            self, button, palette_binding_button, ButtonSize, Dialog, HFader, Numberpad,
            Pagination, PaletteBindingAction, PaletteBindingState,
        },
        BlaulichtApp,
    },
    dmx::{
        animation::{
            audio::{
                self, AudioModulationRuntime, AudioModulationSample, MAX_FRAME_AGE_MS,
                MIN_BEAT_FALLBACK_CONFIDENCE,
            },
            phaser,
        },
        EngineState,
    },
};
use blaulicht_shared::{
    fixture::value::FixtureValue, palette::Palette, AnimationPresetCategory, AnimationSpec,
    AnimationSpecBody, AnimationSpecBodyKind, AnimationSpeedModifier, AnimationTemplate,
    AudioModulationBlend, AudioModulationSignal, AudioModulationSpec, FixtureProperty,
    FlashWindowLayout, FrequencyNormalization, MathematicalBaseFunction, PhaserDuration,
    PhaserKind, SyncMode, MAX_ADD_DEPTH, MAX_SCALE_PERCENT, MAX_SECTION_MULTIPLIER,
    MAX_SENSITIVITY, MAX_THRESHOLD,
};
use egui::{Color32, Context, FontId, Key, Label, RichText, TextEdit, Vec2};
use egui_plot::{GridMark, Line, Plot, PlotPoints};
use std::{
    collections::BTreeMap,
    sync::atomic::{AtomicU64, Ordering},
    time::Instant,
};
use strum::IntoEnumIterator;

const ANIMATIONS_PER_PAGE: usize = 5;
static NEXT_WASM_EDITOR_INSTANCE: AtomicU64 = AtomicU64::new(1_u64 << 63);

pub struct AnimationUI {
    pub pagination: Pagination,

    pub selected_animation_id: Option<u8>,
    pub selected_animation_id_before: Option<u8>,

    pub create_open: bool,
    pub presets_open: bool,
    /// `None` shows every preset category at once.
    pub preset_category: Option<AnimationPresetCategory>,
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
            presets_open: false,
            preset_category: None,
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
                            self.animation_ui_state.selected_animation_id = None;
                            self.animation_ui_state.selected_animation_id_before = None;
                            self.animation_ui_state.delete_confirm_open = false;
                        }
                    }

                    if components::button(ui, true, "Cancel", ButtonSize::Large) {
                        self.animation_ui_state.delete_confirm_open = false;
                    }
                });
            });
    }

    /// Inserts `spec` as a new animation template. Returns false when the
    /// template ID space is exhausted.
    fn insert_animation_template(&mut self, spec: AnimationSpec) -> bool {
        let mut dmx_engine = self.data.state.dmx_engine.write().unwrap();
        let Some(new_id) = (0..=u8::MAX)
            .find(|candidate| !dmx_engine.0.animation_templates.contains_key(candidate))
        else {
            drop(dmx_engine);
            self.show_popup(crate::app::PopupSpec::with_duration(
                std::time::Duration::from_secs(3),
                "Animation limit reached".to_string(),
            ));
            return false;
        };

        dmx_engine
            .0
            .animation_templates
            .insert(new_id, AnimationTemplate { spec });
        true
    }

    fn render_presets_dialog(&mut self, ctx: &Context, _ui: &mut egui::Ui) {
        if !self.animation_ui_state.presets_open {
            return;
        }

        // Presets are just starting points: they create ordinary single-layer
        // animations with no lasting preset identity.
        let mut chosen = None;

        let selected_category = self.animation_ui_state.preset_category;

        Dialog::new("Starter Presets".to_string(), egui::vec2(560.0, 520.0))
            .with_backdrop()
            .show(ctx, |ui| {
                ui.heading(RichText::new("Starter Presets").strong());
                ui.label(
                    RichText::new(
                        "Each preset creates a normal, fully editable animation template.",
                    )
                    .size(11.0)
                    .weak(),
                );
                ui.add_space(8.0);

                // Category filter: `None` shows everything, grouped.
                ui.horizontal_wrapped(|ui| {
                    if components::button(
                        ui,
                        selected_category.is_none(),
                        "All",
                        ButtonSize::Medium.with_width(90.0),
                    ) {
                        self.animation_ui_state.preset_category = None;
                    }
                    for category in AnimationPresetCategory::iter() {
                        if components::button(
                            ui,
                            selected_category == Some(category),
                            category.label(),
                            ButtonSize::Medium.with_width(110.0),
                        ) {
                            self.animation_ui_state.preset_category = Some(category);
                        }
                    }
                });

                ui.add_space(6.0);
                ui.separator();

                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .max_height(ui.available_height() - 56.0)
                    .show(ui, |ui| {
                        for category in AnimationPresetCategory::iter() {
                            if selected_category.is_some_and(|filter| filter != category) {
                                continue;
                            }

                            ui.add_space(6.0);
                            ui.label(RichText::new(category.label()).size(13.0).strong());
                            ui.label(RichText::new(category.description()).size(10.0).weak());
                            ui.add_space(4.0);

                            for preset in category.presets() {
                                if components::button(
                                    ui,
                                    false,
                                    &format!("{}   ·   {}", preset.label(), preset.property()),
                                    ButtonSize::Medium.with_width(490.0),
                                ) {
                                    chosen = Some(preset);
                                }
                                ui.label(RichText::new(preset.description()).size(11.0).weak());
                                ui.add_space(6.0);
                            }
                        }
                    });

                ui.separator();
                if components::button(ui, false, "Close", ButtonSize::Medium) {
                    self.animation_ui_state.presets_open = false;
                }
            });

        if let Some(preset) = chosen {
            if self.insert_animation_template(AnimationSpec::preset(preset)) {
                self.animation_ui_state.presets_open = false;
            }
        }
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
                        AnimationSpecBodyKind::selectable(),
                        self.animation_ui_state.new_mode,
                        &mut self.animation_ui_state.new_mode_dialog_open,
                        "Select Animation Type".to_string(),
                    );

                    if mode_changed {
                        self.animation_ui_state.new_mode = new_mode;
                    }
                });

                if self.animation_ui_state.new_mode != AnimationSpecBodyKind::Wasm {
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
                }

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
                        let base_name = std::mem::take(&mut self.animation_ui_state.new_name);
                        let spec = AnimationSpec {
                            name: base_name.clone(),
                            body: AnimationSpecBody::from(self.animation_ui_state.new_mode),
                            property: self.animation_ui_state.new_prop,
                        };

                        if self.insert_animation_template(spec) {
                            self.animation_ui_state.create_open = false;
                            self.animation_ui_state.close_selection_dialogs();
                        } else {
                            self.animation_ui_state.new_name = base_name;
                        }
                    }
                });
            });
        }
    }

    fn animation_pagination(&mut self, ui: &mut egui::Ui, dmx_engine: &EngineState, compact: bool) {
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

        self.animation_ui_state
            .pagination
            .ui_responsive(ui, compact, |ui| {
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

    /// Snapshot of the live audio used to drive the editor's readouts. The
    /// spectrogram's newest column is the same FFT data the engine modulates
    /// from, so the band readout matches what the layer actually sees.
    pub(crate) fn live_audio_for_preview(&self) -> blaulicht_audio_engine::CollectorOutput {
        let current_audio_colunn = self
            .data
            .state
            .audio_spectrogram
            .read()
            .ok()
            .and_then(|spectrogram| {
                spectrogram
                    .columns
                    .back()
                    .map(|column| column.current_audio_colunn.clone())
            })
            .unwrap_or_default();

        blaulicht_audio_engine::CollectorOutput {
            snapshot: self.collector_snapshot.clone(),
            debug_data: Default::default(),
            current_audio_colunn,
        }
    }

    pub fn animations_ui(
        &mut self,
        ctx: &Context,
        ui: &mut egui::Ui,
        render_context: crate::app::page::PageRenderContext,
    ) {
        self.render_add_animation_dialog(ctx, ui);
        self.render_presets_dialog(ctx, ui);
        self.render_delete_animation_dialog(ctx, ui);

        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), ui.available_height()),
            render_context.primary_layout(),
            |ui| {
                let dmx_engine = { self.data.state.dmx_engine.read().unwrap().clone() };
                self.animation_pagination(ui, &dmx_engine, render_context.is_narrow_dynamic());

                ui.separator();

                ui.allocate_ui_with_layout(
                    egui::vec2(ui.available_width(), ui.available_height()),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        render_context.horizontal(ui, egui::Align::Min, |ui| {
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
                                self.animation_ui_state.presets_open,
                                "Presets",
                                ButtonSize::Medium,
                            ) {
                                self.animation_ui_state.presets_open =
                                    !self.animation_ui_state.presets_open;
                            }

                            if button(
                                ui,
                                self.animation_ui_state.selected_animation_id.is_some(),
                                "Delete",
                                ButtonSize::Medium,
                            ) && self.animation_ui_state.selected_animation_id.is_some()
                            {
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
                                                .load_state(anim.spec.clone());
                                            if let AnimationSpecBody::WasmPlugin(wasm) =
                                                &anim.spec.body
                                            {
                                                let source = crate::plugin::tick::stable_animation_template_id(
                                                    &wasm.plugin_key,
                                                    id,
                                                );
                                                crate::plugin::wasm::clone_animation_instance_state(
                                                    &self.data.state,
                                                    &wasm.plugin_key,
                                                    source,
                                                    self.animation_ui_state
                                                        .edit_state
                                                        .wasm_editor_instance_id,
                                                );
                                            }

                                            self.animation_ui_state.selected_animation_id_before =
                                                self.animation_ui_state.selected_animation_id;
                                        }

                                        let palettes_snapshot = self
                                            .data
                                            .state
                                            .dmx_engine
                                            .read()
                                            .unwrap()
                                            .0
                                            .palettes
                                            .clone();

                                        let live_audio = self.live_audio_for_preview();

                                        if let Some(value_changed) = self
                                            .animation_ui_state
                                            .edit_state
                                            .show(ui, ctx, &palettes_snapshot, &live_audio, &self.data, &[])
                                        {
                                            if let AnimationSpecBody::WasmPlugin(wasm) =
                                                &value_changed.body
                                            {
                                                let target = crate::plugin::tick::stable_animation_template_id(
                                                    &wasm.plugin_key,
                                                    id,
                                                );
                                                crate::plugin::wasm::clone_animation_instance_state(
                                                    &self.data.state,
                                                    &wasm.plugin_key,
                                                    self.animation_ui_state
                                                        .edit_state
                                                        .wasm_editor_instance_id,
                                                    target,
                                                );
                                            }
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
    pub flash_off_numberpad: Numberpad,
    pub flash_on_numberpad: Numberpad,
    pub flash_window_numberpad: Numberpad,
    pub clamp_min_numberpad: Numberpad,
    pub clamp_max_numberpad: Numberpad,
    pub clamp_min_binding: PaletteBindingState,
    pub clamp_max_binding: PaletteBindingState,
    pub freq_gate_numberpad: Numberpad,
    pub freq_boost_numberpad: Numberpad,
    pub freq_min_numberpad: Numberpad,
    pub freq_max_numberpad: Numberpad,
    pub freq_normalization_dialog_open: bool,

    pub mod_freq_min_numberpad: Numberpad,
    pub mod_freq_max_numberpad: Numberpad,
    pub mod_depth_numberpad: Numberpad,
    pub mod_attack_numberpad: Numberpad,
    pub mod_release_numberpad: Numberpad,
    /// Drives the editor's live readouts with the same math the engine uses.
    mod_preview_runtime: AudioModulationRuntime,
    mod_preview_sample: Option<AudioModulationSample>,
    pub(crate) wasm_editor_instance_id: u64,
}

impl Default for AnimationEditState {
    fn default() -> Self {
        Self {
            working_state: AnimationSpec {
                name: "FOO".to_string(),
                body: AnimationSpecBody::AudioModulation(AudioModulationSpec::default()),
                property: FixtureProperty::Alpha,
            },
            math_base_fn_dialog_open: false,
            sync_mode_dialog_open: false,
            speed_numberpad: Numberpad::new()
                .dialog_title("speed-num")
                .range(0.0, 30000.0),
            flash_off_numberpad: Numberpad::new()
                .dialog_title("flash-off-num")
                .range(1.0, 60_000.0),
            flash_on_numberpad: Numberpad::new()
                .dialog_title("flash-on-num")
                .range(1.0, 60_000.0),
            flash_window_numberpad: Numberpad::new()
                .dialog_title("flash-window-num")
                .range(1.0, u16::MAX as f64),
            clamp_min_numberpad: Numberpad::new()
                .dialog_title("clamp-min-num")
                .range(0.0, 360.0),
            clamp_max_numberpad: Numberpad::new()
                .dialog_title("clamp-max-num")
                .range(0.0, 360.0),
            clamp_min_binding: PaletteBindingState::default(),
            clamp_max_binding: PaletteBindingState::default(),
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
            mod_freq_min_numberpad: Numberpad::new()
                .dialog_title("mod-freq-min-num")
                .range(0.0, 20_000.0),
            mod_freq_max_numberpad: Numberpad::new()
                .dialog_title("mod-freq-max-num")
                .range(0.0, 20_000.0),
            mod_depth_numberpad: Numberpad::new()
                .dialog_title("mod-depth-num")
                .range(-(MAX_ADD_DEPTH as f64), MAX_ADD_DEPTH as f64),
            mod_attack_numberpad: Numberpad::new()
                .dialog_title("mod-attack-num")
                .range(0.0, blaulicht_shared::MAX_SMOOTHING_MS as f64),
            mod_release_numberpad: Numberpad::new()
                .dialog_title("mod-release-num")
                .range(0.0, blaulicht_shared::MAX_SMOOTHING_MS as f64),
            mod_preview_runtime: AudioModulationRuntime::default(),
            mod_preview_sample: None,
            wasm_editor_instance_id: NEXT_WASM_EDITOR_INSTANCE.fetch_add(1, Ordering::Relaxed),
        }
    }
}

impl AnimationEditState {
    pub fn load_state(&mut self, spec: AnimationSpec) {
        tracing::debug!("UI load state");
        self.working_state = spec;
        self.sync_mode_dialog_open = false;
        self.math_base_fn_dialog_open = false;
        self.speed_numberpad.close();
        self.flash_off_numberpad.close();
        self.flash_on_numberpad.close();
        self.flash_window_numberpad.close();
        self.freq_gate_numberpad.close();
        self.freq_boost_numberpad.close();
        self.freq_min_numberpad.close();
        self.freq_max_numberpad.close();
        self.freq_normalization_dialog_open = false;
        self.mod_freq_min_numberpad.close();
        self.mod_freq_max_numberpad.close();
        self.mod_depth_numberpad.close();
        self.mod_attack_numberpad.close();
        self.mod_release_numberpad.close();
        self.mod_preview_runtime = AudioModulationRuntime::default();
        self.mod_preview_sample = None;
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        palettes: &BTreeMap<u8, Palette>,
        live_audio: &blaulicht_audio_engine::CollectorOutput,
        data: &crate::state::AppStateWrapper,
        wasm_fixtures: &[(u8, u8)],
    ) -> Option<AnimationSpec> {
        let mut apply_clicked = false;
        let mut upgrade_legacy_wasm = false;

        ui.vertical(|ui| {
            // PATCH: sync properties of the animation that was selected.

            match &self.working_state.body {
                AnimationSpecBody::Phaser(_phaser) => self.anim_phaser_ui(ui, ctx, palettes),
                AnimationSpecBody::FlashAnimation(_) => self.anim_flash_ui(ui, ctx, palettes),
                AnimationSpecBody::AudioModulation(_) => {
                    self.anim_audio_modulation_ui(ui, ctx, live_audio)
                }
                AnimationSpecBody::AudioVolume(_audio) => self.anim_audio_ui(ui),
                AnimationSpecBody::BPMValue(_) => self.anim_bpm_ui(ui),
                AnimationSpecBody::AudioFrequencies(_freq) => self.anim_freq_ui(ui),
                AnimationSpecBody::AudioBeat(_beat) => self.anim_beat_ui(ui),
                AnimationSpecBody::BeatClock(_beat) => self.anim_beat_clock_ui(ui),
                AnimationSpecBody::Wasm(_) => {
                    ui.label("Legacy unconfigured WASM animation.");
                    if ui.button("Configure plugin").clicked() {
                        upgrade_legacy_wasm = true;
                    }
                }
                AnimationSpecBody::WasmPlugin(_) => self.anim_wasm_ui(ui, data, wasm_fixtures),
            }

            ui.vertical(|ui| {
                if components::button(ui, false, "Apply", ButtonSize::Medium) {
                    apply_clicked = true;
                }
            });
        });

        if upgrade_legacy_wasm {
            self.working_state.body = AnimationSpecBody::WasmPlugin(Default::default());
        }

        match apply_clicked {
            true => Some(self.working_state.clone()),
            false => None,
        }
    }

    fn anim_wasm_ui(
        &mut self,
        ui: &mut egui::Ui,
        data: &crate::state::AppStateWrapper,
        fixtures: &[(u8, u8)],
    ) {
        let AnimationSpecBody::WasmPlugin(ref mut wasm) = self.working_state.body else {
            return;
        };
        let registered: Vec<(u8, String, String)> = data
            .state
            .plugin_runtime_kinds
            .read()
            .unwrap()
            .iter()
            .filter_map(|(id, kind)| match kind {
                crate::state::PluginRuntimeKind::Animation {
                    stable_key,
                    display_name,
                } => Some((*id, stable_key.clone(), display_name.clone())),
                _ => None,
            })
            .collect();

        egui::ComboBox::from_label("Animation plugin")
            .selected_text(
                registered
                    .iter()
                    .find(|(_, key, _)| key == &wasm.plugin_key)
                    .map(|(_, _, name)| name.as_str())
                    .unwrap_or(if wasm.plugin_key.is_empty() {
                        "Select plugin"
                    } else {
                        "Plugin unavailable"
                    }),
            )
            .show_ui(ui, |ui| {
                for (_, key, name) in &registered {
                    ui.selectable_value(&mut wasm.plugin_key, key.clone(), name);
                }
            });

        if let Some((plugin_id, _, _)) = registered
            .iter()
            .find(|(_, key, _)| key == &wasm.plugin_key)
        {
            data.state
                .wasm_animation_editor_requests
                .write()
                .unwrap()
                .insert(
                    self.wasm_editor_instance_id,
                    crate::state::WasmAnimationEditorRequest {
                        plugin_key: wasm.plugin_key.clone(),
                        instance_id: self.wasm_editor_instance_id,
                        fixtures: fixtures.to_vec(),
                        last_seen: Instant::now(),
                    },
                );
            let ops = data
                .state
                .wasm_animation_ui_ops
                .read()
                .unwrap()
                .get(&self.wasm_editor_instance_id)
                .cloned();
            if let Some(ops) = ops {
                ui.separator();
                let mut index = 0;
                crate::app::plugin_ui::render_plugin_ops(
                    ui,
                    &ops,
                    &mut index,
                    data,
                    *plugin_id,
                    Some(self.wasm_editor_instance_id),
                );
            } else {
                ui.label("The plugin has not produced editor UI yet.");
            }
        }
    }

    pub fn anim_phaser_ui(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        palettes: &BTreeMap<u8, Palette>,
    ) {
        let preview_property = self.working_state.property;
        let AnimationSpecBody::Phaser(ref mut phaser_mut) = &mut self.working_state.body else {
            return;
        };

        const RENDER_WIDTH: usize = 3;

        let plot_points = (0..(360) * RENDER_WIDTH)
            .map(|x| {
                let y = phaser::generate(phaser_mut, x as f64, preview_property, palettes);
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

                                // `as_float` is beats-per-cycle (duration
                                // multiplier); `as_str` labels the speed
                                // semantic and reads inverted here.
                                let beats = value.as_float();
                                let beats_label = if beats >= 1.0 {
                                    format!("{}", beats as u32)
                                } else {
                                    format!("1/{}", (1.0 / beats).round() as u32)
                                };
                                ui.add_sized(
                                    [60.0, 16.0],
                                    Label::new(
                                        RichText::new(format!("Beats: {beats_label}"))
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
                        let min_frozen = mathematical_phaser.amplitude_min.is_frozen();
                        let mut min_val = mathematical_phaser
                            .amplitude_min
                            .resolve(palettes, preview_property);
                        ui.add_enabled_ui(!min_frozen, |ui| {
                            if self.clamp_min_numberpad.ui(ui, &mut min_val).changed() {
                                mathematical_phaser.amplitude_min = FixtureValue::Literal(min_val);
                            }
                        });
                        match palette_binding_button(
                            ui,
                            ctx,
                            &mut self.clamp_min_binding,
                            mathematical_phaser.amplitude_min,
                            palettes,
                            "Min palette".to_string(),
                        ) {
                            PaletteBindingAction::Bind {
                                palette_id,
                                property,
                            } => {
                                mathematical_phaser.amplitude_min = FixtureValue::PalettePointer {
                                    palette_id,
                                    property,
                                };
                            }
                            PaletteBindingAction::Unbind => {
                                let v = mathematical_phaser
                                    .amplitude_min
                                    .resolve(palettes, preview_property);
                                mathematical_phaser.amplitude_min = FixtureValue::Literal(v);
                            }
                            PaletteBindingAction::None => {}
                        }

                        let max_frozen = mathematical_phaser.amplitude_max.is_frozen();
                        let mut max_val = mathematical_phaser
                            .amplitude_max
                            .resolve(palettes, preview_property);
                        ui.add_enabled_ui(!max_frozen, |ui| {
                            if self.clamp_max_numberpad.ui(ui, &mut max_val).changed() {
                                mathematical_phaser.amplitude_max = FixtureValue::Literal(max_val);
                            }
                        });
                        match palette_binding_button(
                            ui,
                            ctx,
                            &mut self.clamp_max_binding,
                            mathematical_phaser.amplitude_max,
                            palettes,
                            "Max palette".to_string(),
                        ) {
                            PaletteBindingAction::Bind {
                                palette_id,
                                property,
                            } => {
                                mathematical_phaser.amplitude_max = FixtureValue::PalettePointer {
                                    palette_id,
                                    property,
                                };
                            }
                            PaletteBindingAction::Unbind => {
                                let v = mathematical_phaser
                                    .amplitude_max
                                    .resolve(palettes, preview_property);
                                mathematical_phaser.amplitude_max = FixtureValue::Literal(v);
                            }
                            PaletteBindingAction::None => {}
                        }
                    });

                    ui.separator();

                    ui.horizontal(|ui| {
                        let mut enabled = phaser_mut.reverse_after_n_iterations.is_some();
                        if ui
                            .checkbox(&mut enabled, "Reverse after N iterations")
                            .changed()
                        {
                            phaser_mut.reverse_after_n_iterations =
                                if enabled { Some(1) } else { None };
                        }

                        if let Some(ref mut n) = phaser_mut.reverse_after_n_iterations {
                            let mut n_f32 = *n as f32;
                            ui.add(
                                egui::Slider::new(&mut n_f32, 1.0..=32.0)
                                    .step_by(1.0)
                                    .text("N"),
                            );
                            *n = n_f32 as u32;
                        }
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

    fn anim_flash_ui(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        palettes: &BTreeMap<u8, Palette>,
    ) {
        let preview_property = self.working_state.property;
        let AnimationSpecBody::FlashAnimation(ref mut flash) = self.working_state.body else {
            return;
        };

        ui.vertical(|ui| {
            ui.checkbox(&mut flash.beat_aligned, "Beat aligned");
            if flash.beat_aligned {
                ui.horizontal(|ui| {
                    let max_index = AnimationSpeedModifier::ALL.len() - 1;
                    ui.label(format!("Dark gap: {} beats", flash.off_time_beats.as_str()));
                    let mut off_index = flash.off_time_beats.as_index() as f32;
                    if ui
                        .add(HFader::new(&mut off_index, 0.0..=max_index as f32))
                        .changed()
                    {
                        flash.off_time_beats =
                            AnimationSpeedModifier::from_index(off_index as usize);
                    }

                    ui.add_space(16.0);
                    ui.label(format!("On time: {} beats", flash.on_time_beats.as_str()));
                    let mut on_index = flash.on_time_beats.as_index() as f32;
                    if ui
                        .add(HFader::new(&mut on_index, 0.0..=max_index as f32))
                        .changed()
                    {
                        flash.on_time_beats =
                            AnimationSpeedModifier::from_index(on_index as usize);
                    }
                });
            } else {
                ui.horizontal(|ui| {
                    ui.label("Dark gap:");
                    flash.off_time_ms = flash.off_time_ms.max(1);
                    self.flash_off_numberpad.ui(ui, &mut flash.off_time_ms);
                    ui.label("ms");

                    ui.add_space(16.0);
                    ui.label("On time:");
                    flash.on_time_ms = flash.on_time_ms.max(1);
                    self.flash_on_numberpad.ui(ui, &mut flash.on_time_ms);
                    ui.label("ms");
                });
            }

            ui.horizontal(|ui| {
                ui.label("Window size:");
                flash.window_size = flash.window_size.max(1);
                self.flash_window_numberpad.ui(ui, &mut flash.window_size);

                let mut spaced = flash.window_layout == FlashWindowLayout::Spaced;
                if ui.checkbox(&mut spaced, "Spaced windows").changed() {
                    flash.window_layout = if spaced {
                        FlashWindowLayout::Spaced
                    } else {
                        FlashWindowLayout::Contiguous
                    };
                }
                ui.checkbox(&mut flash.random_order, "Random window order");
            });

            ui.add_enabled_ui(!flash.random_order, |ui| {
                ui.horizontal(|ui| {
                    let mut enabled = flash.reverse_after_n_iterations.is_some();
                    if ui
                        .checkbox(&mut enabled, "Reverse after N iterations")
                        .changed()
                    {
                        flash.reverse_after_n_iterations = if enabled { Some(1) } else { None };
                    }
                    if let Some(ref mut n) = flash.reverse_after_n_iterations {
                        let mut n_f32 = (*n).max(1) as f32;
                        ui.add(
                            egui::Slider::new(&mut n_f32, 1.0..=32.0)
                                .step_by(1.0)
                                .text("N"),
                        );
                        *n = n_f32 as u32;
                    }
                });
            });

            ui.label(
                "Animation and scene speed affect only the dark gap; on-time stays fixed. Beat mode follows the beat/sub-beat grid.",
            );
            ui.separator();

            ui.horizontal(|ui| {
                let min_frozen = flash.amplitude_min.is_frozen();
                let mut min_val = flash.amplitude_min.resolve(palettes, preview_property);
                ui.add_enabled_ui(!min_frozen, |ui| {
                    if self.clamp_min_numberpad.ui(ui, &mut min_val).changed() {
                        flash.amplitude_min = FixtureValue::Literal(min_val);
                    }
                });
                match palette_binding_button(
                    ui,
                    ctx,
                    &mut self.clamp_min_binding,
                    flash.amplitude_min,
                    palettes,
                    "Min palette".to_string(),
                ) {
                    PaletteBindingAction::Bind {
                        palette_id,
                        property,
                    } => {
                        flash.amplitude_min = FixtureValue::PalettePointer {
                            palette_id,
                            property,
                        };
                    }
                    PaletteBindingAction::Unbind => {
                        flash.amplitude_min = FixtureValue::Literal(
                            flash.amplitude_min.resolve(palettes, preview_property),
                        );
                    }
                    PaletteBindingAction::None => {}
                }

                let max_frozen = flash.amplitude_max.is_frozen();
                let mut max_val = flash.amplitude_max.resolve(palettes, preview_property);
                ui.add_enabled_ui(!max_frozen, |ui| {
                    if self.clamp_max_numberpad.ui(ui, &mut max_val).changed() {
                        flash.amplitude_max = FixtureValue::Literal(max_val);
                    }
                });
                match palette_binding_button(
                    ui,
                    ctx,
                    &mut self.clamp_max_binding,
                    flash.amplitude_max,
                    palettes,
                    "Max palette".to_string(),
                ) {
                    PaletteBindingAction::Bind {
                        palette_id,
                        property,
                    } => {
                        flash.amplitude_max = FixtureValue::PalettePointer {
                            palette_id,
                            property,
                        };
                    }
                    PaletteBindingAction::Unbind => {
                        flash.amplitude_max = FixtureValue::Literal(
                            flash.amplitude_max.resolve(palettes, preview_property),
                        );
                    }
                    PaletteBindingAction::None => {}
                }
            });
        });
    }

    fn anim_audio_modulation_ui(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        live_audio: &blaulicht_audio_engine::CollectorOutput,
    ) {
        // Advance the preview on the editor's own clock so attack/release read
        // the same way they will at runtime.
        let now_ms = (ctx.input(|input| input.time) * 1000.0).max(0.0) as u64;
        if let AnimationSpecBody::AudioModulation(ref spec) = self.working_state.body {
            if !spec.blend.is_compatibility() {
                self.mod_preview_sample =
                    Some(self.mod_preview_runtime.tick(now_ms, spec, live_audio));
            } else {
                self.mod_preview_sample = None;
            }
        }
        ctx.request_repaint();

        let preview = self.mod_preview_sample;
        let AnimationSpecBody::AudioModulation(ref mut spec) = self.working_state.body else {
            return;
        };

        const BUTTON_SIZE: ButtonSize = ButtonSize::Medium;
        let cell_h = BUTTON_SIZE.dim().0.y;
        const LABEL_W: f32 = 120.0;

        if spec.is_compatibility() {
            Self::compatibility_modulation_ui(ui, spec, LABEL_W, cell_h);
            return;
        }

        // Signal source.
        ui.horizontal(|ui| {
            ui.add_sized([LABEL_W, cell_h], Label::new("Signal:"));
            for option in AudioModulationSignal::selectable() {
                let selected = option.discriminant_label() == spec.signal.discriminant_label();
                if components::button(
                    ui,
                    selected,
                    option.discriminant_label(),
                    BUTTON_SIZE.with_width(110.0),
                ) && !selected
                {
                    spec.signal = option;
                }
            }
        });

        if let AudioModulationSignal::Band {
            ref mut freq_min_hz,
            ref mut freq_max_hz,
        } = spec.signal
        {
            ui.horizontal(|ui| {
                ui.add_sized([LABEL_W, cell_h], Label::new("Freq Min:"));
                self.mod_freq_min_numberpad.ui(ui, freq_min_hz);
                ui.add_space(24.0);
                ui.add_sized([LABEL_W, cell_h], Label::new("Freq Max:"));
                self.mod_freq_max_numberpad.ui(ui, freq_max_hz);
            });
            if freq_min_hz > freq_max_hz {
                std::mem::swap(freq_min_hz, freq_max_hz);
            }
        }

        ui.separator();

        // Blend. Newly authored layers only ever expose Add and Scale.
        ui.horizontal(|ui| {
            ui.add_sized([LABEL_W, cell_h], Label::new("Blend:"));
            for option in AudioModulationBlend::selectable() {
                let selected = option.to_string() == spec.blend.to_string();
                if components::button(
                    ui,
                    selected,
                    &option.to_string(),
                    BUTTON_SIZE.with_width(110.0),
                ) && !selected
                {
                    spec.blend = option;
                }
            }
        });

        ui.horizontal(|ui| match spec.blend {
            AudioModulationBlend::Add { ref mut depth } => {
                ui.add_sized([LABEL_W, cell_h], Label::new("Depth:"));
                self.mod_depth_numberpad.ui(ui, depth);
                ui.label(
                    RichText::new("signed offset at full envelope")
                        .size(11.0)
                        .weak(),
                );
            }
            AudioModulationBlend::Scale {
                ref mut peak_percent,
            } => {
                ui.add_sized([LABEL_W, cell_h], Label::new("At peak:"));
                if ui
                    .add(HFader::new(peak_percent, 0.0..=MAX_SCALE_PERCENT))
                    .changed()
                {
                    // Fader edits are already in range; nothing else to do.
                }
                ui.label(
                    RichText::new(format!("{:.0} % of base at full envelope", peak_percent))
                        .size(11.0)
                        .weak(),
                );
            }
            AudioModulationBlend::LegacyAbsolute => {}
        });

        ui.separator();

        // Shaping.
        let shaping = &mut spec.shaping;
        ui.horizontal(|ui| {
            ui.add_sized([LABEL_W, cell_h], Label::new("Threshold:"));
            ui.add(HFader::new(&mut shaping.threshold, 0.0..=MAX_THRESHOLD));
            ui.label(RichText::new(format!("{:.2}", shaping.threshold)).size(11.0));
        });

        ui.horizontal(|ui| {
            ui.add_sized([LABEL_W, cell_h], Label::new("Sensitivity:"));
            ui.add(HFader::new(&mut shaping.sensitivity, 0.0..=MAX_SENSITIVITY));
            ui.label(RichText::new(format!("{:.2}x", shaping.sensitivity)).size(11.0));
        });

        ui.horizontal(|ui| {
            ui.add_sized([LABEL_W, cell_h], Label::new("Attack (ms):"));
            self.mod_attack_numberpad.ui(ui, &mut shaping.attack_ms);
            ui.add_space(24.0);
            ui.add_sized([LABEL_W, cell_h], Label::new("Release (ms):"));
            self.mod_release_numberpad.ui(ui, &mut shaping.release_ms);
        });

        ui.horizontal(|ui| {
            ui.checkbox(&mut shaping.invert, "Invert (while audio is valid)");
        });

        ui.separator();
        ui.label(RichText::new("Section multipliers").size(12.0).strong());

        for (label, multiplier) in [
            ("Breakdown:", &mut shaping.breakdown_multiplier),
            ("Drop:", &mut shaping.drop_multiplier),
            ("Active Beat:", &mut shaping.active_beat_multiplier),
        ] {
            ui.horizontal(|ui| {
                ui.add_sized([LABEL_W, cell_h], Label::new(label));
                ui.add(HFader::new(multiplier, 0.0..=MAX_SECTION_MULTIPLIER));
                ui.label(RichText::new(format!("{:.2}x", multiplier)).size(11.0));
            });
        }

        spec.shaping.sanitize();
        spec.blend.sanitize();

        ui.separator();
        Self::modulation_live_ui(ui, spec, preview, &live_audio.snapshot);
    }

    /// Compatibility layers stay editable only through the legacy fields that
    /// still apply to them.
    fn compatibility_modulation_ui(
        ui: &mut egui::Ui,
        spec: &mut AudioModulationSpec,
        label_w: f32,
        cell_h: f32,
    ) {
        ui.horizontal(|ui| {
            ui.add_sized([label_w, cell_h], Label::new("Mode:"));
            ui.label(
                RichText::new(format!("{} (compatibility)", spec.signal))
                    .color(Color32::LIGHT_YELLOW),
            );
        });
        ui.label(
            RichText::new(
                "Migrated from a legacy audio animation. It reproduces the old absolute \
                 output; create a new Audio Modulation layer to use shaping and blending.",
            )
            .size(11.0)
            .weak(),
        );

        let AudioModulationSignal::LegacySpectrum(ref mut freqs) = spec.signal else {
            return;
        };

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.add_sized([label_w, cell_h], Label::new("Gate:"));
            ui.add(egui::DragValue::new(&mut freqs.gate).range(0..=255));
            ui.add_space(24.0);
            ui.add_sized([label_w, cell_h], Label::new("Boost:"));
            ui.add(egui::DragValue::new(&mut freqs.boost).range(0..=255));
        });
        ui.horizontal(|ui| {
            ui.add_sized([label_w, cell_h], Label::new("Freq Min:"));
            ui.add(egui::DragValue::new(&mut freqs.freq_min).range(0..=20_000));
            ui.add_space(24.0);
            ui.add_sized([label_w, cell_h], Label::new("Freq Max:"));
            ui.add(egui::DragValue::new(&mut freqs.freq_max).range(0..=20_000));
        });
        if freqs.freq_min > freqs.freq_max {
            std::mem::swap(&mut freqs.freq_min, &mut freqs.freq_max);
        }
    }

    fn modulation_live_ui(
        ui: &mut egui::Ui,
        spec: &AudioModulationSpec,
        preview: Option<AudioModulationSample>,
        snapshot: &blaulicht_shared::CollectedAudioSnapshot,
    ) {
        let Some(sample) = preview else {
            return;
        };

        let health = audio::audio_health(snapshot);
        let health_color = if health.is_valid() {
            Color32::LIGHT_GREEN
        } else {
            Color32::LIGHT_RED
        };

        ui.label(RichText::new("Live").size(12.0).strong());
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("raw {:.3}", sample.raw)).size(12.0));
            ui.add_space(16.0);
            ui.label(RichText::new(format!("envelope {:.3}", sample.envelope)).size(12.0));
            ui.add_space(16.0);
            match audio::contribution(&spec.blend, sample.envelope) {
                Some(audio::AudioModulationContribution::Scale(factor)) => {
                    ui.label(RichText::new(format!("contribution x{factor:.3}")).size(12.0));
                }
                Some(audio::AudioModulationContribution::Add(offset)) => {
                    ui.label(RichText::new(format!("contribution {offset:+.1}")).size(12.0));
                }
                None => {}
            }
        });

        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!("audio {}", health.label()))
                    .size(12.0)
                    .color(health_color),
            );
            ui.add_space(16.0);
            ui.label(
                RichText::new(format!("section {:?}", snapshot.section_state))
                    .size(12.0)
                    .weak(),
            );

            if matches!(spec.signal, AudioModulationSignal::BeatPulse) {
                let confident = snapshot.bpm_confidence >= MIN_BEAT_FALLBACK_CONFIDENCE;
                ui.add_space(16.0);
                ui.label(
                    RichText::new(format!(
                        "bpm confidence {:.2}{}",
                        snapshot.bpm_confidence,
                        if confident {
                            ""
                        } else {
                            " (clock fallback off)"
                        }
                    ))
                    .size(12.0)
                    .color(if confident {
                        Color32::LIGHT_GREEN
                    } else {
                        Color32::LIGHT_YELLOW
                    }),
                );
            }
        });

        ui.label(
            RichText::new(format!(
                "A source with no new frame stays live for {MAX_FRAME_AGE_MS} ms, then decays."
            ))
            .size(10.0)
            .weak(),
        );
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
