use blaulicht_shared::AppPage;
use egui::{Context, Frame, Separator, Widget};
use strum::IntoEnumIterator;

use crate::app::components::{self, ButtonSize};

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
        AppPage::Palettes => egui_phosphor::regular::PALETTE,
        AppPage::SceneGraph => egui_phosphor::regular::GRAPH,
        AppPage::Visualizer => egui_phosphor::regular::WAVEFORM,
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
            .frame(Frame::NONE)
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);

                let button_count = AppPage::iter().count();
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
            });

        changed
    }
}
