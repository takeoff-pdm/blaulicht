use crate::app::{
    components::{self, ButtonSize},
    BlaulichtApp, PopupSpec,
};
use egui::{Color32, CornerRadius, Frame, Margin, RichText, Stroke};
use std::time::Instant;

impl BlaulichtApp {
    pub fn show_popup(&mut self, popup: PopupSpec) {
        self.popup = Some(popup);
        self.popup_open_time = Instant::now();
    }

    pub fn close_popup(&mut self) {
        self.popup = None;
    }

    pub fn render_popup(&mut self, ctx: &egui::Context) {
        if let Some(ref popup) = self.popup {
            let popup = popup.clone();

            let screen_rect = ctx.screen_rect();
            let popup_size = egui::Vec2::new(200.0, 100.0); // desired popup size

            let center_pos = egui::Pos2::new(
                screen_rect.center().x - popup_size.x / 2.0,
                screen_rect.center().y - popup_size.y / 2.0,
            );

            let elapsed = self.popup_open_time.elapsed();
            if elapsed >= popup.lifetime_duration {
                self.close_popup();
            }

            egui::Window::new(&popup.label)
                .fixed_size(popup_size)
                .collapsible(false)
                .resizable(false)
                .title_bar(false)
                .fixed_pos(center_pos)
                .frame(Frame {
                    corner_radius: CornerRadius::same(1),
                    fill: Color32::from_gray(40),
                    stroke: Stroke::new(1.0, Color32::from_gray(60)),
                    inner_margin: Margin::symmetric(6, 12),
                    ..Frame::default()
                })
                .show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new(&popup.label).size(24.0));

                        if let Some(ref btn) = popup.button {
                            ui.add_space(8.0);

                            if components::button(ui, false, &btn.label, ButtonSize::Large) {
                                self.close_popup();
                            }

                            ui.add_space(8.0);
                        }

                        let progress = 1.0
                            - elapsed.as_millis() as f32
                                / popup.lifetime_duration.as_millis() as f32;

                        let text = format!(
                            "{} seconds remaining",
                            popup.lifetime_duration.as_secs() - elapsed.as_secs()
                        );

                        Self::draw_progress_bar(ui, progress, 18.0, &text);
                    })
                });
        }
    }
}
