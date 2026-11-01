use std::collections::{HashMap, HashSet};

use blaulicht_shared::{AppPage, AppSubPage};
use egui::{Context, Frame, Separator, Widget};

use crate::app::{
    components::{self, ButtonSize},
    startup::StartupIndicator,
};

/// Navbar slot for a startup stage that failed or is still running.
pub struct NavbarStatus<'a> {
    pub indicator: StartupIndicator,
    pub summary: &'a str,
}

pub fn app_page_to_icon(from: &AppPage) -> &'static str {
    match from {
        AppPage::Logs => egui_phosphor::regular::TERMINAL_WINDOW,
        AppPage::System => egui_phosphor::regular::GEAR,
        AppPage::Audio => egui_phosphor::regular::WAVEFORM,
        AppPage::Setup => egui_phosphor::regular::WRENCH,
        AppPage::View => egui_phosphor::regular::STACK,
        AppPage::ViewPerformance => egui_phosphor::regular::GAME_CONTROLLER,
        AppPage::Programmer => egui_phosphor::regular::FADERS,
        AppPage::Animations => egui_phosphor::regular::WAVE_SINE,
        AppPage::Palettes => egui_phosphor::regular::PALETTE,
        AppPage::SceneGraph => egui_phosphor::regular::GRAPH,
        AppPage::Visualizer => egui_phosphor::regular::CUBE_FOCUS,
    }
}

/// A page's name in the desktop sidebar (the mobile rail shows only icons).
pub fn app_page_label(page: AppPage) -> &'static str {
    match page {
        AppPage::Logs => "Logs",
        AppPage::System => "System",
        AppPage::Audio => "Audio",
        AppPage::Setup => "Setup",
        AppPage::View => "Views",
        AppPage::ViewPerformance => "View Performance",
        AppPage::Programmer => "Programmer",
        AppPage::Animations => "Animations",
        AppPage::Palettes => "Palettes",
        AppPage::SceneGraph => "Scene Graph",
        AppPage::Visualizer => "Visualizer",
    }
}

/// Something clicked in the desktop sidebar (`components::desktop_sidebar`),
/// applied by [`Navbar::sidebar_click`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarClick {
    /// A page row. For a page with sub-pages, the section header.
    Page(AppPage),
    /// A sub-page row inside its page's section.
    SubPage(AppSubPage),
    /// The fold arrow of a page's section.
    ToggleSection(AppPage),
    /// The startup status slot: opens Logs.
    Status,
    /// The header's collapse button: switches between the full sidebar and
    /// the icon rail.
    ToggleRail,
}

/// What a sidebar click changed, for the events that tell plugins.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavChange {
    /// Switched to this page.
    Page(AppPage),
    /// Selected this sub-page; `page_changed` if that also switched to its
    /// page.
    SubPage {
        subpage: AppSubPage,
        page_changed: bool,
    },
}

pub struct Navbar {
    pub current_page: AppPage,
    /// Selected tab per page. Pages without an entry show their default tab.
    subpages: HashMap<AppPage, AppSubPage>,
    /// Desktop sidebar: pages whose sub-page section is unfolded.
    expanded: HashSet<AppPage>,
    /// The current page when [`Navbar::sync_sections`] last ran, so that it
    /// unfolds a section once per page change and a section folded by hand
    /// stays folded.
    synced_page: Option<AppPage>,
    /// Desktop sidebar: folded to an icon rail.
    rail: bool,
}

impl Navbar {
    pub fn new(initial_page: AppPage) -> Self {
        let mut navbar = Self {
            current_page: AppPage::Logs,
            subpages: HashMap::new(),
            expanded: HashSet::new(),
            synced_page: None,
            rail: false,
        };
        navbar.navigate_to(initial_page);
        navbar
    }

    /// Switches to `page`. A page that moved into another page's tab
    /// ([`AppPage::moved_to`], e.g. Animations into Setup › Animations) opens
    /// that tab instead, so old `NavigatePage` events keep working.
    pub fn navigate_to(&mut self, page: AppPage) {
        match page.moved_to() {
            Some(subpage) => self.navigate_to_subpage(subpage),
            None => self.current_page = page,
        }
    }

    pub fn page(&self) -> AppPage {
        self.current_page
    }

    /// Selects `subpage` and switches to the page that owns it.
    pub fn navigate_to_subpage(&mut self, subpage: AppSubPage) {
        self.select_subpage(subpage);
        self.current_page = subpage.page();
    }

    /// Selects `subpage` in its page without switching the current page.
    pub fn select_subpage(&mut self, subpage: AppSubPage) {
        self.subpages.insert(subpage.page(), subpage);
    }

