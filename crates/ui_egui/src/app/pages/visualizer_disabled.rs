//! Stand-in for the visualizer in builds without the `visualizer` feature
//! (three-d and glow): the page shows a notice, everything else is a no-op.

use egui::Context;

use crate::app::page::PageRenderContext;
use crate::app::BlaulichtApp;
use blaulicht_engine::state::ScreenId;

/// The visualizer's view state; empty without the visualizer.
#[derive(Default)]
pub struct VisualizerUiState;

impl VisualizerUiState {
    pub(crate) fn reset_stage_view(&mut self) {}
}

impl BlaulichtApp {
    pub(crate) fn show_fixtures_in_visualizer(
        &mut self,
        _fixtures: impl IntoIterator<Item = (u8, u8)>,
    ) {
    }

    pub fn visualizer_ui(
        &mut self,
        ui: &mut egui::Ui,
        _ctx: &Context,
        _screen_id: ScreenId,
        _render_context: PageRenderContext,
    ) {
        ui.heading("Visualizer");
        ui.add_space(4.0);
        ui.label("This build has no visualizer (feature \"visualizer\" is off).");
    }
}
