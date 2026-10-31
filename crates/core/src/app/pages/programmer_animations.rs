//! The Programmer's Animations dialog: apply starter presets and saved
//! animation templates to the current selection, list the animations active
//! on it (with remove buttons), and save one of them as a new template.
//!
//! Everything goes through the existing animation events: applying sends
//! `AddAnimation` + `PlayAnimation` in one `Transaction`, removing sends
//! `RemoveAnimation`. A starter preset
//! (`AnimationPreset`) is not a template, so applying one first reuses the
//! template named after it or creates that template, the same way the
//! Animations page does. Editing templates stays on the Animations page.

use crate::{
    app::{
        components::{self, ButtonSize, Dialog, TextInput},
        BlaulichtApp, PopupSpec,
    },
    dmx::EngineState,
};
use blaulicht_shared::{
    scene::{FixtureSelection, Scene},
    ActiveAnimation, AnimationPreset, AnimationPresetCategory, AnimationSpec, AnimationSpecBody,
    AnimationTemplate, ControlEvent, ControlEventMessage, EventOriginator, FixtureProperty,
};
use egui::{Color32, Context, RichText};
use std::{
    collections::{BTreeMap, HashSet},
    time::Duration,
};
use strum::IntoEnumIterator;

/// State of the Animations dialog.
#[derive(Default)]
pub struct AnimationPresetsUi {
    pub open: bool,
    /// The "Save as preset" form, while it is open.
    pub save: Option<SaveAnimationPreset>,
}

/// An active animation that is about to be saved as a template.
pub struct SaveAnimationPreset {
    pub selection: FixtureSelection,
    pub animation_id: u8,
    pub name: String,
}

/// The animations on `selection` in `scene`, keyed by template id. The
/// engine keys them by the *sorted* selection.
pub(crate) fn selection_animations<'a>(
    scene: &'a Scene,
    selection: &FixtureSelection,
) -> Option<&'a BTreeMap<u8, ActiveAnimation>> {
    scene.sink.active_animations.get(&selection.sorted())
}

/// The template a starter preset maps to: the lowest-id template named after
/// the preset.
fn preset_template_id(
    templates: &BTreeMap<u8, AnimationTemplate>,
    preset: AnimationPreset,
) -> Option<u8> {
    templates
        .iter()
        .find(|(_, template)| template.spec.name == preset.label())
        .map(|(id, _)| *id)
}

/// The lowest unused template id.
fn free_template_id(templates: &BTreeMap<u8, AnimationTemplate>) -> Option<u8> {
    (0..=u8::MAX).find(|id| !templates.contains_key(id))
}

/// Returns the template for `preset`, creating it from
/// `AnimationSpec::preset` if there is none. `None` when every template id is
/// taken.
pub(crate) fn ensure_preset_template(
    templates: &mut BTreeMap<u8, AnimationTemplate>,
    preset: AnimationPreset,
) -> Option<u8> {
    if let Some(id) = preset_template_id(templates, preset) {
        return Some(id);
    }
    let id = free_template_id(templates)?;
    templates.insert(
        id,
        AnimationTemplate {
            spec: AnimationSpec::preset(preset),
        },
    );
    Some(id)
}

/// Inserts `spec` as a new template named `name`. `None` when every template
/// id is taken.
pub(crate) fn save_as_template(
    templates: &mut BTreeMap<u8, AnimationTemplate>,
    mut spec: AnimationSpec,
    name: &str,
) -> Option<u8> {
    let id = free_template_id(templates)?;
    spec.name = name.trim().to_string();
    templates.insert(id, AnimationTemplate { spec });
    Some(id)
}

