//! The standard sub-page tab bar. Pages render it through
//! `BlaulichtApp::subpage_ui` (`app/page.rs`), which also keeps the selection
//! in the navbar state; the functions here only lay out and draw it.

use crate::app::components::{self, ButtonSize};
use blaulicht_shared::AppSubPage;
use egui::Ui;

const TAB_GAP: f32 = 2.0;
const TAB_MIN_WIDTH: f32 = 86.0;
const TAB_DIVIDER_HEIGHT: f32 = 1.0;

/// Number of tab buttons that fit into one row at the given width.
fn subpage_tab_columns(width: f32, count: usize) -> usize {
    (((width.max(TAB_MIN_WIDTH) + TAB_GAP) / (TAB_MIN_WIDTH + TAB_GAP)).floor() as usize)
        .clamp(1, count.max(1))
}

/// Exact height of the bottom tab bar: divider, the gap below it, and the
/// wrapped button rows. Must stay in sync with [`subpage_tabs_layout`].
pub fn subpage_tab_bar_height(width: f32, count: usize) -> f32 {
    if count == 0 {
        return 0.0;
    }
    let rows = count.div_ceil(subpage_tab_columns(width, count));
    TAB_DIVIDER_HEIGHT
        + TAB_GAP
        + rows as f32 * ButtonSize::Medium.dim().0.y
        + rows.saturating_sub(1) as f32 * TAB_GAP
}

/// Draws one button per sub-page, wrapping into rows of equal width. Returns
/// the clicked tab if it is not already `active`.
pub fn subpage_tab_bar(
    ui: &mut Ui,
    subpages: &[AppSubPage],
    active: AppSubPage,
) -> Option<AppSubPage> {
    let available = ui.available_width();
    let columns = subpage_tab_columns(available, subpages.len());
    let tab_width = ((available - TAB_GAP * columns.saturating_sub(1) as f32) / columns as f32)
        .max(TAB_MIN_WIDTH);

    let mut clicked = None;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(TAB_GAP, TAB_GAP);
        for &subpage in subpages {
            let is_active = subpage == active;
            if components::Button::new(subpage.label(), ButtonSize::Medium.with_width(tab_width))
                .ui(ui, is_active)
                && !is_active
            {
                clicked = Some(subpage);
            }
        }
    });
    clicked
}

/// Takes all remaining space of `ui`, draws a divider and the tab bar along
/// its bottom edge, and returns a clipped child `Ui` for the content above
/// it, plus the tab clicked this frame (see [`subpage_tab_bar`]).
pub fn subpage_tabs_layout(
    ui: &mut Ui,
    subpages: &[AppSubPage],
    active: AppSubPage,
) -> (Ui, Option<AppSubPage>) {
    let available_size = ui.available_size().max(egui::vec2(1.0, 1.0));
    let (page_rect, _) = ui.allocate_exact_size(available_size, egui::Sense::hover());
    let tab_height =
        subpage_tab_bar_height(page_rect.width(), subpages.len()).min(page_rect.height());
    let tab_rect = egui::Rect::from_min_max(
        egui::pos2(page_rect.min.x, page_rect.max.y - tab_height),
        page_rect.max,
    );
    let content_rect =
        egui::Rect::from_min_max(page_rect.min, egui::pos2(page_rect.max.x, tab_rect.min.y));

    let mut tab_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(tab_rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    tab_ui.set_clip_rect(tab_rect);
    // Draw the divider manually so its height matches what
    // `subpage_tab_bar_height` reserves (a plain `separator()` adds style
    // dependent padding and would push the buttons out of the clip rect).
    tab_ui.spacing_mut().item_spacing.y = TAB_GAP;
    let (divider_rect, _) = tab_ui.allocate_exact_size(
        egui::vec2(tab_ui.available_width(), TAB_DIVIDER_HEIGHT),
        egui::Sense::hover(),
    );
    tab_ui.painter().hline(
        divider_rect.x_range(),
        divider_rect.center().y,
        tab_ui.visuals().widgets.noninteractive.bg_stroke,
    );
    let clicked = subpage_tab_bar(&mut tab_ui, subpages, active);

    let mut content_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(content_rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    content_ui.set_clip_rect(content_rect);
    (content_ui, clicked)
}

#[cfg(test)]
mod tests {
    use super::{subpage_tab_bar_height, subpage_tab_columns};

    #[test]
    fn tab_bar_reserves_one_or_more_content_sized_rows() {
        let wide = subpage_tab_bar_height(800.0, 7);
        let narrow = subpage_tab_bar_height(280.0, 7);

        assert!(wide > 20.0 && wide < 60.0);
        assert!(narrow > wide);
        assert!(narrow < 160.0);
    }

    #[test]
    fn no_subpages_reserve_no_space() {
        assert_eq!(subpage_tab_bar_height(800.0, 0), 0.0);
    }

    #[test]
    fn columns_never_exceed_the_tab_count() {
        assert_eq!(subpage_tab_columns(2_000.0, 3), 3);
        assert_eq!(subpage_tab_columns(10.0, 3), 1);
    }
}