    /// The tab `page` shows: the selected one, else its default. `None` for a
    /// page without sub-pages.
    pub fn subpage(&self, page: AppPage) -> Option<AppSubPage> {
        self.subpages
            .get(&page)
            .copied()
            .or_else(|| page.default_subpage())
    }

    /// Desktop sidebar: whether `page`'s sub-pages are unfolded. Always
    /// `false` for a page without sub-pages.
    pub fn is_expanded(&self, page: AppPage) -> bool {
        self.expanded.contains(&page)
    }

    /// Desktop sidebar: whether it is folded to an icon rail.
    pub fn is_rail(&self) -> bool {
        self.rail
    }

    /// Desktop sidebar: unfolds the current page's section when the current
    /// page changed since the last call, however it changed (sidebar, mobile
    /// rail, `NavigatePage` / `NavigateSubPage` events). Call once per frame
    /// before drawing the sidebar.
    pub fn sync_sections(&mut self) {
        if self.synced_page == Some(self.current_page) {
            return;
        }
        self.synced_page = Some(self.current_page);
        self.expand(self.current_page);
    }

    fn expand(&mut self, page: AppPage) {
        if !page.list_subpages().is_empty() {
            self.expanded.insert(page);
        }
    }

    /// Applies a desktop sidebar click. Returns what changed, `None` if the
    /// click only folded a section or hit the entry already shown.
    pub fn sidebar_click(&mut self, click: SidebarClick) -> Option<NavChange> {
        match click {
            SidebarClick::Page(page) => {
                // Opens the page's selected (or default) sub-page, and shows
                // its section.
                self.expand(page);
                if self.current_page == page {
                    return None;
                }
                self.navigate_to(page);
                Some(NavChange::Page(self.current_page))
            }
            SidebarClick::SubPage(subpage) => {
                let page = subpage.page();
                let page_changed = self.current_page != page;
                let changed = page_changed || self.subpage(page) != Some(subpage);
                self.navigate_to_subpage(subpage);
                self.expand(page);
                changed.then_some(NavChange::SubPage {
                    subpage,
                    page_changed,
                })
            }
            SidebarClick::ToggleSection(page) => {
                if !self.expanded.remove(&page) {
                    self.expand(page);
                }
                None
            }
            SidebarClick::Status => self.sidebar_click(SidebarClick::Page(AppPage::Logs)),
            SidebarClick::ToggleRail => {
                self.rail = !self.rail;
                None
            }
        }
    }

    /// `status`, if given, adds a slot below the pages; clicking it opens Logs.
    pub fn ui(&mut self, ctx: &Context, status: Option<NavbarStatus<'_>>) -> Option<AppPage> {
        const WIDTH: f32 = 45.0;

        let mut changed = None;

        egui::SidePanel::left("navbar")
            .resizable(false)
            .default_width(WIDTH)
            .frame(Frame::NONE)
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);

                let page_count = AppPage::navbar_pages().count();
                let button_count = page_count + usize::from(status.is_some());
                // let spacing_top_bottom = 3.0;
                // let spacing = 6.0;
                let sep_spacing = 3.0;
                let button_height = (ui.available_height()
                    - (sep_spacing * (button_count - 1) as f32))
                    / button_count as f32;

                // println!("a: {}", button_height);

                let button_size = ButtonSize::Large
                    .with_height(button_height)
                    .with_width(WIDTH)
                    .with_font_size(14.5);

                // ui.add_space(spacing_top_bottom);

                for (idx, page) in AppPage::navbar_pages().enumerate() {
                    let is_selected = self.current_page == page;
                    // let label = page.short().to_uppercase();
                    let label = app_page_to_icon(&page);

                    if components::Button::new(&label, button_size).ui(ui, is_selected) {
                        if !is_selected {
                            self.current_page = page.clone();
                            changed = Some(page.clone());
                        }
                    }

                    // if components::button(ui, is_selected, &label, button_size) && !is_selected {
                    // }

                    if idx + 1 < button_count {
                        // ui.add_space(spacing);
                        // ui.separator();
                        Separator::default().spacing(sep_spacing).ui(ui);
                    }
                }

                if let Some(status) = &status {
                    let size = egui::vec2(WIDTH, button_height);
                    if status_slot(ui, size, status) && self.current_page != AppPage::Logs {
                        self.current_page = AppPage::Logs;
                        changed = Some(AppPage::Logs);
                    }
                }
            });

        changed
    }
}

/// Icon, short caption and color of the startup status slot.
pub(crate) fn status_style(
    indicator: StartupIndicator,
) -> (&'static str, &'static str, egui::Color32) {
    use egui_phosphor::regular as icons;
    match indicator {
        StartupIndicator::Failed => (icons::WARNING, "FAIL", egui::Color32::LIGHT_RED),
        StartupIndicator::Restarting => (
            icons::ARROWS_CLOCKWISE,
            "RESTART",
            egui::Color32::from_rgb(240, 180, 60),
        ),
        StartupIndicator::Running => (icons::HOURGLASS, "BUSY", egui::Color32::LIGHT_BLUE),
    }
}

