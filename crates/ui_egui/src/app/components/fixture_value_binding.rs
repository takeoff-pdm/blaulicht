use std::collections::BTreeMap;

use blaulicht_shared::{fixture::value::FixtureValue, palette::Palette, FixtureProperty};
use egui::Context;

use crate::app::components::{self, id_selection_dialog, ButtonSize};

/// Tracks the palette-picker dialogs for a given slot. Keep one per slot you
/// render. Binding is a two-step flow: first pick the palette, then — when the
/// palette covers more than one property — pick which property to read out.
#[derive(Default, Clone, Copy)]
pub struct PaletteBindingState {
    pub picker_open: bool,
    pub prop_picker_open: bool,
    /// Palette chosen in the first step, awaiting a property choice.
    pub pending_palette_id: Option<u8>,
}

/// Returns the resolved literal value for `value`, given current palettes.
/// Use this to drive a slider/numberpad that operates on raw `u16`.
pub fn resolved(
    value: FixtureValue,
    palettes: &BTreeMap<u8, Palette>,
    property: FixtureProperty,
) -> u16 {
    value.resolve(palettes, property)
}

/// Outcome of interacting with the binding button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteBindingAction {
    None,
    /// Bind to `palette_id`. `property` optionally pins which property of the
    /// (multi-property) palette to read; `None` resolves in the slot's context.
    Bind {
        palette_id: u8,
        property: Option<FixtureProperty>,
    },
    Unbind,
}

/// Renders a small button next to a slider. Click → opens a palette picker.
/// While bound, button is highlighted and shows palette name.
pub fn palette_binding_button(
    ui: &mut egui::Ui,
    ctx: &Context,
    state: &mut PaletteBindingState,
    value: FixtureValue,
    palettes: &BTreeMap<u8, Palette>,
    dialog_label: String,
) -> PaletteBindingAction {
    let label = match value {
        FixtureValue::Literal(_) => "P".to_string(),
        FixtureValue::PalettePointer {
            palette_id,
            property,
        } => {
            let name = match palettes.get(&palette_id) {
                Some(p) => short(&p.name),
                None => format!("?{palette_id}"),
            };
            let icon = egui_phosphor::regular::LETTER_CIRCLE_P;
            match property {
                Some(p) => format!("{icon} {name}·{p}"),
                None => format!("{icon} {name}"),
            }
        }
    };

    let is_active = matches!(value, FixtureValue::PalettePointer { .. });

    if components::button(ui, is_active, &label, ButtonSize::Medium.with_width(60.0)) {
        state.picker_open = true;
    }

    // Step two: the palette covers more than one property, so let the user
    // pick which one to read out.
    if state.prop_picker_open {
        if let Some(palette_id) = state.pending_palette_id {
            let props = palettes
                .get(&palette_id)
                .map(|p| p.kind.properties(palettes))
                .unwrap_or_default();

            let current = value
                .palette_property()
                .unwrap_or_else(|| props.first().copied().unwrap_or(FixtureProperty::Alpha));

            let (selected, changed) = components::selection_dialog(
                ctx,
                props,
                current,
                &mut state.prop_picker_open,
                format!("{dialog_label} — property"),
            );

            if changed {
                state.pending_palette_id = None;
                return PaletteBindingAction::Bind {
                    palette_id,
                    property: Some(selected),
                };
            }
        }
        if !state.prop_picker_open {
            state.pending_palette_id = None;
        }
        return PaletteBindingAction::None;
    }

    if !state.picker_open {
        return PaletteBindingAction::None;
    }

    // Build the (palette_id, display-name) list. Insert a synthetic
    // "Unbind" item with id `u8::MAX` so the user can clear the binding.
    let mut entries: Vec<(u8, String)> = palettes
        .iter()
        .map(|(id, p)| (*id, format!("{id}: {}", p.name)))
        .collect();

    const UNBIND_KEY: u8 = u8::MAX;
    entries.push((UNBIND_KEY, "<unbind>".to_string()));

    let current = value.palette_id().unwrap_or(UNBIND_KEY);

    let (selected, changed) =
        id_selection_dialog(ctx, entries, current, &mut state.picker_open, dialog_label);

    if !changed {
        return PaletteBindingAction::None;
    }

    if selected == UNBIND_KEY {
        return PaletteBindingAction::Unbind;
    }

    // Decide whether a property choice is needed. Single-property palettes
    // bind immediately; multi-property ones open the property picker.
    let props = palettes
        .get(&selected)
        .map(|p| p.kind.properties(palettes))
        .unwrap_or_default();

    if props.len() > 1 {
        state.pending_palette_id = Some(selected);
        state.prop_picker_open = true;
        PaletteBindingAction::None
    } else {
        PaletteBindingAction::Bind {
            palette_id: selected,
            property: props.first().copied(),
        }
    }
}

fn short(s: &str) -> String {
    const MAX: usize = 10;
    if s.chars().count() <= MAX {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(MAX).collect();
        format!("{truncated}…")
    }
}
