//! Setup › Animations: the animation template editor.
//!
//! Lists the animation templates on the left; the toolbar creates a template
//! (Create, or Presets for a starter preset), deletes the selected one, and
//! the right side edits the selected template with
//! [`AnimationEditState`](crate::app::pages::AnimationEditState).
//!
//! Applying templates and presets to a selection lives in the Programmer
//! (`pages/programmer_animations.rs`), which does not call into this module.

use super::duo;
use crate::app::components::studio::{self, ListRow, ListRowState};
use crate::app::{
    components::{self, button, ButtonSize, Dialog, Pagination},
    pages::AnimationEditState,
    BlaulichtApp,
};
use blaulicht_engine::{
    client::EngineClient, dmx::animation::phaser, hub::HostCommand,
    plugin::stable_animation_template_id,
};
use blaulicht_shared::{
    fixture::value::FixtureValue, AnimationPresetCategory, AnimationSpec, AnimationSpecBody,
    AnimationSpecBodyKind, AnimationSpeedModifier, AnimationTemplate, FixtureProperty,
    PhaserDuration, PhaserKind,
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

        if crate::app::ui_tokens::Density::of(ctx).is_compact() {
            self.setup_animations_desktop_ui(ui, ctx);
            return;
        }

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

                                self.animation_template_editor(
                                    ui,
                                    ctx,
                                    &templates,
                                    &palettes_snapshot,
                                    &selected_fixtures,
                                    panel_max,
                                );
                            });
                        });
                    },
                );
            },
        );
    }
    /// Setup › Animations on the desktop (`setup.html` `animations()`): the
    /// templates under an "Animations" bar with Add and Presets (name and
    /// property per row), and the selected template under an "Animation:"
    /// bar with Delete: its name, property and kind, and a read-only Shape
    /// pane ([`shape_pane`]).
    fn setup_animations_desktop_ui(&mut self, ui: &mut egui::Ui, ctx: &Context) {
        let (templates, palettes_snapshot) = self
            .client
            .with_session(|show, _| (show.0.animation_templates.clone(), show.0.palettes.clone()))
            .unwrap_or_default();
        if self
            .animation_ui_state
            .selected_animation_id
            .map_or(true, |id| !templates.contains_key(&id))
        {
            self.animation_ui_state.selected_animation_id = templates.keys().next().copied();
        }
        let (mut list, mut detail) = duo::duo(ui);

        let (mut add, mut presets) = (false, false);
        let (add_open, presets_open) = (
            self.animation_ui_state.create_open,
            self.animation_ui_state.presets_open,
        );
        studio::pane_header(&mut list, None, "Animations", true, |ui| {
            add = duo::bar_button(ui, "Add", add_open);
            presets = duo::bar_button(ui, "Presets", presets_open);
        });
        if add {
            self.animation_ui_state.create_open = !self.animation_ui_state.create_open;
            if !self.animation_ui_state.create_open {
                self.animation_ui_state.close_selection_dialogs();
            }
        }
        if presets {
            self.animation_ui_state.presets_open = !self.animation_ui_state.presets_open;
        }

        let t = crate::app::ui_tokens::studio(ctx);
        let small = crate::app::ui_tokens::Density::of(ctx)
            .font(crate::app::ui_tokens::Weight::Regular, 12.0);
        let mut picked = None;
        egui::ScrollArea::vertical()
            .id_salt("setup-animations-list")
            .auto_shrink([false, false])
            .show(&mut list, |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
                for (id, template) in &templates {
                    let active = self.animation_ui_state.selected_animation_id == Some(*id);
                    let state = if active {
                        ListRowState::Active
                    } else {
                        ListRowState::Idle
                    };
                    let response = ListRow::new(&template.spec.name).state(state).show(ui);
                    let property = format!("{:?}", template.spec.property);
                    ui.painter().text(
                        response.rect.right_center() - egui::vec2(8.0, 0.0),
                        egui::Align2::RIGHT_CENTER,
                        property,
                        small.clone(),
                        if active { t.strong } else { t.muted },
                    );
                    if response.clicked() {
                        picked = Some(*id);
                    }
                }
            });
        if picked.is_some() {
            self.animation_ui_state.selected_animation_id = picked;
        }

        let selected = self
            .animation_ui_state
            .selected_animation_id
            .and_then(|id| templates.get(&id));
        let Some(template) = selected else {
            studio::pane_header(&mut detail, None, "Animation", true, |_| {});
            duo::body(&mut detail, None, |ui| {
                duo::note(ui, "Add an animation, or start from a preset.")
            });
            return;
        };
        let mut delete = false;
        let delete_open = self.animation_ui_state.delete_confirm_open;
        studio::pane_header(
            &mut detail,
            Some("Animation:"),
            &template.spec.name,
            true,
            |ui| {
                delete = duo::bar_button(ui, "Delete", delete_open);
            },
        );
        if delete {
            self.animation_ui_state.delete_confirm_open =
                !self.animation_ui_state.delete_confirm_open;
        }
        let id = self
            .animation_ui_state
            .selected_animation_id
            .unwrap_or_default();
        let mut rename = None;
        duo::body(&mut detail, Some(duo::form_height(3)), |ui| {
            rename = duo::name_row(ui, ui.id().with(("animation", id)), &template.spec.name);
            duo::form_row(ui, "Property", |ui| {
                duo::text(ui, format!("{:?}", template.spec.property))
            });
            duo::form_row(ui, "Kind", |ui| {
                let body = format!("{:?}", template.spec.body);
                let kind = body.split(['(', ' ', '{']).next().unwrap_or_default();
                duo::text(ui, kind.to_owned())
            });
        });
        if let Some(name) = rename {
            let mut spec = template.spec.clone();
            spec.name = name.clone();
            self.animation_ui_state.edit_state.working_state.name = name;
            self.client
                .send(HostCommand::UpdateAnimationTemplate { id, spec });
        }
        duo::lower_pane(&mut detail, "Shape");
        duo::body(&mut detail, None, |ui| {
            shape_pane(ui, template, &palettes_snapshot);
        });
    }

    /// The editor of the selected template, or "No Animation Selected".
    #[allow(clippy::too_many_arguments)]
    fn animation_template_editor(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &Context,
        templates: &BTreeMap<u8, AnimationTemplate>,
        palettes_snapshot: &BTreeMap<u8, blaulicht_shared::palette::Palette>,
        selected_fixtures: &[(u8, u8)],
        panel_max: egui::Pos2,
    ) {
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
                    if let AnimationSpecBody::WasmPlugin(wasm) = &anim.spec.body {
                        // The editor instance starts from the
                        // template's plugin state.
                        self.client.send(HostCommand::CloneAnimationInstanceState {
                            plugin_key: wasm.plugin_key.clone(),
                            source: stable_animation_template_id(&wasm.plugin_key, id),
                            target: self.animation_ui_state.edit_state.wasm_editor_instance_id,
                        });
                    }

                    self.animation_ui_state.selected_animation_id_before =
                        self.animation_ui_state.selected_animation_id;
                }

                let live_audio = self.live_audio_for_preview();

                if let Some(value_changed) = self.animation_ui_state.edit_state.show(
                    ui,
                    ctx,
                    palettes_snapshot,
                    &live_audio,
                    &self.client,
                    &mut self.view_state.plugin_tabs_selected,
                    selected_fixtures,
                    panel_max,
                ) {
                    if let AnimationSpecBody::WasmPlugin(wasm) = &value_changed.body {
                        // And the template gets the editor's
                        // state back, before its new spec.
                        self.client.send(HostCommand::CloneAnimationInstanceState {
                            plugin_key: wasm.plugin_key.clone(),
                            source: self.animation_ui_state.edit_state.wasm_editor_instance_id,
                            target: stable_animation_template_id(&wasm.plugin_key, id),
                        });
                    }
                    self.client.send(HostCommand::UpdateAnimationTemplate {
                        id,
                        spec: value_changed,
                    });
                }
            }
            None => {
                ui.label("No Animation Selected");
            }
        }
    }
}

