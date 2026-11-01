//! The desktop layout's navigation sidebar: one row per page (icon and name),
//! pages with sub-pages as foldable sections with their sub-pages indented
//! underneath, and the startup status at the bottom. It only draws and
//! reports clicks; [`Navbar::sidebar_click`] applies them.

use blaulicht_shared::{AppPage, AppSubPage};
use egui::{
    collapsing_header::CollapsingState,
    text::{LayoutJob, TextWrapping},
    Align2, FontId, Sense, Ui,
};

use super::navbar::{app_page_label, app_page_to_icon, status_style};
use super::{Navbar, NavbarStatus, SidebarClick};
use crate::app::startup::StartupIndicator;

/// Width of the sidebar until the user drags its edge, and the range it can
/// be dragged in.
pub const DESKTOP_SIDEBAR_WIDTH: f32 = 200.0;
pub const DESKTOP_SIDEBAR_WIDTH_RANGE: std::ops::RangeInclusive<f32> = 150.0..=320.0;

const ROW_HEIGHT: f32 = 28.0;
const ICON_COLUMN: f32 = 26.0;

/// Draws the sidebar as a resizable left panel of `ui` and returns this
/// frame's click. `status`, if given, adds the startup status slot at the
/// bottom (it opens Logs).
pub fn desktop_sidebar(
    ui: &mut Ui,
    navbar: &Navbar,
    status: Option<NavbarStatus<'_>>,
) -> Option<SidebarClick> {
    let mut click = None;
    egui::Panel::left("desktop_nav")
        .resizable(true)
        .default_size(DESKTOP_SIDEBAR_WIDTH)
        .size_range(DESKTOP_SIDEBAR_WIDTH_RANGE)
        .show_inside(ui, |ui| {
            ui.add_space(6.0);
            ui.label(egui::RichText::new("Blaulicht").heading().strong());
            ui.add_space(2.0);
            ui.separator();

            if let Some(status) = &status {
                egui::Panel::bottom("desktop_nav_status")
                    .resizable(false)
                    .show_separator_line(true)
                    .frame(egui::Frame::NONE.inner_margin(egui::Margin::symmetric(0, 6)))
                    .show_inside(ui, |ui| {
                        if status_row(ui, status) {
                            click = Some(SidebarClick::Status);
                        }
                    });
            }

            // Fixed id scopes for the list and each entry: auto ids under
            // the root `Ui` shift between egui's passes of a frame (the
            // first frame has two), which egui warns about for the scroll
            // area and the rows.
            let list_scope = egui::UiBuilder::new()
                .id_salt(egui::Id::new("desktop_nav_list"))
                .global_scope(true);
            ui.scope_builder(list_scope, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        for page in AppPage::navbar_pages() {
                            let scope = egui::UiBuilder::new()
                                .id_salt(egui::Id::new(("desktop_nav_entry", page)))
                                .global_scope(true);
                            let entry = ui.scope_builder(scope, |ui| page_entry(ui, navbar, page));
                            if let Some(page_click) = entry.inner {
                                click = Some(page_click);
                            }
                        }
                    });
            });
        });
    click
}

/// One page: a plain row, or a foldable section for a page with sub-pages.
fn page_entry(ui: &mut Ui, navbar: &Navbar, page: AppPage) -> Option<SidebarClick> {
    let current = navbar.page() == page;
    let subpages = page.list_subpages();
    let icon = app_page_to_icon(&page);
    let label = app_page_label(page);

    if subpages.is_empty() {
        let response = ui
            .horizontal(|ui| {
                // Where a section's fold arrow is, so all icons line up.
                ui.add_space(ui.spacing().indent);
                nav_row(ui, page_row_id(page), icon, label, current)
            })
            .inner;
        return response.clicked().then_some(SidebarClick::Page(page));
    }

    let expanded = navbar.is_expanded(page);
    let id = ui.make_persistent_id(("desktop_nav_section", page));
    let mut state = CollapsingState::load_with_default_open(ui.ctx(), id, expanded);
    // The navbar owns the folding; egui's state only animates it.
    state.set_open(expanded);

    let mut click = None;
    let (toggle, header, _) = state
        .show_header(ui, |ui| {
            // The current page's row is highlighted while its section is
            // folded; unfolded, its sub-page row is.
            nav_row(ui, page_row_id(page), icon, label, current && !expanded)
        })
        .body(|ui| {
            for &subpage in subpages {
                let selected = current && navbar.subpage(page) == Some(subpage);
                if subpage_row(ui, subpage, selected).clicked() {
                    click = Some(SidebarClick::SubPage(subpage));
                }
            }
        });
    if toggle.clicked() {
        click = Some(SidebarClick::ToggleSection(page));
    }
    if header.inner.clicked() {
        click = Some(SidebarClick::Page(page));
    }
    click
}

/// A sub-page row: indented by the section body, its label in line with the
/// page names (an empty icon column).
fn subpage_row(ui: &mut Ui, subpage: AppSubPage, selected: bool) -> egui::Response {
    let id = egui::Id::new(("desktop_nav_subpage", subpage));
    nav_row(ui, id, "", subpage.label(), selected)
}

/// Rows have fixed ids rather than auto ids, which change between egui's
/// layout passes (the panel's sizing pass, folding animations).
fn page_row_id(page: AppPage) -> egui::Id {
    egui::Id::new(("desktop_nav_page", page))
}

