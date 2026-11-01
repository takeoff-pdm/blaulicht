use crate::app::view_state::ScreenId;
use crate::app::{components, BlaulichtApp};
use crate::layout::UiLayout;
use blaulicht_engine::client::EngineClient;
use blaulicht_shared::{
    AppPage, AppSubPage, ControlEvent, ControlEventMessage, EventOriginator, MainUiEvent,
};
use egui::Context;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PageWidthClass {
    Narrow,
    Standard,
    Wide,
}

#[derive(Debug, Clone, Copy)]
pub struct PageRenderContext {
    pub(crate) mode: blaulicht_engine::config::PageRenderMode,
    pub(crate) size: egui::Vec2,
    pub(crate) width_class: PageWidthClass,
    /// Only the visualizer reads it.
    #[cfg_attr(not(feature = "visualizer"), allow(dead_code))]
    pub(crate) short: bool,
    /// Whether pages with sub-pages draw their tab bar
    /// ([`BlaulichtApp::subpage_ui`]). Off in the desktop layout's main
    /// window, whose sidebar lists the sub-pages instead.
    subpage_tabs: bool,
}

impl PageRenderContext {
    pub(crate) fn new(mode: blaulicht_engine::config::PageRenderMode, size: egui::Vec2) -> Self {
        let width = finite_dimension(size.x);
        let height = finite_dimension(size.y);
        let width_class = if width < 720.0 {
            PageWidthClass::Narrow
        } else if width < 1_200.0 {
            PageWidthClass::Standard
        } else {
            PageWidthClass::Wide
        };
        Self {
            mode,
            size: egui::vec2(width, height),
            width_class,
            short: height < 480.0,
            subpage_tabs: true,
        }
    }

    /// The context of the main window's page, which fills its central panel.
    /// - Mobile: the kiosk rendering (`Default`), with sub-page tab bars.
    /// - Desktop: `Dynamic` rendering (the page reflows with the window, as in
    ///   a "Dynamic" dock tile), without tab bars: the sidebar lists the
    ///   sub-pages.
    pub(crate) fn for_main_screen(layout: UiLayout, size: egui::Vec2) -> Self {
        use blaulicht_engine::config::PageRenderMode;
        match layout {
            UiLayout::Mobile => Self::new(PageRenderMode::Default, size),
            UiLayout::Desktop => Self::new(PageRenderMode::Dynamic, size).without_subpage_tabs(),
        }
    }

    /// Pages render without their sub-page tab bar: something else (the
    /// desktop sidebar) selects the sub-page.
    pub(crate) fn without_subpage_tabs(self) -> Self {
        Self {
            subpage_tabs: false,
            ..self
        }
    }

    pub(crate) fn shows_subpage_tabs(self) -> bool {
        self.subpage_tabs
    }

    pub(crate) fn is_dynamic(self) -> bool {
        self.mode == blaulicht_engine::config::PageRenderMode::Dynamic
    }

    pub(crate) fn is_narrow_dynamic(self) -> bool {
        self.is_dynamic() && self.width_class == PageWidthClass::Narrow
    }

    pub(crate) fn primary_layout(self) -> egui::Layout {
        if self.is_narrow_dynamic() {
            egui::Layout::top_down(egui::Align::Min)
        } else {
            egui::Layout::left_to_right(egui::Align::Min)
        }
    }

    pub(crate) fn horizontal<R>(
        self,
        ui: &mut egui::Ui,
        align: egui::Align,
        add_contents: impl FnOnce(&mut egui::Ui) -> R,
    ) -> egui::InnerResponse<R> {
        if self.is_narrow_dynamic() {
            ui.horizontal_wrapped(add_contents)
        } else if align == egui::Align::Min {
            ui.horizontal_top(add_contents)
        } else {
            ui.horizontal(add_contents)
        }
    }
}

fn finite_dimension(value: f32) -> f32 {
    if value.is_finite() {
        value.max(1.0)
    } else {
        1.0
    }
}

/// Sub-page navigation. A page with tabs declares them in
/// `AppPage::list_subpages` and renders through [`BlaulichtApp::subpage_ui`];
/// the selected tab per page lives in the navbar state, so it survives page
/// switches and is shared by every screen that shows the page.
impl BlaulichtApp {
    /// The tab `page` currently shows; `None` for a page without sub-pages.
    pub(crate) fn current_subpage(&self, page: AppPage) -> Option<AppSubPage> {
        self.navbar.subpage(page)
    }

    /// Selects `subpage` within its page (the current page stays as it is,
    /// since the click may come from another screen) and tells plugins with a
    /// `NavigateSubPage` event. Use it for in-page links to a tab.
    pub(crate) fn open_subpage(&mut self, subpage: AppSubPage) {
        if self.current_subpage(subpage.page()) == Some(subpage) {
            return;
        }
        self.navbar.select_subpage(subpage);
        // Web originator: notifies plugins without looping back into the UI.
        self.client.send(ControlEventMessage::new(
            EventOriginator::Web,
            ControlEvent::MainUi(MainUiEvent::NavigateSubPage(subpage)),
        ));
    }

