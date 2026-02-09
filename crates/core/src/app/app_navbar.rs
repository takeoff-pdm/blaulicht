use blaulicht_shared::{ControlEvent, ControlEventMessage, EventOriginator, MainUiEvent};
use egui::Context;

use crate::app::BlaulichtApp;

impl BlaulichtApp {
    pub fn show_navbar(&mut self, ctx: &Context) {
        let Some(page_change) = self.navbar.ui(ctx) else {
            return;
        };

        // self.current_page = page_change.clone();

        // Send event to notify plugins.
        self.data
            .event_bus_connection
            .send(ControlEventMessage::new(
                EventOriginator::Web,
                ControlEvent::MainUi(MainUiEvent::NavigatePage(page_change)),
            ));
    }
}
