use egui::{Context, Vec2};
use std::fmt::Display;

use crate::app::components::{ButtonSize, Dialog, Pagination};

const ITEMS_PER_PAGE: usize = 5;

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
    T: Display,
    T: PartialEq,
    T: Clone,
{
    let page_memory_id = egui::Id::new(("components::selection_dialog_page", label.clone()));

    if !*is_open {
        ctx.data_mut(|data| {
            data.remove_temp::<usize>(page_memory_id);
        });
        return (current_selection, false);
    }

    let options_vec: Vec<T> = options.into_iter().collect();

    let vpadding = 5.0;
    let options_len = options_vec.len();

    let width = 300.0;

    let button_size = ButtonSize::Custom(
        width * 2.0 / 3.0,
        ButtonSize::Medium.dim().0.y,
        ButtonSize::Medium.dim().1,
    );

    let button_height = button_size.dim().0.y;
    let visible_items = options_len.min(ITEMS_PER_PAGE);
    let item_block_height = if visible_items == 0 {
        button_height
    } else {
        visible_items as f32 * (button_height + vpadding) - vpadding
    };

    let nav_block_height = ButtonSize::Medium.dim().0.y + vpadding * 3.0;
    let close_block_height = button_height + vpadding * 3.0;
    let separator_height = 2.0;

    let height = nav_block_height + item_block_height + close_block_height + separator_height;

    let dialog_dim = Vec2::new(width, height.max(200.0));

    let mut selection = current_selection.clone();
    let mut changed = false;

    Dialog::new(label, dialog_dim)
        .with_backdrop()
        .show(ctx, |ui| {
            let mut reset_page_state = false;

            let mut pagination = Pagination::default()
                .with_items_per_page(ITEMS_PER_PAGE)
                .with_ui_width(width);

            let stored_page_index = ui
                .ctx()
                .data(|data| data.get_temp::<usize>(page_memory_id))
                .unwrap_or(0);
            pagination.set_current_page(stored_page_index);

            let page_items: Vec<&T> = pagination
                .prepare_current_page_items(&options_vec)
                .collect();
            let page_len = page_items.len();

            pagination.ui(ui, |ui| {
                ui.add_space(vpadding);

                if page_len == 0 {
                    ui.label("No options available");
                    return;
                }

                for (idx, option) in page_items.iter().enumerate() {
                    let option_ref = *option;
                    let is_active = selection == *option_ref;
                    let label = option_ref.to_string();

                    if super::button(ui, is_active, &label, button_size) {
                        selection = option_ref.clone();
                        changed = true;
                        *is_open = false;
                        reset_page_state = true;
                    }

                    if idx + 1 < page_len {
                        ui.add_space(vpadding);
                    }
                }
            });

            if !reset_page_state {
                let current_page_index = pagination.current_page();
                ui.ctx()
                    .data_mut(|data| data.insert_temp(page_memory_id, current_page_index));
            }

            ui.add_space(vpadding);

            ui.separator();

            ui.add_space(vpadding);

            if super::button(ui, false, "Close", button_size) {
                *is_open = false;
                reset_page_state = true;
            }

            if reset_page_state {
                ui.ctx()
                    .data_mut(|data| data.remove_temp::<usize>(page_memory_id));
            }
        });

    (selection, changed)
}
