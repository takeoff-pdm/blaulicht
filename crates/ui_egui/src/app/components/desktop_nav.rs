//! The desktop layout's navigation sidebar, styled like a studio app's
//! browser: compact rows grouped under small section headers (PERFORM,
//! SETUP, SYSTEM), Phosphor icons in `muted` that turn `accent` on the
//! current entry, which also gets the `selection` fill and a 2px accent bar.
//! Pages with sub-pages are foldable sections with their sub-pages indented
//! underneath (an indent guide on the left). The header folds the sidebar to
//! an icon rail; the startup status sits at the bottom. It only draws and
//! reports clicks; [`Navbar::sidebar_click`] applies them.

use blaulicht_shared::{AppPage, AppSubPage};
use egui::{
    text::{LayoutJob, TextWrapping},
    Align2, Color32, Rect, Sense, Ui,
};

use super::navbar::{app_page_label, app_page_to_icon, status_style};
use super::{Navbar, NavbarStatus, SidebarClick};
use crate::app::startup::StartupIndicator;
use crate::app::ui_tokens::{self, Density, Weight};

/// Width of the sidebar until the user drags its edge, and the range it can
/// be dragged in.
pub const DESKTOP_SIDEBAR_WIDTH: f32 = 200.0;
pub const DESKTOP_SIDEBAR_WIDTH_RANGE: std::ops::RangeInclusive<f32> = 150.0..=320.0;
/// Width of the sidebar folded to an icon rail.
pub const DESKTOP_RAIL_WIDTH: f32 = 44.0;

/// Height of the header (logo, name, fold button).
const HEADER_HEIGHT: f32 = 40.0;
/// Left padding of the icons in the full sidebar.
const ROW_PAD: f32 = 12.0;
/// Width of the icon column; labels start after it.
const ICON_COLUMN: f32 = 24.0;
const ICON_SIZE: f32 = 15.0;
/// Width of a section's fold arrow, at the right end of its row.
const FOLD_WIDTH: f32 = 24.0;

/// The sidebar's groups in order. Pages missing here (a page added later)
/// are listed after them, so every navbar page stays reachable.
const GROUPS: &[(&str, &[AppPage])] = &[
    (
        "Perform",
        &[
            AppPage::Programmer,
            AppPage::ViewPerformance,
            AppPage::View,
            AppPage::SceneGraph,
            AppPage::Visualizer,
        ],
    ),
    ("Setup", &[AppPage::Setup, AppPage::Audio]),
    ("System", &[AppPage::System, AppPage::Logs]),
];