fn status_slot(ui: &mut egui::Ui, size: egui::Vec2, status: &NavbarStatus<'_>) -> bool {
    let (icon, caption, color) = status_style(status.indicator);

    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let painter = ui.painter_at(rect);
    let fill = if response.hovered() {
        color.gamma_multiply(0.3)
    } else {
        color.gamma_multiply(0.15)
    };
    painter.rect_filled(rect, 2.0, fill);
    painter.text(
        rect.center() - egui::vec2(0.0, 5.0),
        egui::Align2::CENTER_CENTER,
        icon,
        egui::FontId::proportional(16.0),
        color,
    );
    painter.text(
        rect.center() + egui::vec2(0.0, 10.0),
        egui::Align2::CENTER_CENTER,
        caption,
        egui::FontId::proportional(8.0),
        color,
    );

    response
        .on_hover_text(format!("{}\nClick to open Logs.", status.summary))
        .clicked()
}

#[cfg(test)]
mod tests {
    use super::{NavChange, Navbar, SidebarClick};
    use blaulicht_shared::{AppPage, AppSubPage};

    #[test]
    fn pages_start_on_their_default_subpage() {
        let navbar = Navbar::new(AppPage::Logs);
        assert_eq!(
            navbar.subpage(AppPage::System),
            Some(AppSubPage::System_System)
        );
        assert_eq!(navbar.subpage(AppPage::Logs), None);
    }

    #[test]
    fn navigating_to_a_subpage_switches_to_its_page() {
        let mut navbar = Navbar::new(AppPage::Logs);
        navbar.navigate_to_subpage(AppSubPage::System_Midi);
        assert_eq!(navbar.page(), AppPage::System);
        assert_eq!(
            navbar.subpage(AppPage::System),
            Some(AppSubPage::System_Midi)
        );
    }

    #[test]
    fn selecting_a_subpage_keeps_the_current_page() {
        let mut navbar = Navbar::new(AppPage::Audio);
        navbar.select_subpage(AppSubPage::System_Plugins);
        assert_eq!(navbar.page(), AppPage::Audio);
        assert_eq!(
            navbar.subpage(AppPage::System),
            Some(AppSubPage::System_Plugins)
        );
    }

    #[test]
    fn selection_survives_leaving_the_page() {
        let mut navbar = Navbar::new(AppPage::Logs);
        navbar.navigate_to_subpage(AppSubPage::System_Serial);
        navbar.navigate_to(AppPage::Audio);
        navbar.navigate_to(AppPage::System);
        assert_eq!(
            navbar.subpage(AppPage::System),
            Some(AppSubPage::System_Serial)
        );
    }

    #[test]
    fn moved_pages_open_their_setup_tab() {
        let mut navbar = Navbar::new(AppPage::Logs);
        navbar.navigate_to(AppPage::Animations);
        assert_eq!(navbar.page(), AppPage::Setup);
        assert_eq!(
            navbar.subpage(AppPage::Setup),
            Some(AppSubPage::Setup_Animations)
        );

        navbar.navigate_to(AppPage::Palettes);
        assert_eq!(navbar.page(), AppPage::Setup);
        assert_eq!(
            navbar.subpage(AppPage::Setup),
            Some(AppSubPage::Setup_Palettes)
        );

        // Navigating to Setup itself keeps the selected tab.
        navbar.navigate_to(AppPage::Audio);
        navbar.navigate_to(AppPage::Setup);
        assert_eq!(
            navbar.subpage(AppPage::Setup),
            Some(AppSubPage::Setup_Palettes)
        );

        let navbar = Navbar::new(AppPage::Animations);
        assert_eq!(navbar.page(), AppPage::Setup);
    }

    #[test]
    fn sidebar_subpage_click_navigates_and_unfolds() {
        let mut navbar = Navbar::new(AppPage::Logs);
        let change = navbar.sidebar_click(SidebarClick::SubPage(AppSubPage::System_Dmx));
        assert_eq!(
            change,
            Some(NavChange::SubPage {
                subpage: AppSubPage::System_Dmx,
                page_changed: true,
            })
        );
        assert_eq!(navbar.page(), AppPage::System);
        assert_eq!(
            navbar.subpage(AppPage::System),
            Some(AppSubPage::System_Dmx)
        );
        assert!(navbar.is_expanded(AppPage::System));

        // Another tab of the same page.
        let change = navbar.sidebar_click(SidebarClick::SubPage(AppSubPage::System_Midi));
        assert_eq!(
            change,
            Some(NavChange::SubPage {
                subpage: AppSubPage::System_Midi,
                page_changed: false,
            })
        );
        // The tab already shown.
        assert_eq!(
            navbar.sidebar_click(SidebarClick::SubPage(AppSubPage::System_Midi)),
            None
        );
    }