/// `base`, or `base 2`, `base 3`, … if a template already uses that name.
pub(crate) fn unique_template_name(
    templates: &BTreeMap<u8, AnimationTemplate>,
    base: &str,
) -> String {
    let base = base.trim();
    let base = if base.is_empty() { "Animation" } else { base };
    let taken: HashSet<&str> = templates
        .values()
        .map(|template| template.spec.name.as_str())
        .collect();
    if !taken.contains(base) {
        return base.to_string();
    }
    (2..)
        .map(|n| format!("{base} {n}"))
        .find(|name| !taken.contains(name.as_str()))
        .unwrap()
}

/// Tapping a template toggles it: applied and started if it is not on the
/// selection yet (a new animation starts paused), removed if it is. The
/// transaction rolls back as a whole when the engine rejects the add.
pub(crate) fn toggle_animation_event(template_id: u8, active: bool) -> ControlEvent {
    if active {
        ControlEvent::RemoveAnimation(template_id)
    } else {
        ControlEvent::Transaction(vec![
            ControlEvent::AddAnimation(template_id),
            ControlEvent::PlayAnimation(template_id),
        ])
    }
}

/// A template's label: its name, or `#id` when it has none.
fn template_label(id: u8, name: &str) -> String {
    if name.trim().is_empty() {
        format!("#{id}")
    } else {
        name.to_string()
    }
}

/// Templates that are not named after a starter preset, by id, with their
/// labels. The others are
/// reached through their preset button.
fn saved_templates(
    templates: &BTreeMap<u8, AnimationTemplate>,
) -> Vec<(u8, String, FixtureProperty)> {
    let preset_names: HashSet<&str> = AnimationPreset::iter()
        .map(|preset| preset.label())
        .collect();
    templates
        .iter()
        .filter(|(_, template)| !preset_names.contains(template.spec.name.as_str()))
        .map(|(id, template)| {
            (
                *id,
                template_label(*id, &template.spec.name),
                template.spec.property,
            )
        })
        .collect()
}

/// Properties that are bound to a palette on any fixture of the selection.
/// The engine rejects animations on those.
fn frozen_properties(scene: &Scene, selection: &FixtureSelection) -> HashSet<FixtureProperty> {
    let mut frozen = HashSet::new();
    for key in &selection.fixtures {
        let Some(state) = scene.sink.fixture_states.get(key) else {
            continue;
        };
        for property in FixtureProperty::iter() {
            if state.slot(property).is_frozen() {
                frozen.insert(property);
            }
        }
    }
    frozen
}

impl BlaulichtApp {
    /// Number of animations on the current selection in the focused scene
    /// (the toolbar button's count).
    pub(crate) fn selection_animation_count(dmx_engine: &EngineState) -> usize {
        selection_animations(dmx_engine.curr_scene(), &dmx_engine.get_selection())
            .map_or(0, BTreeMap::len)
    }

    fn send_animation_event(&self, event: ControlEvent) {
        self.data
            .client
            .send(ControlEventMessage::new(EventOriginator::Web, event));
    }

    /// Applies a starter preset to the selection, creating its template first
    /// if needed; removes it if it is already applied.
    fn toggle_preset(&mut self, preset: AnimationPreset, active_ids: &HashSet<u8>) {
        let id = {
            let mut dmx_engine = self.data.state.dmx_engine.write().unwrap();
            ensure_preset_template(&mut dmx_engine.0.animation_templates, preset)
        };
        match id {
            Some(id) => {
                self.send_animation_event(toggle_animation_event(id, active_ids.contains(&id)))
            }
            None => self.show_popup(PopupSpec::with_duration(
                Duration::from_secs(3),
                "Animation limit reached".to_string(),
            )),
        }
    }

