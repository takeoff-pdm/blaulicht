use egui::{Context, Vec2};
use std::fmt::Display;

use crate::app::components::{ButtonSize, Dialog};

// Returns an option value and if it was changed.
pub fn selection_dialog<I, T>(
    ctx: &Context,
    options: I,
    current_selection: T,
    is_open: &mut bool,
    label: String,
) -> (T, bool)
where
    I: IntoIterator<Item = T>,
    I: Clone,
    T: Display,
    T: PartialEq,
    T: Eq,
    T: Clone,
{
    let vpadding = 5.0;
    let options_len = options.clone().into_iter().count();

    let height = ((options_len + 2) as f32 * (ButtonSize::Medium.dim().0.y + vpadding)) - vpadding;

    let width = 300.0;
    let dialog_dim = Vec2::new(width, height);

    let button_size = ButtonSize::Custom(
        width * 2.0 / 3.0,
        ButtonSize::Medium.dim().0.y,
        ButtonSize::Medium.dim().1,
    );

    let mut selection = current_selection.clone();
    let mut changed = false;

    Dialog::new(label, dialog_dim)
        .with_backdrop()
        .show(ctx, |ui| {
            for (idx, option) in options.into_iter().enumerate() {
                if super::button(
                    ui,
                    current_selection == option,
                    &option.to_string(),
                    button_size,
                ) {
                    selection = option;
                    changed = true;
                    *is_open = false;
                }

                if idx + 1 < options_len {
                    ui.add_space(vpadding);
                }
            }

            ui.add_space(vpadding);

            ui.separator();

            ui.add_space(vpadding);

            if super::button(ui, false, "Close", button_size) {
                *is_open = false;
            }
        });

    (selection, changed)
}
