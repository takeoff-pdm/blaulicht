use crate::app::BlaulichtApp;
use crate::state::ScreenId;
use blaulicht_shared::AppPage;
use egui::Context;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PageWidthClass {
    Narrow,
    Standard,
    Wide,
}

#[derive(Debug, Clone, Copy)]
pub struct PageRenderContext {
    pub(crate) mode: crate::config::PageRenderMode,
    pub(crate) size: egui::Vec2,
    pub(crate) width_class: PageWidthClass,
    pub(crate) short: bool,
}

impl PageRenderContext {
    pub(crate) fn new(mode: crate::config::PageRenderMode, size: egui::Vec2) -> Self {
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
        }
    }

    pub(crate) fn is_dynamic(self) -> bool {
        self.mode == crate::config::PageRenderMode::Dynamic
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
            AppPage::FixturesSetup => {
                self.fixtures_ui_setup(ui, ctx, render_context);
            }
            AppPage::View => {
                self.view_ui(ui, ctx, render_context);
            }
            AppPage::ViewPerformance => {
                self.view_perf_ui(ui, ctx, render_context);
            }
            AppPage::FixturesPerformance => {
                self.fixtures_ui(ui, ctx, render_context);
            }
            AppPage::Animations => {
                self.animations_ui(ctx, ui, render_context);
            }
            AppPage::Palettes => {
                self.palettes_ui(ui, ctx, render_context);
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
                crate::config::PageRenderMode::Dynamic,
                egui::vec2(719.0, 480.0)
            )
            .width_class,
            PageWidthClass::Narrow
        );
        assert_eq!(
            PageRenderContext::new(
                crate::config::PageRenderMode::Dynamic,
                egui::vec2(720.0, 480.0)
            )
            .width_class,
            PageWidthClass::Standard
        );
        assert_eq!(
            PageRenderContext::new(
                crate::config::PageRenderMode::Dynamic,
                egui::vec2(1_200.0, 480.0)
            )
            .width_class,
            PageWidthClass::Wide
        );
    }

    #[test]
    fn invalid_dimensions_are_sanitized() {
        let context = PageRenderContext::new(
            crate::config::PageRenderMode::Dynamic,
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
                    crate::config::PageRenderMode::Default,
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