    /// Stores the active animation's spec as a new template.
    fn save_animation_as_preset(&mut self, save: &SaveAnimationPreset) {
        let mut dmx_engine = self.data.state.dmx_engine.write().unwrap();
        let scene_id = dmx_engine.0.current_scene_focus;
        let Some(spec) = dmx_engine
            .0
            .scenes
            .get(&scene_id)
            .and_then(|scene| selection_animations(scene, &save.selection))
            .and_then(|animations| animations.get(&save.animation_id))
            .map(|animation| animation.spec_cloned.clone())
        else {
            return;
        };
        let wasm_key = match &spec.body {
            AnimationSpecBody::WasmPlugin(wasm) => Some(wasm.plugin_key.clone()),
            _ => None,
        };
        let saved = save_as_template(&mut dmx_engine.0.animation_templates, spec, &save.name);
        drop(dmx_engine);

        let Some(new_id) = saved else {
            self.show_popup(PopupSpec::with_duration(
                Duration::from_secs(3),
                "Animation limit reached".to_string(),
            ));
            return;
        };

        // A WASM animation keeps its plugin state per instance; the template
        // gets a copy, as when the Animations page saves its editor state.
        if let Some(plugin_key) = wasm_key {
            let fixtures = save.selection.sorted().fixtures;
            let source = crate::plugin::tick::stable_animation_instance_id(
                &plugin_key,
                scene_id,
                save.animation_id,
                &fixtures,
            );
            let target = crate::plugin::tick::stable_animation_template_id(&plugin_key, new_id);
            crate::plugin::wasm::clone_animation_instance_state(
                &self.data.state,
                &plugin_key,
                source,
                target,
            );
        }

        self.show_popup(PopupSpec::with_duration(
            Duration::from_secs(3),
            format!("Saved preset \"{}\"", save.name.trim()),
        ));
    }