/// The sidebar's sections: a header and its navbar pages, in order.
pub(crate) fn sidebar_groups() -> Vec<(&'static str, Vec<AppPage>)> {
    let navbar: Vec<AppPage> = AppPage::navbar_pages().collect();
    let mut groups: Vec<(&'static str, Vec<AppPage>)> = GROUPS
        .iter()
        .map(|(name, pages)| {
            let pages = pages.iter().copied().filter(|p| navbar.contains(p));
            (*name, pages.collect())
        })
        .collect();
    let rest: Vec<AppPage> = navbar
        .into_iter()
        .filter(|page| !GROUPS.iter().any(|(_, pages)| pages.contains(page)))
        .collect();
    if !rest.is_empty() {
        groups.push(("More", rest));
    }
    groups.retain(|(_, pages)| !pages.is_empty());
    groups
}

/// Draws the sidebar as a left panel of `ui` (resizable, or the fixed icon
/// rail when [`Navbar::is_rail`]) and returns this frame's click. `status`,
/// if given, adds the startup status slot at the bottom (it opens Logs).
pub fn desktop_sidebar(
    ui: &mut Ui,
    navbar: &Navbar,
    status: Option<NavbarStatus<'_>>,
) -> Option<SidebarClick> {
    let rail = navbar.is_rail();
    let mut click = None;
    let frame = egui::Frame::NONE.fill(sidebar_fill(ui));
    // Separate panel ids, so the rail does not overwrite the width the user
    // dragged the full sidebar to.
    let panel = if rail {
        egui::Panel::left("desktop_nav_rail")
            .resizable(false)
            .exact_size(DESKTOP_RAIL_WIDTH)
    } else {
        egui::Panel::left("desktop_nav")
            .resizable(true)
            .default_size(DESKTOP_SIDEBAR_WIDTH)
            .size_range(DESKTOP_SIDEBAR_WIDTH_RANGE)
    };
    panel.frame(frame).show_inside(ui, |ui| {
        if header(ui, rail) {
            click = Some(SidebarClick::ToggleRail);
        }

        if let Some(status) = &status {
            egui::Panel::bottom("desktop_nav_status")
                .resizable(false)
                .show_separator_line(true)
                .frame(egui::Frame::NONE.inner_margin(egui::Margin::symmetric(4, 4)))
                .show_inside(ui, |ui| {
                    if status_row(ui, status, rail) {
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
                    ui.spacing_mut().item_spacing.y = 0.0;
                    for (index, (name, pages)) in sidebar_groups().into_iter().enumerate() {
                        group_header(ui, name, rail, index == 0);
                        for page in pages {
                            let scope = egui::UiBuilder::new()
                                .id_salt(egui::Id::new(("desktop_nav_entry", page)))
                                .global_scope(true);
                            let entry =
                                ui.scope_builder(scope, |ui| page_entry(ui, navbar, page, rail));
                            if let Some(page_click) = entry.inner {
                                click = Some(page_click);
                            }
                        }
                    }
                    ui.add_space(8.0);
                });
        });
    });
    click
}

/// The sidebar's background: `surface`, one step above the page.
fn sidebar_fill(ui: &Ui) -> Color32 {
    ui_tokens::palette(ui.ctx()).surface
}

/// Colours of the sidebar.
struct NavColors {
    text: Color32,
    strong: Color32,
    muted: Color32,
    accent: Color32,
    selection: Color32,
    hover: Color32,
    line: Color32,
}

fn nav_colors(ui: &Ui) -> NavColors {
    let color = ui_tokens::palette(ui.ctx());
    NavColors {
        text: color.text,
        strong: color.text_strong,
        muted: color.muted,
        accent: color.accent,
        selection: color.selection,
        hover: color.surface_raised,
        line: color.line_strong,
    }
}

/// The header: the logo mark and name, and the button that folds the
/// sidebar to the rail (or unfolds it). Returns whether that was clicked.
fn header(ui: &mut Ui, rail: bool) -> bool {
    let density = Density::of(ui.ctx());
    let colors = nav_colors(ui);
    let width = ui.available_width();
    let (_, rect) = ui.allocate_space(egui::vec2(width, HEADER_HEIGHT));
    let painter = ui.painter_at(rect);
    painter.hline(
        rect.x_range(),
        rect.bottom() - 0.5,
        egui::Stroke::new(ui_tokens::HAIRLINE, color_line(ui)),
    );

    let fold_rect = if rail {
        rect
    } else {
        Rect::from_min_max(
            egui::pos2(rect.right() - 32.0, rect.top()),
            rect.right_bottom(),
        )
    };
    if !rail {
        painter.text(
            egui::pos2(rect.left() + ROW_PAD, rect.center().y),
            Align2::LEFT_CENTER,
            egui_phosphor::regular::SIREN,
            density.font(Weight::Regular, 17.0),
            colors.accent,
        );
        painter.text(
            egui::pos2(rect.left() + ROW_PAD + ICON_COLUMN, rect.center().y),
            Align2::LEFT_CENTER,
            "Blaulicht",
            density.font(Weight::SemiBold, 15.0),
            colors.strong,
        );
    }

    let response = ui
        .interact(
            fold_rect,
            egui::Id::new("desktop_nav_fold_rail"),
            Sense::click(),
        )
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(if rail {
            "Expand the sidebar"
        } else {
            "Collapse the sidebar to icons"
        });
    let (icon, icon_color) = if rail {
        // The rail's header is the logo; hovering shows it unfolds.
        if response.hovered() {
            (egui_phosphor::regular::CARET_DOUBLE_RIGHT, colors.strong)
        } else {
            (egui_phosphor::regular::SIREN, colors.accent)
        }
    } else if response.hovered() {
        (egui_phosphor::regular::CARET_DOUBLE_LEFT, colors.strong)
    } else {
        (egui_phosphor::regular::CARET_DOUBLE_LEFT, colors.muted)
    };
    if response.hovered() && !rail {
        painter.rect_filled(
            fold_rect.shrink2(egui::vec2(4.0, 8.0)),
            ui_tokens::RADIUS,
            colors.hover,
        );
    }
    painter.text(
        fold_rect.center(),
        Align2::CENTER_CENTER,
        icon,
        density.font(Weight::Regular, if rail { 17.0 } else { 13.0 }),
        icon_color,
    );
    response.clicked()
}

fn color_line(ui: &Ui) -> Color32 {
    ui_tokens::palette(ui.ctx()).line
}

/// A group's small uppercase header; on the rail a short hairline instead.
fn group_header(ui: &mut Ui, name: &str, rail: bool, first: bool) {
    let density = Density::of(ui.ctx());
    let width = ui.available_width();
    if rail {
        let (_, rect) = ui.allocate_space(egui::vec2(width, if first { 8.0 } else { 13.0 }));
        if !first {
            ui.painter().hline(
                rect.shrink2(egui::vec2(10.0, 0.0)).x_range(),
                rect.center().y,
                egui::Stroke::new(ui_tokens::HAIRLINE, color_line(ui)),
            );
        }
        return;
    }
    let height = if first { 26.0 } else { 32.0 };
    let (_, rect) = ui.allocate_space(egui::vec2(width, height));
    let text = density.overline(name);
    let galley = egui::WidgetText::from(text).into_galley(
        ui,
        Some(egui::TextWrapMode::Truncate),
        rect.width() - ROW_PAD,
        egui::TextStyle::Small,
    );
    let pos = egui::pos2(rect.left() + ROW_PAD, rect.bottom() - 6.0 - galley.size().y);
    ui.painter().galley(pos, galley, nav_colors(ui).muted);
}

/// One page: a plain row, or a foldable section for a page with sub-pages.
/// On the rail every page is one icon (sub-pages are reached through it).
fn page_entry(ui: &mut Ui, navbar: &Navbar, page: AppPage, rail: bool) -> Option<SidebarClick> {
    let current = navbar.page() == page;
    let subpages = page.list_subpages();
    let icon = app_page_to_icon(&page);
    let label = app_page_label(page);

    if rail {
        let response = rail_row(ui, page_row_id(page), icon, label, current);
        return response.clicked().then_some(SidebarClick::Page(page));
    }

    if subpages.is_empty() {
        let state = if current {
            RowState::Selected
        } else {
            RowState::Idle
        };
        let row = nav_row(ui, page_row_id(page), icon, label, state, None);
        return row.response.clicked().then_some(SidebarClick::Page(page));
    }

    let expanded = navbar.is_expanded(page);
    // The current page's row is highlighted while its section is folded;
    // unfolded, its sub-page row is and the page's icon stays lit.
    let state = match (current, expanded) {
        (true, false) => RowState::Selected,
        (true, true) => RowState::InSection,
        (false, _) => RowState::Idle,
    };
    let row = nav_row(
        ui,
        page_row_id(page),
        icon,
        label,
        state,
        Some((page, expanded)),
    );

    let mut click = None;
    if row.response.clicked() {
        click = Some(SidebarClick::Page(page));
    }
    if row.fold.is_some_and(|fold| fold.clicked()) {
        click = Some(SidebarClick::ToggleSection(page));
    }
    if expanded {
        let top = ui.cursor().top();
        for &subpage in subpages {
            let selected = current && navbar.subpage(page) == Some(subpage);
            if subpage_row(ui, subpage, selected).clicked() {
                click = Some(SidebarClick::SubPage(subpage));
            }
        }
        // The indent guide, under the page's icon.
        let x = ui.max_rect().left() + ROW_PAD + ICON_SIZE / 2.0;
        let bottom = ui.cursor().top();
        ui.painter().vline(
            x.round() + 0.5,
            (top + 2.0)..=(bottom - 2.0),
            egui::Stroke::new(ui_tokens::HAIRLINE, nav_colors(ui).line),
        );
    }
    click
}

/// A sub-page row: indented under its page (labels in line with the page
/// names), no icon.
fn subpage_row(ui: &mut Ui, subpage: AppSubPage, selected: bool) -> egui::Response {
    let id = egui::Id::new(("desktop_nav_subpage", subpage));
    let state = if selected {
        RowState::Selected
    } else {
        RowState::Idle
    };
    nav_row(ui, id, "", subpage.label(), state, None).response
}

/// Rows have fixed ids rather than auto ids, which change between egui's
/// layout passes (the panel's sizing pass, folding animations).
fn page_row_id(page: AppPage) -> egui::Id {
    egui::Id::new(("desktop_nav_page", page))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RowState {
    Idle,
    /// The current entry: selection fill, accent bar and icon.
    Selected,
    /// A page whose current sub-page is shown below it: accent icon only.
    InSection,
}

struct NavRow {
    response: egui::Response,
    /// The fold arrow of a section row.
    fold: Option<egui::Response>,
}

/// Paints the state of a row: the selection fill and the 2px accent bar for
/// the current entry, a subtle fill under the pointer.
fn paint_row_background(ui: &Ui, rect: Rect, state: RowState, hovered: bool) {
    let colors = nav_colors(ui);
    let painter = ui.painter();
    if state == RowState::Selected {
        painter.rect_filled(rect, 0.0, colors.selection);
        let bar = Rect::from_min_size(
            rect.left_top(),
            egui::vec2(ui_tokens::ACCENT_BAR, rect.height()),
        );
        painter.rect_filled(bar, 0.0, colors.accent);
    } else if hovered {
        painter.rect_filled(rect, 0.0, colors.hover);
    }
}

/// A full-width row: icon column (`icon` may be empty, then the label still
/// starts after it), label cut with "…" when the sidebar is too narrow, and
/// for a section (`fold` = page and whether it is unfolded) a fold arrow at
/// the right end.
fn nav_row(
    ui: &mut Ui,
    id: egui::Id,
    icon: &str,
    label: &str,
    state: RowState,
    fold: Option<(AppPage, bool)>,
) -> NavRow {
    let density = Density::of(ui.ctx());
    let colors = nav_colors(ui);
    let height = nav_row_height(density);
    let width = ui.available_width().max(height);
    let (_, rect) = ui.allocate_space(egui::vec2(width, height));
    let response = ui
        .interact(rect, id, Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    // Interacted after the row, so it wins the pointer over its area.
    let fold_response = fold.map(|(page, _)| {
        let fold_rect = Rect::from_min_max(
            egui::pos2(rect.right() - FOLD_WIDTH, rect.top()),
            rect.right_bottom(),
        );
        ui.interact(
            fold_rect,
            egui::Id::new(("desktop_nav_fold", page)),
            Sense::click(),
        )
    });
    let hovered = response.hovered() || fold_response.as_ref().is_some_and(|r| r.hovered());

    if ui.is_rect_visible(rect) {
        paint_row_background(ui, rect, state, hovered);
        let painter = ui.painter_at(rect);
        let lit = state != RowState::Idle;
        let icon_color = if lit {
            colors.accent
        } else if hovered {
            colors.text
        } else {
            colors.muted
        };
        let text_color = if lit || hovered {
            colors.strong
        } else {
            colors.text
        };
        let x = rect.left() + ROW_PAD;
        painter.text(
            egui::pos2(x, rect.center().y),
            Align2::LEFT_CENTER,
            icon,
            density.font(Weight::Regular, ICON_SIZE),
            icon_color,
        );
        let x = x + ICON_COLUMN;
        let right = if fold.is_some() {
            rect.right() - FOLD_WIDTH
        } else {
            rect.right() - 6.0
        };
        let weight = if state == RowState::Selected {
            Weight::Medium
        } else {
            Weight::Regular
        };
        let mut job = LayoutJob::simple_singleline(
            label.to_owned(),
            density.font(weight, nav_text_size(density)),
            text_color,
        );
        job.wrap = TextWrapping::truncate_at_width((right - x).max(0.0));
        let galley = painter.layout_job(job);
        let pos = egui::pos2(x, rect.center().y - galley.size().y / 2.0);
        painter.galley(pos, galley, text_color);

        if let (Some((_, expanded)), Some(fold_response)) = (fold, &fold_response) {
            let arrow = if expanded {
                egui_phosphor::regular::CARET_DOWN
            } else {
                egui_phosphor::regular::CARET_RIGHT
            };
            let arrow_color = if fold_response.hovered() {
                colors.strong
            } else {
                colors.muted
            };
            painter.text(
                fold_response.rect.center(),
                Align2::CENTER_CENTER,
                arrow,
                density.font(Weight::Regular, 11.0),
                arrow_color,
            );
        }
    }

    let selected = state == RowState::Selected;
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, selected, label)
    });
    NavRow {
        response,
        fold: fold_response,
    }
}