/// A full-width selectable row with an icon column (`icon` may be empty),
/// styled like egui's selectable labels (selection fill when selected, hover
/// fill otherwise). The label is cut with "…" when the sidebar is too narrow.
fn nav_row(ui: &mut Ui, id: egui::Id, icon: &str, label: &str, selected: bool) -> egui::Response {
    let width = ui.available_width().max(ROW_HEIGHT);
    let (_, rect) = ui.allocate_space(egui::vec2(width, ROW_HEIGHT));
    let response = ui.interact(rect, id, Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);

    if ui.is_rect_visible(rect) {
        let visuals = ui.style().interact_selectable(&response, selected);
        let painter = ui.painter_at(rect);
        if selected || response.hovered() || response.has_focus() {
            painter.rect_filled(rect, visuals.corner_radius, visuals.weak_bg_fill);
        }
        let color = visuals.text_color();
        let x = rect.left() + 6.0;
        painter.text(
            egui::pos2(x, rect.center().y),
            Align2::LEFT_CENTER,
            icon,
            FontId::proportional(16.0),
            color,
        );
        let x = x + ICON_COLUMN;
        let mut job =
            LayoutJob::simple_singleline(label.to_owned(), FontId::proportional(14.0), color);
        job.wrap = TextWrapping::truncate_at_width((rect.right() - x - 4.0).max(0.0));
        let galley = painter.layout_job(job);
        let pos = egui::pos2(x, rect.center().y - galley.size().y / 2.0);
        painter.galley(pos, galley, color);
    }

    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, selected, label)
    });
    response
}

/// The startup status slot: icon and a short caption in the status color,
/// the summary on hover. Returns whether it was clicked.
fn status_row(ui: &mut Ui, status: &NavbarStatus<'_>) -> bool {
    let (icon, _, color) = status_style(status.indicator);
    let caption = match status.indicator {
        StartupIndicator::Failed => "Startup failed",
        StartupIndicator::Restarting => "Restarting...",
        StartupIndicator::Running => "Starting...",
    };
    let width = ui.available_width();
    let (_, rect) = ui.allocate_space(egui::vec2(width, ROW_HEIGHT));
    let response = ui.interact(rect, egui::Id::new("desktop_nav_status"), Sense::click());
    let painter = ui.painter_at(rect);
    let fill = if response.hovered() {
        color.gamma_multiply(0.3)
    } else {
        color.gamma_multiply(0.15)
    };
    painter.rect_filled(rect, 3.0, fill);
    let x = rect.left() + 6.0 + ui.spacing().indent;
    painter.text(
        egui::pos2(x, rect.center().y),
        Align2::LEFT_CENTER,
        icon,
        FontId::proportional(16.0),
        color,
    );
    painter.text(
        egui::pos2(x + ICON_COLUMN, rect.center().y),
        Align2::LEFT_CENTER,
        caption,
        FontId::proportional(14.0),
        color,
    );
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(format!("{}\nClick to open Logs.", status.summary))
        .clicked()
}

#[cfg(test)]
mod tests {
    use super::{desktop_sidebar, page_row_id};
    use crate::app::components::{Navbar, NavbarStatus, SidebarClick};
    use crate::app::startup::StartupIndicator;
    use blaulicht_shared::{AppPage, AppSubPage};
    use egui::{Event, PointerButton, Pos2, RawInput, Rect};

    /// Runs one frame of the sidebar with `events` and returns its click.
    fn frame(
        ctx: &egui::Context,
        navbar: &Navbar,
        status: Option<StartupIndicator>,
        events: Vec<Event>,
    ) -> Option<SidebarClick> {
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1000.0, 700.0))),
            events,
            ..Default::default()
        };
        let mut click = None;
        let _ = ctx.run_ui(input, |ui| {
            let status = status.map(|indicator| NavbarStatus {
                indicator,
                summary: "audio device failed",
            });
            click = desktop_sidebar(ui, navbar, status).or(click);
        });
        click
    }

    /// Lays the sidebar out, then clicks the center of the widget `id`.
    fn click(
        navbar: &Navbar,
        status: Option<StartupIndicator>,
        id: egui::Id,
    ) -> Option<SidebarClick> {
        let ctx = egui::Context::default();
        frame(&ctx, navbar, status, vec![]);
        frame(&ctx, navbar, status, vec![]);
        let pos = ctx
            .read_response(id)
            .unwrap_or_else(|| panic!("no widget {id:?}"))
            .rect
            .center();
        let button = |pressed| Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        frame(&ctx, navbar, status, vec![Event::PointerMoved(pos)]);
        frame(&ctx, navbar, status, vec![button(true)]);
        frame(&ctx, navbar, status, vec![button(false)])
    }

    #[test]
    fn clicking_rows_reports_pages_and_subpages() {
        let mut navbar = Navbar::new(AppPage::Logs);
        navbar.sync_sections();
        assert_eq!(
            click(&navbar, None, page_row_id(AppPage::Programmer)),
            Some(SidebarClick::Page(AppPage::Programmer))
        );
        assert_eq!(
            click(&navbar, None, page_row_id(AppPage::System)),
            Some(SidebarClick::Page(AppPage::System))
        );

        // Sub-page rows exist only in an unfolded section.
        let dmx = egui::Id::new(("desktop_nav_subpage", AppSubPage::System_Dmx));
        navbar.navigate_to(AppPage::System);
        navbar.sync_sections();
        assert_eq!(
            click(&navbar, None, dmx),
            Some(SidebarClick::SubPage(AppSubPage::System_Dmx))
        );
    }

    #[test]
    fn startup_status_slot_opens_logs() {
        let navbar = Navbar::new(AppPage::Programmer);
        assert_eq!(
            click(
                &navbar,
                Some(StartupIndicator::Failed),
                egui::Id::new("desktop_nav_status")
            ),
            Some(SidebarClick::Status)
        );
    }
}
