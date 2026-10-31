use blaulicht_shared::{ControlEvent, ControlEventMessage, EventOriginator, MainUiEvent};
use egui::Context;

use crate::app::{components::NavbarStatus, BlaulichtApp};
use blaulicht_engine::client::EngineClient;

impl BlaulichtApp {
    pub fn show_navbar(&mut self, ctx: &Context) {
        let summary = self.startup_progress.summary();
        let status = self
            .startup_progress
            .indicator()
            .map(|indicator| NavbarStatus {
                indicator,
                summary: &summary,
            });
        let Some(page_change) = self.navbar.ui(ctx, status) else {
            return;
        };

        // self.current_page = page_change.clone();

        // Send event to notify plugins.
        self.client.send(ControlEventMessage::new(
            EventOriginator::Web,
            ControlEvent::MainUi(MainUiEvent::NavigatePage(page_change)),
        ));
    }
}
