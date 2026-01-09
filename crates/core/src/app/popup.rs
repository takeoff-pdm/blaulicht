use crate::app::{
    components::{self, ButtonSize},
    BlaulichtApp, PopupSpec,
};
use egui::{Color32, CornerRadius, Frame, Margin, RichText, Stroke};
use std::time::{Duration, Instant};

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
            let label = popup.label.clone();
            let button = popup.button.clone();
            let lifetime_duration = popup.lifetime_duration;

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

            egui::Window::new(&label)
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
                        ui.label(RichText::new(&label).size(24.0));

                        if let Some(ref btn) = button {
                            ui.add_space(8.0);

                            if components::button(ui, false, &btn.label, ButtonSize::Large) {
                                self.close_popup();
                            }

                            ui.add_space(8.0);
                        }

                        let progress =
                            1.0 - elapsed.as_millis() as f32 / lifetime_duration.as_millis() as f32;

                        let text = format!(
                            "{} seconds remaining",
                            lifetime_duration.as_secs() - elapsed.as_secs()
                        );

                        Self::draw_progress_bar(ui, progress, 18.0, &text);
                    })
                });
        }
    }

    pub fn render_init_popup(&mut self, ctx: &egui::Context) {
        const INIT_POPUP_DURATION: Duration = Duration::from_secs(2);

        let open_elapsed = self.init_popup_open_time.elapsed();

        if open_elapsed > INIT_POPUP_DURATION {
            return;
        }

        let screen_rect = ctx.screen_rect();
        let popup_size = egui::Vec2::new(370.0, 250.0); // desired popup size

        let center_pos = egui::Pos2::new(
            screen_rect.center().x - popup_size.x / 2.0,
            screen_rect.center().y - popup_size.y / 2.0,
        );

        render_popup_backdrop(ctx);

        egui::Window::new("Initializing...")
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
                    ui.label(
                        RichText::new("Initializing...")
                            .size(14.0)
                            .color(Color32::from_gray(150)),
                    );

                    // if let Some(ref btn) = button {
                    //     ui.add_space(8.0);
                    //
                    //     if components::button(ui, false, &btn.label, ButtonSize::Large) {
                    //         self.close_popup();
                    //     }
                    //
                    //     ui.add_space(8.0);
                    // }

                    ui.add(egui::Image::new(blaulicht_assets::LOGO_IMAGE));

                    let progress = 1.0
                        - open_elapsed.as_millis() as f32 / INIT_POPUP_DURATION.as_millis() as f32;

                    let text = format!(
                        "{} seconds remaining",
                        INIT_POPUP_DURATION.as_secs() - open_elapsed.as_secs()
                    );

                    Self::draw_progress_bar(ui, progress, 18.0, &text);
                })
            });
    }
}

pub fn render_popup_backdrop(ctx: &egui::Context) {
    egui::Area::new("init_modal_backdrop".into())
        .interactable(true) // Blocks clicks from going through to the widgets behind
        .fixed_pos(egui::pos2(0.0, 0.0))
        .order(egui::Order::Background)
        .show(ctx, |ui| {
            // Get the full screen rect
            let screen_rect = ctx.screen_rect();

            // Allocate a rect that covers the whole screen to catch clicks
            let response = ui.allocate_rect(screen_rect, egui::Sense::click());

            // Optional: Close popup if user clicks the dark background
            if response.clicked() {
                {}
            }

            // Paint the semi-transparent black color
            let painter = ui.painter();
            painter.rect_filled(
                screen_rect,
                egui::CornerRadius::ZERO,
                egui::Color32::from_black_alpha(180), // Adjust alpha for darkness (0-255)
            );
        });
}
