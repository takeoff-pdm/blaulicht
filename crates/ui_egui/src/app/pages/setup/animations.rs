//! Setup › Animations: the animation template editor.
//!
//! Lists the animation templates on the left; the toolbar creates a template
//! (Create, or Presets for a starter preset), deletes the selected one, and
//! the right side edits the selected template with
//! [`AnimationEditState`](crate::app::pages::AnimationEditState).
//!
//! Applying templates and presets to a selection lives in the Programmer
//! (`pages/programmer_animations.rs`), which does not call into this module.

use crate::app::{
    components::{self, button, ButtonSize, Dialog, Pagination},
    pages::AnimationEditState,
    BlaulichtApp,
};
use blaulicht_engine::{
    client::EngineClient, hub::HostCommand, plugin::stable_animation_template_id,
};
use blaulicht_shared::{
    AnimationPresetCategory, AnimationSpec, AnimationSpecBody, AnimationSpecBodyKind,
    AnimationTemplate, FixtureProperty,
};
use egui::{Context, FontId, Key, Label, RichText, TextEdit, Vec2};
use std::collections::BTreeMap;
use strum::IntoEnumIterator;

/// Four, not five, since the Setup tab bar takes the room of the fifth at 800x480.
const ANIMATIONS_PER_PAGE: usize = 4;

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
                    if components::button_as(
                        ui,
                        false,
                        components::ButtonVariant::Primary,
                        "Confirm",
                        ButtonSize::Large,
                    ) {
                        if let Some(id) = self.animation_ui_state.selected_animation_id {
                            self.send_setup_command(
                                HostCommand::DeleteAnimationTemplate(id),
                                |_, _| {},
                            );
                            self.animation_ui_state.selected_animation_id = None;
                            self.animation_ui_state.selected_animation_id_before = None;
                            self.animation_ui_state.delete_confirm_open = false;
                        }
                    }

                    if components::button_as(
                        ui,
                        true,
                        components::ButtonVariant::Tonal,
                        "Cancel",
                        ButtonSize::Large,
                    ) {
                        self.animation_ui_state.delete_confirm_open = false;
                    }
                });
            });
    }

    /// Sends `spec` as a new animation template; `on_ok` runs once the
    /// engine has added it ("Animation limit reached" shows as a popup).
    /// Does nothing while the previous Setup command still waits for its
    /// reply, so a double tap adds one template.
    fn insert_animation_template(
        &mut self,
        spec: AnimationSpec,
        on_ok: impl FnOnce(&mut Self) + 'static,
    ) {
        if self.setup_reply_pending() {
            return;
        }
        self.send_setup_command(HostCommand::CreateAnimationTemplate(spec), |app, _| {
            on_ok(app)
        });
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
                        ButtonSize::Medium.with_touch_width(90.0),
                    ) {
                        self.animation_ui_state.preset_category = None;
                    }
                    for category in AnimationPresetCategory::iter() {
                        if components::button(
                            ui,
                            selected_category == Some(category),
                            category.label(),
                            ButtonSize::Medium.with_touch_width(110.0),
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
            self.insert_animation_template(AnimationSpec::preset(preset), |app| {
                app.animation_ui_state.presets_open = false;
            });
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
                ui.spacing_mut().interact_size =
                    crate::app::ui_tokens::Density::of(ui.ctx()).interact_size();

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

                    let mut button_pressed = components::button_as(
                        ui,
                        true,
                        components::ButtonVariant::Primary,
                        "Create",
                        ButtonSize::Medium,
                    );

                    ctx.input(|input| {
                        if input.key_pressed(Key::Enter) {
                            button_pressed = true;
                        }
                    });

                    if button_pressed {
                        // The name stays in the field until the template is
                        // added, so a refused create keeps it.
                        let spec = AnimationSpec {
                            name: self.animation_ui_state.new_name.clone(),
                            body: AnimationSpecBody::from(self.animation_ui_state.new_mode),
                            property: self.animation_ui_state.new_prop,
                        };

                        self.insert_animation_template(spec, |app| {
                            app.animation_ui_state.new_name.clear();
                            app.animation_ui_state.create_open = false;
                            app.animation_ui_state.close_selection_dialogs();
                        });
                    }
                });
            });
        }
    }

    fn animation_pagination(
        &mut self,
        ui: &mut egui::Ui,
        templates: &BTreeMap<u8, AnimationTemplate>,
        compact: bool,
    ) {
        // TODO: can we do this without the allocation?
        let raw_items: Vec<_> = templates.iter().collect();
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

                    let label = components::indexed_label(id, &animation.spec.name);

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

    /// Setup › Animations. Also rendered on its own for a desktop dock pane
    /// still showing the old Animations page (`AppPage::Animations`).
    pub(crate) fn setup_animations_ui(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &Context,
        render_context: crate::app::page::PageRenderContext,
    ) {
        let panel_max = ui.clip_rect().max;
        self.render_add_animation_dialog(ctx, ui);
        self.render_presets_dialog(ctx, ui);
        self.render_delete_animation_dialog(ctx, ui);

        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), ui.available_height()),
            render_context.primary_layout(),
            |ui| {
                // One read per frame: the templates, the palettes (for the
                // editor's previews) and this session's selected fixtures.
                let (templates, palettes_snapshot, selected_fixtures) = self
                    .client
                    .with_session(|show, me| {
                        (
                            show.0.animation_templates.clone(),
                            show.0.palettes.clone(),
                            me.fixture_selection(&show.0.groups).fixtures,
                        )
                    })
                    .unwrap_or_default();
                self.animation_pagination(ui, &templates, render_context.is_narrow_dynamic());

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

                            if components::button_as(
                                ui,
                                self.animation_ui_state.selected_animation_id.is_some(),
                                components::ButtonVariant::Danger,
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

                                // A template deleted elsewhere is no selection.
                                let selected = self
                                    .animation_ui_state
                                    .selected_animation_id
                                    .and_then(|id| templates.get(&id).map(|anim| (id, anim)));
                                match selected {
                                    Some((id, anim)) => {
                                        // Load new value.
                                        if self.animation_ui_state.selected_animation_id
                                            != self.animation_ui_state.selected_animation_id_before
                                        {
                                            self.animation_ui_state
                                                .edit_state
                                                .load_state(anim.spec.clone());
                                            if let AnimationSpecBody::WasmPlugin(wasm) =
                                                &anim.spec.body
                                            {
                                                // The editor instance starts from the
                                                // template's plugin state.
                                                self.client.send(
                                                    HostCommand::CloneAnimationInstanceState {
                                                        plugin_key: wasm.plugin_key.clone(),
                                                        source: stable_animation_template_id(
                                                            &wasm.plugin_key,
                                                            id,
                                                        ),
                                                        target: self
                                                            .animation_ui_state
                                                            .edit_state
                                                            .wasm_editor_instance_id,
                                                    },
                                                );
                                            }

                                            self.animation_ui_state.selected_animation_id_before =
                                                self.animation_ui_state.selected_animation_id;
                                        }

                                        let live_audio = self.live_audio_for_preview();

                                        if let Some(value_changed) =
                                            self.animation_ui_state.edit_state.show(
                                                ui,
                                                ctx,
                                                &palettes_snapshot,
                                                &live_audio,
                                                &self.client,
                                                &mut self.view_state.plugin_tabs_selected,
                                                &selected_fixtures,
                                                panel_max,
                                            )
                                        {
                                            if let AnimationSpecBody::WasmPlugin(wasm) =
                                                &value_changed.body
                                            {
                                                // And the template gets the editor's
                                                // state back, before its new spec.
                                                self.client.send(
                                                    HostCommand::CloneAnimationInstanceState {
                                                        plugin_key: wasm.plugin_key.clone(),
                                                        source: self
                                                            .animation_ui_state
                                                            .edit_state
                                                            .wasm_editor_instance_id,
                                                        target: stable_animation_template_id(
                                                            &wasm.plugin_key,
                                                            id,
                                                        ),
                                                    },
                                                );
                                            }
                                            self.client.send(
                                                HostCommand::UpdateAnimationTemplate {
                                                    id,
                                                    spec: value_changed,
                                                },
                                            );
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
