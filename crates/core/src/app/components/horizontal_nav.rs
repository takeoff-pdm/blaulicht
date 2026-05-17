use crate::app::components::{self, ButtonSize};
use blaulicht_shared::AppPage;
use egui::{Separator, Ui, Widget};

pub fn horizontal_nav_for_tab(ui: &mut Ui, page: AppPage) {
    let items = vec!["DEBUG", "MAIN", "FOO"];

    let sep_spacing = 3.0;
    let sep_total = sep_spacing * (items.len().saturating_sub(1)) as f32;
    let item_width = (ui.available_width() - sep_total) / items.len() as f32;
    let subpages = page.list_subpages();

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;

        for (idx, item) in subpages.iter().enumerate() {
            let is_last = idx == items.len() - 1;

            // item.
            if components::Button::new(
                item.to_string().as_str(),
                ButtonSize::Medium.with_width(item_width),
            )
            .ui(ui, false)
            {}

            if !is_last {
                Separator::default().spacing(sep_spacing).ui(ui);
            }
        }
    });
}