/// An icon-only row of the rail, the page name on hover.
fn rail_row(ui: &mut Ui, id: egui::Id, icon: &str, label: &str, current: bool) -> egui::Response {
    let density = Density::of(ui.ctx());
    let colors = nav_colors(ui);
    let height = nav_row_height(density) + 6.0;
    let width = ui.available_width();
    let (_, rect) = ui.allocate_space(egui::vec2(width, height));
    let response = ui
        .interact(rect, id, Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    if ui.is_rect_visible(rect) {
        let state = if current {
            RowState::Selected
        } else {
            RowState::Idle
        };
        paint_row_background(ui, rect, state, response.hovered());
        let icon_color = if current {
            colors.accent
        } else if response.hovered() {
            colors.strong
        } else {
            colors.muted
        };
        ui.painter().text(
            rect.center(),
            Align2::CENTER_CENTER,
            icon,
            density.font(Weight::Regular, 17.0),
            icon_color,
        );
    }
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, current, label)
    });
    if response.hovered() {
        rail_tooltip(ui, id, rect, label);
    }
    response
}

/// The page name next to a hovered rail icon (to its right, like a pro
/// app's tool rail), instead of egui's tooltip under the pointer, which
/// would cover the icon below.
fn rail_tooltip(ui: &Ui, id: egui::Id, row: Rect, label: &str) {
    let density = Density::of(ui.ctx());
    let color = ui_tokens::palette(ui.ctx());
    egui::Area::new(id.with("rail_tooltip"))
        .order(egui::Order::Tooltip)
        .interactable(false)
        .pivot(Align2::LEFT_CENTER)
        .fixed_pos(egui::pos2(row.right() + 6.0, row.center().y))
        .show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style())
                .inner_margin(egui::Margin::symmetric(8, 4))
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new(label)
                            .font(density.font(Weight::Medium, nav_text_size(density)))
                            .color(color.text_strong),
                    );
                });
        });
}

