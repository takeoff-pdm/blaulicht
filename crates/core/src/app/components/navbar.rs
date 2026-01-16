use blaulicht_shared::{AppPage, ControlEvent, ControlEventMessage, EventOriginator, MainUiEvent};
use egui::Context;
use strum::IntoEnumIterator;

use crate::app::{
    components::{self, ButtonSize},
    BlaulichtApp,
};

fn app_page_to_icon(from: &AppPage) -> &'static str {
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

impl BlaulichtApp {
    pub fn navbar_ui(&mut self, ctx: &Context) {
        const WIDTH: f32 = 45.0;

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

                        // Send event to notify plugins.
                        self.data
                            .event_bus_connection
                            .send(ControlEventMessage::new(
                                EventOriginator::Web,
                                ControlEvent::MainUi(MainUiEvent::NavigatePage(page)),
                            ));
                    }

                    if idx + 1 < button_count {
                        ui.add_space(spacing);
                    }
                }
            });
    }
}
