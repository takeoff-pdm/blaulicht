use crate::app::{
    components::{self, ButtonSize, Dialog},
    BlaulichtApp, PopupSpec,
};
use crate::msg::StageState;
use egui::{Color32, CornerRadius, Frame, Margin, RichText, Stroke};
use std::time::{Duration, Instant};

const DEFAULT_POPUP_LABEL_SIZE: f32 = 24.0;

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

            let size = popup.label_size.unwrap_or(DEFAULT_POPUP_LABEL_SIZE);

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
                        ui.label(RichText::new(&label).size(size));

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
        const INIT_POPUP_MIN_DURATION: Duration = if cfg!(debug_assertions) {
            Duration::from_millis(500)
        } else {
            Duration::from_secs(3)
        };
        // Failsafe: never trap the UI behind the modal if the engine hangs during init.
        const INIT_POPUP_MAX_DURATION: Duration = Duration::from_secs(15);

        let open_elapsed = self.init_popup_open_time.elapsed();

        if (self.engine_initialization_complete || open_elapsed > INIT_POPUP_MAX_DURATION)
            && open_elapsed > INIT_POPUP_MIN_DURATION
        {
            return false;
        }

        let screen_rect = ctx.screen_rect();
        const LOGO_HEIGHT: f32 = 110.0;
        let popup_size = egui::Vec2::new(370.0, 330.0); // desired popup size

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

                    ui.add(egui::Image::new(blaulicht_assets::LOGO_IMAGE).max_height(LOGO_HEIGHT));
                    ui.add_space(4.0);

                    self.render_startup_checklist(ui);
                    ui.add_space(8.0);

                    let current_log = self
                        .log_window
                        .logs
                        .iter()
                        .last()
                        .map(|l| l.message.lines().next().unwrap_or_default().to_owned())
                        .unwrap_or_default();
                    ui.add(egui::Label::new(current_log).truncate());
                })
            });
        true
    }

    fn render_startup_checklist(&self, ui: &mut egui::Ui) {
        const ICON_WIDTH: f32 = 20.0;
        const LABEL_WIDTH: f32 = 90.0;
        const ROW_HEIGHT: f32 = 20.0;

        for (stage, status) in self.startup_progress.iter() {
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), ROW_HEIGHT),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.add_space(12.0);
                    ui.allocate_ui_with_layout(
                        egui::vec2(ICON_WIDTH, ROW_HEIGHT),
                        egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
                        |ui| match stage_icon(status.state) {
                            Some((icon, color)) => {
                                ui.label(RichText::new(icon).size(16.0).color(color));
                            }
                            None => {
                                ui.add(egui::Spinner::new().size(14.0));
                            }
                        },
                    );
                    let label_color = match status.state {
                        StageState::Pending => Color32::from_gray(120),
                        StageState::Failed => Color32::LIGHT_RED,
                        _ => ui.visuals().text_color(),
                    };
                    ui.allocate_ui_with_layout(
                        egui::vec2(LABEL_WIDTH, ROW_HEIGHT),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| ui.label(RichText::new(stage.label()).color(label_color)),
                    );
                    if let Some(detail) = &status.detail {
                        ui.add(
                            egui::Label::new(RichText::new(detail).color(Color32::from_gray(150)))
                                .truncate(),
                        );
                    }
                },
            );
        }
    }
}

/// Checklist icon and color for a stage; `None` means "draw a spinner".
fn stage_icon(state: StageState) -> Option<(&'static str, Color32)> {
    use egui_phosphor::regular as icons;
    match state {
        StageState::Pending => Some((icons::CIRCLE, Color32::from_gray(100))),
        StageState::Ok => Some((icons::CHECK_CIRCLE, Color32::from_rgb(90, 200, 110))),
        StageState::Failed => Some((icons::X_CIRCLE, Color32::LIGHT_RED)),
        StageState::Running | StageState::Restarting => None,
    }
}
