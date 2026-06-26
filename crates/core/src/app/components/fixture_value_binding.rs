use std::collections::BTreeMap;

use blaulicht_shared::{
    FixtureProperty,
    fixture::value::FixtureValue,
    palette::Palette,
};
use egui::Context;

use crate::app::components::{self, ButtonSize, id_selection_dialog};

/// Tracks whether the palette-picker dialog for a given (property, scope) is open.
/// Keep one per slot you render.
#[derive(Default, Clone, Copy)]
pub struct PaletteBindingState {
    pub picker_open: bool,
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
    Bind(u8),
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
        FixtureValue::PalettePointer { palette_id } => match palettes.get(&palette_id) {
            Some(p) => format!("◆ {}", short(&p.name)),
            None => format!("◆ ?{palette_id}"),
        },
    };

    let is_active = matches!(value, FixtureValue::PalettePointer { .. });

    if components::button(ui, is_active, &label, ButtonSize::Medium.with_width(60.0)) {
        state.picker_open = true;
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
        PaletteBindingAction::Unbind
    } else {
        PaletteBindingAction::Bind(selected)
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
