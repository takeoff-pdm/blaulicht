use crate::app::{
    components::{self, ButtonSize, Dialog},
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

            let center_pos = egui::vec2(
                screen_rect.center().x - popup_size.x / 2.0,
                screen_rect.center().y - popup_size.y / 2.0,
            );

            let elapsed = self.popup_open_time.elapsed();
            if elapsed >= popup.lifetime_duration {
                self.close_popup();
            }

            components::Dialog::new(label.clone(), popup_size)
                .with_backdrop()
                .fixed_pos(center_pos)
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
                            lifetime_duration
                                .as_secs()
                                .saturating_sub(elapsed.as_secs())
                        );

                        Self::draw_progress_bar(ui, progress, 18.0, &text);
                    });
                });
        }
    }

    pub fn render_init_popup(&mut self, ctx: &egui::Context) -> bool {
        const INIT_POPUP_DURATION: Duration = Duration::from_secs(2);

        let open_elapsed = self.init_popup_open_time.elapsed();

        if open_elapsed > INIT_POPUP_DURATION {
            return false;
        }

        let screen_rect = ctx.screen_rect();
        let popup_size = egui::Vec2::new(370.0, 250.0); // desired popup size

        let center_pos = egui::Pos2::new(
            screen_rect.center().x - popup_size.x / 2.0,
            screen_rect.center().y - popup_size.y / 2.0,
        );

        let order = egui::Order::Foreground;
        let window_id = egui::Id::new("init_modal_window");
        let (backdrop_layer, _clicked) = Dialog::render_popup_backdrop(
            ctx,
            window_id.with("backdrop"),
            order,
            Some(Color32::BLACK),
        );
        let window_layer = egui::LayerId::new(order, window_id);

        ctx.memory_mut(|mem| {
            let areas = mem.areas_mut();
            areas.move_to_top(window_layer);
            areas.set_sublayer(backdrop_layer, window_layer);
            mem.set_modal_layer(window_layer);
        });

        const INIT_TITLE: &str = concat!("Initializing (v", env!("CARGO_PKG_VERSION"), ") ...");

        egui::Window::new(INIT_TITLE)
            .id(window_id)
            .fixed_size(popup_size)
            .collapsible(false)
            .resizable(false)
            .title_bar(false)
            .fixed_pos(center_pos)
            .order(order)
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
                        RichText::new(INIT_TITLE)
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

                    ui.label(
                        self.log_window
                            .logs
                            .iter()
                            .last()
                            .map(|l| l.message.clone())
                            .unwrap_or_default(),
                    );

                    let progress = 1.0
                        - open_elapsed.as_millis() as f32 / INIT_POPUP_DURATION.as_millis() as f32;

                    let text = format!(
                        "{} seconds remaining",
                        INIT_POPUP_DURATION.as_secs() - open_elapsed.as_secs()
                    );

                    Self::draw_progress_bar(ui, progress, 18.0, &text);
                })
            });
        true
    }
}
