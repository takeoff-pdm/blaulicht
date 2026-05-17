use crate::app::BlaulichtApp;
use crate::state::ScreenId;
use blaulicht_shared::AppPage;
use egui::Context;

impl BlaulichtApp {
    // Page passed as parameter, so that this function can be used on an external screen.
    pub fn page_content_based_on_tab(
        &mut self,
        page: AppPage,
        ui: &mut egui::Ui,
        ctx: &Context,
        screen_id: ScreenId,
    ) {
        match page {
            AppPage::Logs => {
                self.logs_ui(ui, ctx);
            }
            AppPage::System => {
                self.system_ui(ui, ctx, screen_id);
            }
            AppPage::Audio => {
                self.audio_ui(ui, ctx);
            }
            AppPage::FixturesSetup => {
                self.fixtures_ui_setup(ui, ctx);
            }
            AppPage::View => {
                self.view_ui(ui, ctx);
            }
            AppPage::ViewPerformance => {
                self.view_perf_ui(ui, ctx);
            }
            AppPage::FixturesPerformance => {
                self.fixtures_ui(ui, ctx);
            }
            AppPage::Animations => {
                self.animations_ui(ctx, ui);
            }
            AppPage::Visualizer => {
                self.visualizer_ui(ui, ctx);
            }
        }
    }
}
