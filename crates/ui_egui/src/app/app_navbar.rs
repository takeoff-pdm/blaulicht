use blaulicht_shared::{ControlEvent, ControlEventMessage, EventOriginator, MainUiEvent};
use egui::Context;

use crate::app::{
    components::{self, NavChange, NavbarStatus},
    BlaulichtApp,
};
use blaulicht_engine::client::EngineClient;

impl BlaulichtApp {
    /// The mobile layout's icon rail.
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
        self.notify_navigation(MainUiEvent::NavigatePage(page_change));
    }

    /// The desktop layout's navigation sidebar, as a left panel of `ui`.
    pub fn show_desktop_sidebar(&mut self, ui: &mut egui::Ui) {
        let summary = self.startup_progress.summary();
        let status = self
            .startup_progress
            .indicator()
            .map(|indicator| NavbarStatus {
                indicator,
                summary: &summary,
            });
        self.navbar.sync_sections();
        let Some(click) = components::desktop_sidebar(ui, &self.navbar, status) else {
            return;
        };
        match self.navbar.sidebar_click(click) {
            None => {}
            Some(NavChange::Page(page)) => {
                self.notify_navigation(MainUiEvent::NavigatePage(page));
            }
            Some(NavChange::SubPage {
                subpage,
                page_changed,
            }) => {
                if page_changed {
                    self.notify_navigation(MainUiEvent::NavigatePage(subpage.page()));
                }
                self.notify_navigation(MainUiEvent::NavigateSubPage(subpage));
            }
        }
    }

    /// Tells plugins about a navigation in this UI. Sent as `Web`, so it
    /// doesn't loop back into the UI.
    fn notify_navigation(&self, event: MainUiEvent) {
        self.client.send(ControlEventMessage::new(
            EventOriginator::Web,
            ControlEvent::MainUi(event),
        ));
    }
}