    /// Renders `page` with its sub-page tab bar along the bottom edge and calls
    /// `content` with the active tab and a `Ui` for the area above the bar.
    /// Without tabs in `render_context` (the desktop layout, where the sidebar
    /// selects the tab), `content` gets all of `ui` and no bar is drawn.
    /// Returns `None`, drawing nothing, if `page` has no sub-pages.
    pub(crate) fn subpage_ui<R>(
        &mut self,
        ui: &mut egui::Ui,
        page: AppPage,
        render_context: PageRenderContext,
        content: impl FnOnce(&mut Self, &mut egui::Ui, AppSubPage) -> R,
    ) -> Option<R> {
        let active = self.current_subpage(page)?;
        if !render_context.shows_subpage_tabs() {
            return Some(content(self, ui, active));
        }
        let (mut content_ui, clicked) =
            components::subpage_tabs_layout(ui, page.list_subpages(), active);
        let active = match clicked {
            Some(subpage) => {
                self.open_subpage(subpage);
                subpage
            }
            None => active,
        };
        Some(content(self, &mut content_ui, active))
    }
}

impl BlaulichtApp {
    // Page passed as parameter, so that this function can be used on an external screen.
    pub fn page_content_based_on_tab(
        &mut self,
        page: AppPage,
        ui: &mut egui::Ui,
        ctx: &Context,
        screen_id: ScreenId,
        render_context: PageRenderContext,
    ) {
        match page {
            AppPage::Logs => {
                self.logs_ui(ui, ctx, render_context);
            }
            AppPage::System => {
                self.system_ui(ui, ctx, screen_id, render_context);
            }
            AppPage::Audio => {
                self.audio_ui(ui, ctx, render_context);
            }
            AppPage::Setup => {
                self.setup_ui(ui, ctx, render_context);
            }
            AppPage::View => {
                self.view_ui(ui, ctx, render_context);
            }
            AppPage::ViewPerformance => {
                self.view_perf_ui(ui, ctx, render_context);
            }
            AppPage::Programmer => {
                self.programmer_ui(ui, ctx, render_context);
            }
            // Moved into Setup (`AppPage::moved_to`): the navbar redirects
            // them and saved dock panes are migrated, so this only renders
            // the editor alone should a caller still pass the old page.
            AppPage::Animations => {
                self.setup_animations_ui(ui, ctx, render_context);
            }
            AppPage::Palettes => {
                self.setup_palettes_ui(ui, ctx, render_context);
            }
            AppPage::SceneGraph => {
                self.scene_graph_ui(ui, ctx, render_context);
            }
            AppPage::Visualizer => {
                self.visualizer_ui(ui, ctx, screen_id, render_context);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn responsive_classes_are_stable_at_boundaries() {
        assert_eq!(
            PageRenderContext::new(
                blaulicht_engine::config::PageRenderMode::Dynamic,
                egui::vec2(719.0, 480.0)
            )
            .width_class,
            PageWidthClass::Narrow
        );
        assert_eq!(
            PageRenderContext::new(
                blaulicht_engine::config::PageRenderMode::Dynamic,
                egui::vec2(720.0, 480.0)
            )
            .width_class,
            PageWidthClass::Standard
        );
        assert_eq!(
            PageRenderContext::new(
                blaulicht_engine::config::PageRenderMode::Dynamic,
                egui::vec2(1_200.0, 480.0)
            )
            .width_class,
            PageWidthClass::Wide
        );
    }

    #[test]
    fn main_screen_context_follows_the_layout() {
        let size = egui::vec2(1_000.0, 700.0);
        let mobile = PageRenderContext::for_main_screen(UiLayout::Mobile, size);
        assert!(!mobile.is_dynamic());
        assert!(mobile.shows_subpage_tabs());
        let desktop = PageRenderContext::for_main_screen(UiLayout::Desktop, size);
        assert!(desktop.is_dynamic());
        assert!(!desktop.shows_subpage_tabs());
    }

    #[test]
    fn subpage_tabs_are_on_unless_turned_off() {
        let context = PageRenderContext::new(
            blaulicht_engine::config::PageRenderMode::Default,
            egui::vec2(800.0, 480.0),
        );
        assert!(context.shows_subpage_tabs());
        let desktop = context.without_subpage_tabs();
        assert!(!desktop.shows_subpage_tabs());
        assert_eq!(desktop.size, context.size);
    }

    #[test]
    fn invalid_dimensions_are_sanitized() {
        let context = PageRenderContext::new(
            blaulicht_engine::config::PageRenderMode::Dynamic,
            egui::vec2(f32::NAN, f32::INFINITY),
        );
        assert_eq!(context.size, egui::vec2(1.0, 1.0));
        assert!(context.short);
    }

    #[test]
    fn responsive_horizontal_row_does_not_consume_page_height() {
        let ctx = egui::Context::default();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 600.0),
            )),
            ..Default::default()
        };

        let _ = ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let render_context = PageRenderContext::new(
                    blaulicht_engine::config::PageRenderMode::Default,
                    ui.available_size(),
                );
                let row = render_context.horizontal(ui, egui::Align::Min, |ui| {
                    ui.button("Action");
                });

                assert!(row.response.rect.height() < 100.0);
                assert!(ui.available_height() > 400.0);
            });
        });
    }
}
