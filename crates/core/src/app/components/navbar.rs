use std::collections::HashMap;

use blaulicht_shared::{AppPage, AppSubPage};
use egui::{Context, Frame, Separator, Widget};
use strum::IntoEnumIterator;

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
        AppPage::Visualizer => egui_phosphor::regular::WAVEFORM,
    }
}

pub struct Navbar {
    pub current_page: AppPage,
    /// Selected tab per page. Pages without an entry show their default tab.
    subpages: HashMap<AppPage, AppSubPage>,
}

impl Navbar {
    pub fn new(initial_page: AppPage) -> Self {
        Self {
            current_page: initial_page,
            subpages: HashMap::new(),
        }
    }

    pub fn navigate_to(&mut self, page: AppPage) {
        self.current_page = page;
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

                let page_count = AppPage::iter().count();
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

                for (idx, page) in AppPage::iter().enumerate() {
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

fn status_slot(ui: &mut egui::Ui, size: egui::Vec2, status: &NavbarStatus<'_>) -> bool {
    use egui_phosphor::regular as icons;
    let (icon, caption, color) = match status.indicator {
        StartupIndicator::Failed => (icons::WARNING, "FAIL", egui::Color32::LIGHT_RED),
        StartupIndicator::Restarting => (
            icons::ARROWS_CLOCKWISE,
            "RESTART",
            egui::Color32::from_rgb(240, 180, 60),
        ),
        StartupIndicator::Running => (icons::HOURGLASS, "BUSY", egui::Color32::LIGHT_BLUE),
    };

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
    use super::Navbar;
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
}