    /// The Animations dialog (toolbar button "Animations"). Left: the
    /// animations on the selection, each with remove and save. Right: saved
    /// templates and the starter presets; one tap applies or removes one.
    pub(crate) fn render_animation_presets_dialog(
        &mut self,
        ctx: &Context,
        dmx_engine: &EngineState,
    ) {
        if !self.programmer_ui_state.animation_presets.open {
            return;
        }

        const WIDTH: f32 = 740.0;
        // Used to centre the dialog; the content sets the real height.
        const HEIGHT: f32 = 420.0;
        const LIST_HEIGHT: f32 = 290.0;
        const ACTIVE_WIDTH: f32 = 240.0;
        const ROW_BUTTON: f32 = 32.0;
        const SAVE_BUTTON: f32 = 54.0;
        // Preset buttons per row.
        const PRESET_COLUMNS: usize = 4;

        let scene = dmx_engine.curr_scene();
        let selection = dmx_engine.get_selection().sorted();
        let has_selection = !dmx_engine.selection().is_empty();
        let templates = &dmx_engine.0.animation_templates;
        let active: Vec<(u8, String, FixtureProperty, bool)> =
            selection_animations(scene, &selection)
                .map(|animations| {
                    animations
                        .iter()
                        .map(|(id, animation)| {
                            (
                                *id,
                                template_label(*id, &animation.spec_cloned.name),
                                animation.spec_cloned.property,
                                animation.enabled,
                            )
                        })
                        .collect()
                })
                .unwrap_or_default();
        let active_ids: HashSet<u8> = active.iter().map(|(id, ..)| *id).collect();
        let frozen = frozen_properties(scene, &selection);
        let saved = saved_templates(templates);

        // Esc belongs to the save form while it is open.
        let save_open = self.programmer_ui_state.animation_presets.save.is_some();

        let response = Dialog::new(
            "Programmer Animations".to_string(),
            egui::vec2(WIDTH, HEIGHT),
        )
        .with_backdrop()
        .show(ctx, |ui| {
            ui.heading(RichText::new("Animations").strong());
            ui.label(if has_selection {
                format!(
                    "Scene \"{}\" · {} fixture(s) selected. Tap to apply, tap again to remove.",
                    scene.name,
                    selection.fixtures.len()
                )
            } else {
                "Select a group or fixtures first.".to_string()
            });
            ui.separator();

            ui.horizontal_top(|ui| {
                //
                // Active on the selection.
                //
                ui.allocate_ui_with_layout(
                    egui::vec2(ACTIVE_WIDTH, LIST_HEIGHT),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.set_width(ACTIVE_WIDTH);
                        ui.label(
                            RichText::new(format!("On selection ({})", active.len())).strong(),
                        );
                        ui.add_space(4.0);
                        egui::ScrollArea::vertical()
                            .id_salt("programmer_active_animations")
                            .max_height(LIST_HEIGHT - 24.0)
                            .min_scrolled_height(LIST_HEIGHT - 24.0)
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                if active.is_empty() {
                                    ui.label("No animations on this selection.");
                                }
                                for (id, name, property, enabled) in &active {
                                    ui.horizontal(|ui| {
                                        if components::button(
                                            ui,
                                            false,
                                            egui_phosphor::regular::X,
                                            ButtonSize::Medium.with_width(ROW_BUTTON),
                                        ) {
                                            self.send_animation_event(
                                                ControlEvent::RemoveAnimation(*id),
                                            );
                                        }
                                        let text_width = (ui.available_width()
                                            - SAVE_BUTTON
                                            - ui.spacing().item_spacing.x)
                                            .max(40.0);
                                        ui.allocate_ui_with_layout(
                                            egui::vec2(text_width, ROW_BUTTON),
                                            egui::Layout::top_down(egui::Align::Min),
                                            |ui| {
                                                ui.set_width(text_width);
                                                ui.add(
                                                    egui::Label::new(
                                                        RichText::new(name).color(Color32::WHITE),
                                                    )
                                                    .truncate(),
                                                );
                                                let state =
                                                    if *enabled { "" } else { " · paused" };
                                                ui.add(
                                                    egui::Label::new(
                                                        RichText::new(format!(
                                                            "#{id} · {property}{state}"
                                                        ))
                                                        .small(),
                                                    )
                                                    .truncate(),
                                                );
                                            },
                                        );
                                        if components::button(
                                            ui,
                                            false,
                                            "Save",
                                            ButtonSize::Medium.with_width(SAVE_BUTTON),
                                        ) {
                                            self.programmer_ui_state.animation_presets.save =
                                                Some(SaveAnimationPreset {
                                                    selection: selection.clone(),
                                                    animation_id: *id,
                                                    name: unique_template_name(templates, name),
                                                });
                                        }
                                    });
                                    ui.add_space(2.0);
                                }
                            });
                    },
                );

                ui.separator();

                //
                // Saved templates and starter presets.
                //
                // The column gets an explicit width: inside the horizontal
                // row the wrapped button rows would otherwise grow the dialog
                // instead of wrapping.
                let column_width = (WIDTH - ACTIVE_WIDTH - 2.0 * ui.spacing().item_spacing.x - 8.0)
                    .max(PRESET_COLUMNS as f32 * 60.0);
                ui.allocate_ui_with_layout(
                    egui::vec2(column_width, LIST_HEIGHT),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.set_width(column_width);
                        egui::ScrollArea::vertical()
                            .id_salt("programmer_animation_presets")
                            .max_height(LIST_HEIGHT)
                            .min_scrolled_height(LIST_HEIGHT)
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                let spacing = ui.spacing().item_spacing.x;
                                // Leave room for the scroll bar.
                                let size = ButtonSize::Medium.with_width(
                                    ((column_width - 12.0 - spacing * (PRESET_COLUMNS - 1) as f32)
                                        / PRESET_COLUMNS as f32)
                                        .floor(),
                                );
                                // One tap applies or removes; the button shows
                                // whether it is on the selection.
                                let tap = |ui: &mut egui::Ui,
                                           label: &str,
                                           property: FixtureProperty,
                                           is_active: bool,
                                           hover: &str|
                                 -> bool {
                                    let enabled =
                                        has_selection && (is_active || !frozen.contains(&property));
                                    let reason = if !has_selection {
                                        "Nothing selected".to_string()
                                    } else {
                                        format!("{property} is bound to a palette")
                                    };
                                    let response = ui.scope(|ui| {
                                        if enabled {
                                            components::button(ui, is_active, label, size)
                                        } else {
                                            components::action_button(
                                                ui,
                                                false,
                                                label,
                                                size,
                                                Some(&reason),
                                            )
                                        }
                                    });
                                    if enabled {
                                        response.response.on_hover_text(hover);
                                    }
                                    response.inner
                                };

                                // Fixed rows: `horizontal_wrapped` does not
                                // wrap inside the dialog's horizontal row.
                                if !saved.is_empty() {
                                    ui.label(RichText::new("Saved").strong());
                                    for row in saved.chunks(PRESET_COLUMNS) {
                                        ui.horizontal(|ui| {
                                            for (id, name, property) in row {
                                                let is_active = active_ids.contains(id);
                                                if tap(
                                                    ui,
                                                    name,
                                                    *property,
                                                    is_active,
                                                    &format!("Template #{id} · {property}"),
                                                ) {
                                                    self.send_animation_event(
                                                        toggle_animation_event(*id, is_active),
                                                    );
                                                }
                                            }
                                        });
                                    }
                                    ui.add_space(6.0);
                                }

                                for category in AnimationPresetCategory::iter() {
                                    ui.label(RichText::new(category.label()).strong())
                                        .on_hover_text(category.description());
                                    let presets: Vec<AnimationPreset> =
                                        category.presets().collect();
                                    for row in presets.chunks(PRESET_COLUMNS) {
                                        ui.horizontal(|ui| {
                                            for preset in row {
                                                let is_active =
                                                    preset_template_id(templates, *preset)
                                                        .is_some_and(|id| active_ids.contains(&id));
                                                if tap(
                                                    ui,
                                                    preset.label(),
                                                    preset.property(),
                                                    is_active,
                                                    preset.description(),
                                                ) {
                                                    self.toggle_preset(*preset, &active_ids);
                                                }
                                            }
                                        });
                                    }
                                    ui.add_space(6.0);
                                }
                            });
                    },
                );
            });

            ui.separator();
            if components::button(ui, true, "Close", ButtonSize::Medium.with_width(140.0)) {
                self.programmer_ui_state.animation_presets.open = false;
                self.programmer_ui_state.animation_presets.save = None;
            }

            // Nested, so the form gets the higher modal order.
            self.render_save_animation_preset_dialog(ui.ctx(), templates);
        });

        if response.cancel_requested && !save_open {
            self.programmer_ui_state.animation_presets.open = false;
        }
    }

    /// "Save as preset": names the new template (prefilled with a free name).
    fn render_save_animation_preset_dialog(
        &mut self,
        ctx: &Context,
        templates: &BTreeMap<u8, AnimationTemplate>,
    ) {
        let Some(save) = self.programmer_ui_state.animation_presets.save.as_mut() else {
            return;
        };

        const WIDTH: f32 = 380.0;
        const HEIGHT: f32 = 180.0;

        let name = save.name.trim().to_string();
        let name_taken = templates
            .values()
            .any(|template| template.spec.name == name);
        let mut close = false;
        let mut confirm = false;

        let response = Dialog::new(
            "Save Animation Preset".to_string(),
            egui::vec2(WIDTH, HEIGHT),
        )
        .with_backdrop()
        .show(ctx, |ui| {
            ui.heading("Save as preset");
            ui.label("Saves this animation as a new template.");
            ui.add_space(6.0);
            TextInput::new(WIDTH - 20.0)
                .with_monospace_font(false)
                .with_hint_text("Name")
                .ui(ui, &mut save.name);
            if name_taken {
                ui.label(RichText::new("A template with this name exists.").small());
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if components::button(ui, false, "Cancel", ButtonSize::Medium.with_width(140.0)) {
                    close = true;
                }
                if components::action_button(
                    ui,
                    !name.is_empty() && !name_taken,
                    "Save",
                    ButtonSize::Medium.with_width(140.0),
                    Some(if name.is_empty() {
                        "Enter a name"
                    } else {
                        "Name taken"
                    }),
                ) {
                    confirm = true;
                }
            });
        });

        if confirm {
            if let Some(save) = self.programmer_ui_state.animation_presets.save.take() {
                self.save_animation_as_preset(&save);
            }
        } else if close || response.cancel_requested {
            self.programmer_ui_state.animation_presets.save = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ensure_preset_template, preset_template_id, save_as_template, saved_templates,
        selection_animations, toggle_animation_event, unique_template_name,
    };
    use crate::{dmx::DmxEngine, event::SystemEventBus, state::AppState};
    use blaulicht_shared::{
        fixture::{
            light::Light,
            state::{Fixture, FixtureGroup},
            FixtureType,
        },
        AnimationPreset, AnimationSpec, AnimationTemplate, ControlEvent, ControlEventMessage,
        EventOriginator,
    };
    use std::{collections::BTreeMap, sync::Arc};

    fn template(name: &str) -> AnimationTemplate {
        let mut spec = AnimationSpec::empty();
        spec.name = name.to_string();
        AnimationTemplate { spec }
    }

    #[test]
    fn a_preset_reuses_the_template_named_after_it() {
        let mut templates = BTreeMap::from([(0, template("Other")), (3, template("Kick Flash"))]);
        assert_eq!(
            ensure_preset_template(&mut templates, AnimationPreset::KickFlash),
            Some(3)
        );
        assert_eq!(templates.len(), 2);
    }

    #[test]
    fn a_preset_without_template_gets_one_at_the_lowest_free_id() {
        let mut templates = BTreeMap::from([(0, template("Other")), (2, template("More"))]);
        let id = ensure_preset_template(&mut templates, AnimationPreset::HueDrift).unwrap();
        assert_eq!(id, 1);
        assert_eq!(templates[&1].spec.name, "Hue Drift");
        assert_eq!(
            templates[&1].spec.property,
            AnimationPreset::HueDrift.property()
        );
        // Applying it again reuses it.
        assert_eq!(
            ensure_preset_template(&mut templates, AnimationPreset::HueDrift),
            Some(1)
        );
        assert_eq!(templates.len(), 3);
    }

    #[test]
    fn no_template_is_created_when_all_ids_are_taken() {
        let mut templates: BTreeMap<u8, AnimationTemplate> =
            (0..=u8::MAX).map(|id| (id, template("Full"))).collect();
        assert_eq!(
            ensure_preset_template(&mut templates, AnimationPreset::KickFlash),
            None
        );
        assert_eq!(
            save_as_template(&mut templates, AnimationSpec::empty(), "X"),
            None
        );
    }

    #[test]
    fn saving_keeps_the_spec_and_sets_the_trimmed_name() {
        let mut templates = BTreeMap::from([(0, template("A"))]);
        let spec = AnimationSpec::preset(AnimationPreset::TempoSweep);
        let id = save_as_template(&mut templates, spec, "  My Sweep ").unwrap();
        assert_eq!(id, 1);
        assert_eq!(templates[&1].spec.name, "My Sweep");
        assert_eq!(
            templates[&1].spec.property,
            AnimationPreset::TempoSweep.property()
        );
    }

    #[test]
    fn suggested_names_do_not_collide() {
        let templates =
            BTreeMap::from([(0, template("Kick Flash")), (1, template("Kick Flash 2"))]);
        assert_eq!(
            unique_template_name(&templates, "Kick Flash"),
            "Kick Flash 3"
        );
        assert_eq!(unique_template_name(&templates, "New"), "New");
        assert_eq!(unique_template_name(&templates, "  "), "Animation");
    }

    #[test]
    fn saved_templates_leave_out_preset_named_ones() {
        let templates = BTreeMap::from([(0, template("Kick Flash")), (1, template("My Chase"))]);
        let saved: Vec<u8> = saved_templates(&templates)
            .iter()
            .map(|(id, ..)| *id)
            .collect();
        assert_eq!(saved, vec![1]);
        assert_eq!(
            preset_template_id(&templates, AnimationPreset::KickFlash),
            Some(0)
        );
        assert_eq!(
            preset_template_id(&templates, AnimationPreset::BassPump),
            None
        );
    }

    #[test]
    fn unnamed_templates_are_labelled_by_id() {
        let templates = BTreeMap::from([(4, template(" ")), (5, template("Color"))]);
        let labels: Vec<String> = saved_templates(&templates)
            .into_iter()
            .map(|(_, label, _)| label)
            .collect();
        assert_eq!(labels, vec!["#4".to_string(), "Color".to_string()]);
    }

    #[test]
    fn a_tap_toggles_the_animation() {
        let ControlEvent::Transaction(events) = toggle_animation_event(4, false) else {
            panic!("applying is a transaction");
        };
        assert!(matches!(
            events.as_slice(),
            [
                ControlEvent::AddAnimation(4),
                ControlEvent::PlayAnimation(4)
            ]
        ));
        assert!(matches!(
            toggle_animation_event(4, true),
            ControlEvent::RemoveAnimation(4)
        ));
    }

    /// Runs the dialog's events through the engine: a preset applied to a
    /// limited selection shows up under that selection in the focused scene
    /// only, and the second tap removes it again.
    #[test]
    fn applying_and_removing_a_preset_goes_through_the_engine() {
        let state = Arc::new(AppState::new(&[]));
        let bus = SystemEventBus::new();
        let engine = DmxEngine::new(
            state.clone(),
            bus.new_connection(),
            crate::hub::SystemHub::new(),
            Default::default(),
        );
        let send = |event: ControlEvent| {
            let mut guard = state.dmx_engine.write().unwrap();
            engine.apply(
                &mut guard,
                ControlEventMessage::new(EventOriginator::Web, event),
            )
        };

        {
            let mut guard = state.dmx_engine.write().unwrap();
            let fixtures = (0..4u8)
                .map(|fid| {
                    (
                        fid,
                        Fixture::new(
                            0,
                            1 + 3 * fid as usize,
                            format!("F{fid}"),
                            FixtureType::from(Light::Generic3ChanNoAlpha),
                        ),
                    )
                })
                .collect();
            guard.0.groups.insert(
                0,
                FixtureGroup {
                    name: "G".to_string(),
                    fixtures,
                    folder: None,
                },
            );
            guard.0.new_scene("Focused".to_string());
            guard.0.new_scene("Other".to_string());
            guard.0.current_scene_focus = 0;
        }
        for event in [
            ControlEvent::SelectGroup(0),
            ControlEvent::LimitSelectionToFixtureInCurrentGroup(3),
            ControlEvent::LimitSelectionToFixtureInCurrentGroup(1),
        ] {
            assert_eq!(send(event).0, None);
        }

        let id = {
            let mut guard = state.dmx_engine.write().unwrap();
            ensure_preset_template(&mut guard.0.animation_templates, AnimationPreset::KickFlash)
                .unwrap()
        };
        assert_eq!(send(toggle_animation_event(id, false)).0, None);

        {
            let guard = state.dmx_engine.read().unwrap();
            let selection = guard.get_selection();
            let animations = selection_animations(guard.curr_scene(), &selection).unwrap();
            assert_eq!(animations.keys().copied().collect::<Vec<_>>(), vec![id]);
            assert_eq!(animations[&id].spec_cloned.name, "Kick Flash");
            assert!(animations[&id].enabled, "an applied preset runs");
            assert!(guard.0.scenes[&1].sink.active_animations.is_empty());
        }

        assert_eq!(send(toggle_animation_event(id, true)).0, None);
        let guard = state.dmx_engine.read().unwrap();
        assert!(selection_animations(guard.curr_scene(), &guard.get_selection()).is_none());
    }
}