fn nav_row_height(density: Density) -> f32 {
    match density {
        Density::Touch => 28.0,
        Density::Compact => 26.0,
    }
}

fn nav_text_size(density: Density) -> f32 {
    match density {
        Density::Touch => 14.0,
        Density::Compact => 13.0,
    }
}

/// The startup status slot: icon and a short caption in the status color,
/// the summary on hover. On the rail only the icon. Returns whether it was
/// clicked.
fn status_row(ui: &mut Ui, status: &NavbarStatus<'_>, rail: bool) -> bool {
    let density = Density::of(ui.ctx());
    let (icon, _, color) = status_style(status.indicator);
    let caption = match status.indicator {
        StartupIndicator::Failed => "Startup failed",
        StartupIndicator::Restarting => "Restarting...",
        StartupIndicator::Running => "Starting...",
    };
    let width = ui.available_width();
    let (_, rect) = ui.allocate_space(egui::vec2(width, nav_row_height(density)));
    let response = ui.interact(rect, egui::Id::new("desktop_nav_status"), Sense::click());
    let painter = ui.painter_at(rect);
    let fill = if response.hovered() {
        color.gamma_multiply(0.3)
    } else {
        color.gamma_multiply(0.15)
    };
    painter.rect_filled(rect, ui_tokens::RADIUS, fill);
    if rail {
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            icon,
            density.font(Weight::Regular, ICON_SIZE),
            color,
        );
    } else {
        let x = rect.left() + ROW_PAD - 4.0;
        painter.text(
            egui::pos2(x, rect.center().y),
            Align2::LEFT_CENTER,
            icon,
            density.font(Weight::Regular, ICON_SIZE),
            color,
        );
        painter.text(
            egui::pos2(x + ICON_COLUMN, rect.center().y),
            Align2::LEFT_CENTER,
            caption,
            density.font(Weight::Medium, nav_text_size(density)),
            color,
        );
    }
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(format!("{}\nClick to open Logs.", status.summary))
        .clicked()
}