    #[test]
    fn sidebar_page_click_opens_the_selected_or_default_subpage() {
        let mut navbar = Navbar::new(AppPage::Logs);
        assert_eq!(
            navbar.sidebar_click(SidebarClick::Page(AppPage::Setup)),
            Some(NavChange::Page(AppPage::Setup))
        );
        assert_eq!(
            navbar.subpage(AppPage::Setup),
            Some(AppSubPage::Setup_Fixtures)
        );
        assert!(navbar.is_expanded(AppPage::Setup));

        navbar.sidebar_click(SidebarClick::SubPage(AppSubPage::Setup_Palettes));
        navbar.sidebar_click(SidebarClick::Page(AppPage::Programmer));
        assert_eq!(navbar.page(), AppPage::Programmer);
        // A page without sub-pages has no section to unfold.
        assert!(!navbar.is_expanded(AppPage::Programmer));

        navbar.sidebar_click(SidebarClick::Page(AppPage::Setup));
        assert_eq!(
            navbar.subpage(AppPage::Setup),
            Some(AppSubPage::Setup_Palettes)
        );
        // Clicking the page already shown changes nothing.
        assert_eq!(
            navbar.sidebar_click(SidebarClick::Page(AppPage::Setup)),
            None
        );
    }

    #[test]
    fn sections_unfold_once_per_page_change() {
        let mut navbar = Navbar::new(AppPage::Logs);
        navbar.sync_sections();
        assert!(!navbar.is_expanded(AppPage::System));

        // Navigation from elsewhere (an event, the mobile rail) unfolds the
        // current page's section.
        navbar.navigate_to_subpage(AppSubPage::System_Serial);
        navbar.sync_sections();
        assert!(navbar.is_expanded(AppPage::System));

        // Folded by hand: stays folded while the page stays.
        assert_eq!(
            navbar.sidebar_click(SidebarClick::ToggleSection(AppPage::System)),
            None
        );
        navbar.sync_sections();
        assert!(!navbar.is_expanded(AppPage::System));
        assert_eq!(navbar.page(), AppPage::System);

        // Leaving and coming back unfolds it again; other sections keep
        // their state.
        navbar.sidebar_click(SidebarClick::ToggleSection(AppPage::Setup));
        navbar.navigate_to(AppPage::Audio);
        navbar.sync_sections();
        navbar.navigate_to(AppPage::System);
        navbar.sync_sections();
        assert!(navbar.is_expanded(AppPage::System));
        assert!(navbar.is_expanded(AppPage::Setup));

        navbar.sidebar_click(SidebarClick::ToggleSection(AppPage::Setup));
        assert!(!navbar.is_expanded(AppPage::Setup));
        // Pages without sub-pages never unfold.
        navbar.sidebar_click(SidebarClick::ToggleSection(AppPage::Logs));
        assert!(!navbar.is_expanded(AppPage::Logs));
    }

    #[test]
    fn sidebar_status_opens_logs() {
        let mut navbar = Navbar::new(AppPage::Programmer);
        assert_eq!(
            navbar.sidebar_click(SidebarClick::Status),
            Some(NavChange::Page(AppPage::Logs))
        );
        assert_eq!(navbar.sidebar_click(SidebarClick::Status), None);
    }

    #[test]
    fn moved_pages_open_their_setup_tab_from_the_sidebar() {
        let mut navbar = Navbar::new(AppPage::Logs);
        assert_eq!(
            navbar.sidebar_click(SidebarClick::Page(AppPage::Palettes)),
            Some(NavChange::Page(AppPage::Setup))
        );
        assert_eq!(
            navbar.subpage(AppPage::Setup),
            Some(AppSubPage::Setup_Palettes)
        );
    }

    #[test]
    fn rail_toggle_keeps_the_page() {
        let mut navbar = Navbar::new(AppPage::Programmer);
        assert!(!navbar.is_rail());
        assert_eq!(navbar.sidebar_click(SidebarClick::ToggleRail), None);
        assert!(navbar.is_rail());
        assert_eq!(navbar.page(), AppPage::Programmer);
        navbar.sidebar_click(SidebarClick::ToggleRail);
        assert!(!navbar.is_rail());
    }

    #[test]
    fn every_page_has_its_own_icon() {
        let icons: std::collections::HashSet<_> = AppPage::navbar_pages()
            .map(|page| super::app_page_to_icon(&page))
            .collect();
        assert_eq!(icons.len(), AppPage::navbar_pages().count());
    }
}
