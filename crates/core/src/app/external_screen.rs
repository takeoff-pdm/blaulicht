#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

use crate::app::{BlaulichtApp, ExternalScreen};
use crate::state::ScreenId;
use blaulicht_shared::AppPage;
use egui::{
    vec2, Color32, Context, Frame, Id, Margin, Sense, Stroke, WidgetText,
};
use egui_dock::tab_viewer::OnCloseResponse;
use egui_dock::{DockArea, DockState, Style};
use strum::IntoEnumIterator;

#[derive(Clone)]
pub(crate) struct Pane {
    page: AppPage,
}

impl Pane {
    fn new(page: AppPage) -> Self {
        Self { page }
    }

    fn label(&self) -> &str {
        match self.page {
            AppPage::Logs => "Logs",
            AppPage::System => "System",
            AppPage::Audio => "Audio",
            AppPage::FixturesSetup => "Fixtures Setup",
            AppPage::View => "View",
            AppPage::ViewPerformance => "View Performance",
            AppPage::FixturesPerformance => "Fixtures Performance",
            AppPage::Animations => "Animations",
            AppPage::Visualizer => "Visualizer",
        }
    }

    fn title_text(&self) -> WidgetText {
        self.label().into()
    }
}

pub fn create() -> ExternalScreen {
    ExternalScreen::new()
}

impl ExternalScreen {
    pub(crate) fn new() -> Self {
        // let tabs = (1..=8).map(|idx| Pane::new(format!("Tab {idx}"))).collect();

        let tabs = AppPage::iter().map(|page| Pane::new(page)).collect();

        Self {
            dock_state: DockState::new(tabs),
        }
    }

    fn ensure_core_tabs(&mut self) {
        let present_pages = {
            let mut pages = Vec::new();
            for surface in self.dock_state.iter_surfaces() {
                for (_, pane) in surface.iter_all_tabs() {
                    if !pages.contains(&pane.page) {
                        pages.push(pane.page);
                    }
                }
            }
            pages
        };

        for page in AppPage::iter() {
            if !present_pages.contains(&page) {
                self.dock_state.push_to_first_leaf(Pane::new(page));
            }
        }
    }
}

struct TabViewer<'bl, 'ct> {
    app: &'bl mut BlaulichtApp,
    ctx: &'ct Context,
    screen_id: ScreenId,
}

impl<'bl, 'ct> egui_dock::TabViewer for TabViewer<'bl, 'ct> {
    type Tab = Pane;

    fn title(&mut self, tab: &mut Self::Tab) -> WidgetText {
        tab.title_text()
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab) {
        Frame::NONE
            .outer_margin(Margin::same(0))
            .inner_margin(Margin::same(8))
            .stroke(Stroke::new(2.0, Color32::GRAY))
            .show(ui, |ui| {
                let (rect, _response) = ui.allocate_exact_size(vec2(723.0, 480.0), Sense::empty());
                let mut child_ui = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(rect)
                        .layout(egui::Layout::top_down(egui::Align::Min)),
                );

                child_ui.set_clip_rect(rect);

                self.app.page_content_based_on_tab(
                    tab.page,
                    &mut child_ui,
                    self.ctx,
                    self.screen_id,
                );
            });
    }

    fn on_close(&mut self, tab: &mut Self::Tab) -> OnCloseResponse {
        tracing::info!("Closed tab: {}", tab.label());
        OnCloseResponse::Close
    }
}

impl BlaulichtApp {
    pub fn drive_external_screen(&mut self, ctx: &Context, screen_idx: usize) {
        let viewport_id =
            egui::ViewportId::from_hash_of(format!("blaulicht_external_screen: {screen_idx}"));
        ctx.show_viewport_immediate(
            viewport_id,
            egui::ViewportBuilder::default()
                .with_title(format!("blaulicht_external_screen: {screen_idx}"))
                .with_inner_size([1920.0, 1080.0])
                .with_resizable(true),
            |ctx, _class| {
                let mut screen = self.external_screens[screen_idx].clone();
                let screen_id = ScreenId::external(screen_idx);

                self.draw_external_screen_contents(ctx, screen_id, &mut screen);

                // TODO: This is peak bullshit code.
                self.external_screens[screen_idx] = screen;
            },
        );
    }

    pub fn draw_external_screen_contents(
        &mut self,
        ctx: &Context,
        screen_id: ScreenId,
        screen: &mut ExternalScreen,
    ) {
        self.render_plugin_ui(ctx, screen_id);

        let mut tab_viewer = TabViewer {
            app: self,
            ctx,
            screen_id,
        };

        DockArea::new(screen.dock_state())
            .id(Id::new(("external_screen_dock", screen_id)))
            .style(Style::from_egui(ctx.style().as_ref()))
            .show(ctx, &mut tab_viewer);

        screen.ensure_core_tabs();
    }
}
