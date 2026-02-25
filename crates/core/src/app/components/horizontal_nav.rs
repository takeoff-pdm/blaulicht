use egui::{Button, Separator, Ui, Widget};

use crate::app::components::{self, ButtonSize};

pub fn horizontal_nav(ui: &mut Ui) {
    let items = vec!["DEBUG", "MAIN", "FOO"];

    let sep_spacing = 3.0;
    let sep_total = sep_spacing * (items.len().saturating_sub(1)) as f32;
    let item_width = (ui.available_width() - sep_total) / items.len() as f32;

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;

        for (idx, item) in items.iter().enumerate() {
            let is_last = idx == items.len() - 1;

            // item.
            if components::Button::new(item, ButtonSize::Medium.with_width(item_width))
                .ui(ui, false)
            {}

            if !is_last {
                Separator::default().spacing(sep_spacing).ui(ui);
            }
        }
    });
}
