use blaulicht_shared::{AppPage, ControlEvent, ControlEventMessage, EventOriginator, MainUiEvent};
use egui::Context;
use strum::IntoEnumIterator;

use crate::app::{
    components::{self, ButtonSize},
    BlaulichtApp,
};

pub fn app_page_to_icon(from: &AppPage) -> &'static str {
    match from {
        AppPage::Logs => egui_phosphor::regular::TERMINAL_WINDOW,
        AppPage::System => egui_phosphor::regular::GEAR,
        AppPage::Audio => egui_phosphor::regular::WAVEFORM,
        AppPage::FixturesSetup => egui_phosphor::regular::WRENCH,
        AppPage::View => egui_phosphor::regular::STACK,
        AppPage::ViewPerformance => egui_phosphor::regular::GAME_CONTROLLER,
        AppPage::FixturesPerformance => egui_phosphor::regular::FADERS,
        AppPage::Animations => egui_phosphor::regular::WAVE_SINE,
    }
}

pub struct Navbar {
    pub current_page: AppPage,
}

impl Navbar {
    pub fn new(initial_page: AppPage) -> Self {
        Self {
            current_page: initial_page,
        }
    }

    pub fn navigate_to(&mut self, page: AppPage) {
        self.current_page = page;
    }

    pub fn page(&self) -> AppPage {
        self.current_page
    }

    pub fn ui(&mut self, ctx: &Context) -> Option<AppPage> {
        const WIDTH: f32 = 45.0;

        let mut changed = None;

        egui::SidePanel::left("navbar")
            .resizable(false)
            .default_width(WIDTH)
            .show(ctx, |ui| {
                let button_count = AppPage::iter().count();
                let spacing_top_bottom = 3.0;
                let spacing = 6.0;
                let button_height = ui.available_height() / button_count as f32;
                let button_size = ButtonSize::Large
                    .with_height(button_height - spacing - spacing_top_bottom)
                    .with_width(WIDTH)
                    .with_font_size(20.0);

                ui.add_space(spacing_top_bottom);

                for (idx, page) in AppPage::iter().enumerate() {
                    let is_selected = self.current_page == page;
                    // let label = page.short().to_uppercase();
                    let label = app_page_to_icon(&page);

                    if components::button(ui, is_selected, &label, button_size) && !is_selected {
                        self.current_page = page.clone();
                        changed = Some(page.clone());
                    }

                    if idx + 1 < button_count {
                        ui.add_space(spacing);
                    }
                }
            });

        changed
    }
}