#[cfg(test)]
mod tests {
    use super::{desktop_sidebar, page_row_id, sidebar_groups};
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

    #[test]
    fn every_navbar_page_is_in_one_group() {
        let listed: Vec<AppPage> = sidebar_groups()
            .into_iter()
            .flat_map(|(_, pages)| pages)
            .collect();
        let mut expected: Vec<AppPage> = AppPage::navbar_pages().collect();
        let mut sorted = listed.clone();
        sorted.sort_by_key(|page| *page as usize);
        expected.sort_by_key(|page| *page as usize);
        assert_eq!(sorted, expected);
    }

    #[test]
    fn the_header_folds_to_the_rail_and_rail_icons_open_pages() {
        let mut navbar = Navbar::new(AppPage::Logs);
        let fold = egui::Id::new("desktop_nav_fold_rail");
        assert_eq!(click(&navbar, None, fold), Some(SidebarClick::ToggleRail));
        navbar.sidebar_click(SidebarClick::ToggleRail);
        assert!(navbar.is_rail());
        assert_eq!(
            click(&navbar, None, page_row_id(AppPage::Setup)),
            Some(SidebarClick::Page(AppPage::Setup))
        );
        assert_eq!(click(&navbar, None, fold), Some(SidebarClick::ToggleRail));
    }

    #[test]
    fn a_section_fold_arrow_toggles_it() {
        let mut navbar = Navbar::new(AppPage::System);
        navbar.sync_sections();
        let fold = egui::Id::new(("desktop_nav_fold", AppPage::System));
        assert_eq!(
            click(&navbar, None, fold),
            Some(SidebarClick::ToggleSection(AppPage::System))
        );
    }
}