/// The Shape pane of the desktop Animations page (`setup.html`
/// `animations()`): a 150px preview of the curve drawn from the saved
/// settings, then its settings as a read-only list.
fn shape_pane(
    ui: &mut egui::Ui,
    template: &AnimationTemplate,
    palettes: &BTreeMap<u8, blaulicht_shared::palette::Palette>,
) {
    let property = template.spec.property;
    let literal = |v: &FixtureValue| match v {
        FixtureValue::Literal(n) => n.to_string(),
        _ => "Set by palette".to_owned(),
    };
    // Points of two cycles, x and y in 0..=1 (y up).
    let (points, rows): (Vec<[f32; 2]>, Vec<(&str, String)>) = match &template.spec.body {
        AnimationSpecBody::Phaser(p) => {
            const N: usize = 400;
            let raw: Vec<f32> = (0..=N)
                .map(|i| {
                    phaser::generate(p, i as f64 * 720.0 / N as f64, property, palettes) as f32
                })
                .collect();
            let top = raw.iter().copied().fold(255.0_f32, f32::max);
            let points = raw
                .iter()
                .enumerate()
                .map(|(i, y)| [i as f32 / N as f32, y / top])
                .collect();
            let (curve, range) = match &p.kind {
                PhaserKind::Mathematical(m) => (
                    format!("{:?}", m.base),
                    format!(
                        "{} to {}",
                        literal(&m.amplitude_min),
                        literal(&m.amplitude_max)
                    ),
                ),
                PhaserKind::Keyframed(_) => ("Keyframed".to_owned(), "--".to_owned()),
            };
            let duration = match p.time_total {
                PhaserDuration::Fixed(ms) => format!("{ms} ms"),
                PhaserDuration::Beat(m) => format!("{} beats", beats(m)),
            };
            (
                points,
                vec![
                    ("Curve", curve),
                    ("Range", range),
                    ("Duration", duration),
                    ("Sync", format!("{:?}", p.sync)),
                ],
            )
        }
        AnimationSpecBody::FlashAnimation(f) => {
            let (on, off) = (f.on_time_ms.max(1) as f32, f.off_time_ms as f32);
            let period = on + off;
            let mut points = Vec::new();
            for cycle in 0..2 {
                let start = cycle as f32 * period;
                for (x, y) in [
                    (start, 1.0),
                    (start + on, 1.0),
                    (start + on, 0.0),
                    (start + period, 0.0),
                ] {
                    points.push([x / (2.0 * period), y]);
                }
            }
            (
                points,
                vec![
                    (
                        "Range",
                        format!(
                            "{} to {}",
                            literal(&f.amplitude_min),
                            literal(&f.amplitude_max)
                        ),
                    ),
                    ("On", format!("{} ms", f.on_time_ms)),
                    ("Off", format!("{} ms", f.off_time_ms)),
                    (
                        "Beat aligned",
                        if f.beat_aligned { "Yes" } else { "No" }.to_owned(),
                    ),
                ],
            )
        }
        AnimationSpecBody::WasmPlugin(_) | AnimationSpecBody::Wasm(_) => {
            duo::note(
                ui,
                "This animation is drawn by a plugin, so its shape isn't known here.",
            );
            return;
        }
        _ => {
            duo::note(
                ui,
                "This animation follows the audio, so its shape isn't known here.",
            );
            return;
        }
    };

    // `.wave`: the well, a centre line and the curve.
    let t = crate::app::ui_tokens::studio(ui.ctx());
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), 150.0),
        egui::Sense::hover(),
    );
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, t.well);
    painter.hline(
        rect.x_range(),
        rect.center().y.round() + 0.5,
        egui::Stroke::new(1.0_f32, t.line),
    );
    // The mockup's viewBox: y from 58 (0) to 4 (1) of 60.
    let line: Vec<egui::Pos2> = points
        .iter()
        .map(|[x, y]| {
            egui::pos2(
                rect.left() + x * rect.width(),
                rect.top() + (58.0 - y * 54.0) / 60.0 * rect.height(),
            )
        })
        .collect();
    painter.add(egui::Shape::line(line, egui::Stroke::new(1.5_f32, t.data)));

    ui.add_space(8.0);
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.y = duo::KV_GAP;
        for (term, value) in rows {
            duo::kv_row(ui, term, |ui| duo::text(ui, value));
        }
    });
    ui.add_space(8.0);
    duo::note(ui, "Curve drawn from the saved settings.");
}

/// A beat count like the mockup's "1/4 beats".
fn beats(m: AnimationSpeedModifier) -> String {
    format!("{m:?}").trim_start_matches('_').replace('_', "/")
}
