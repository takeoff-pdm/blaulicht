use crate::{
    app::{
        components::{self, clickable, ButtonSize, Dialog},
        BlaulichtApp,
    },
    msg::SystemMessage,
};
use blaulicht_shared::LogLevel;
use egui::{Color32, Context, FontId, Frame, Margin, RichText, ScrollArea};

impl BlaulichtApp {
    pub fn render_screens_dialog(&mut self, ctx: &Context) {
        if !self.system_ui_state.screens_dialog_open {
            return;
        }

        Dialog::new("Screens".to_string(), egui::vec2(560.0, 460.0))
            .with_backdrop()
            .show(ctx, |ui| {
                ui.heading(RichText::new("External Screens").strong());

                ui.add_space(8.0);
                ui.label(
                    RichText::new(
                        "External screens are extra windows mirroring the UI. They are restored \
                         from the showfile on startup.",
                    )
                    .color(Color32::from_gray(150)),
                );

                ui.add_space(12.0);

                if components::button(ui, true, "Add Screen", ButtonSize::Medium) {
                    self.add_external_screen();
                }

                ui.add_space(12.0);
                ui.separator();
                ui.add_space(12.0);

                let count = self.external_screens.len();
                ui.label(RichText::new(format!("Active Screens ({count})")).strong());

                if self.external_screens.is_empty() {
                    ui.add_space(6.0);
                    ui.label("No external screens.");
                    self.render_screens_close_button(ui);
                    return;
                }

                let row_height = ButtonSize::Medium.dim().0.y.max(44.0);
                let mut delete_index: Option<usize> = None;

                ScrollArea::vertical()
                    .auto_shrink([false, true])
                    .max_height(280.0)
                    .show(ui, |ui| {
                        for (index, screen) in self.external_screens.iter().enumerate() {
                            let dimensions = screen.dimensions;
                            let position = screen.position;
                            let owner_plugin_id = screen.owner_plugin_id;
                            let is_plugin_owned = owner_plugin_id.is_some();

                            ui.add_space(6.0);

                            Frame::NONE
                                .fill(ui.visuals().faint_bg_color)
                                .inner_margin(Margin::symmetric(12, 4))
                                .show(ui, |ui| {
                                    ui.set_min_height(row_height);
                                    ui.allocate_ui_with_layout(
                                        egui::vec2(ui.available_width(), row_height),
                                        egui::Layout::left_to_right(egui::Align::Center),
                                        |ui| {
                                            ui.label(
                                                RichText::new(format!("#{index}"))
                                                    .font(FontId::monospace(15.0))
                                                    .strong(),
                                            );

                                            ui.add_space(12.0);

                                            let geometry = match position {
                                                Some(pos) => format!(
                                                    "{:.0}x{:.0}  @ ({:.0}, {:.0})",
                                                    dimensions.x, dimensions.y, pos.x, pos.y
                                                ),
                                                None => format!(
                                                    "{:.0}x{:.0}",
                                                    dimensions.x, dimensions.y
                                                ),
                                            };
                                            ui.label(
                                                RichText::new(geometry)
                                                    .font(FontId::monospace(13.0)),
                                            );

                                            if let Some(pid) = owner_plugin_id {
                                                ui.add_space(8.0);
                                                ui.label(
                                                    RichText::new(format!("PLUGIN({pid})"))
                                                        .color(Color32::from_rgb(120, 170, 255))
                                                        .strong(),
                                                );
                                            }

                                            ui.with_layout(
                                                egui::Layout::right_to_left(egui::Align::Center),
                                                |ui| {
                                                    if is_plugin_owned {
                                                        return;
                                                    }

                                                    let delete_color =
                                                        Color32::from_rgb(160, 45, 45);
                                                    if clickable(
                                                        ui,
                                                        false,
                                                        delete_color,
                                                        ButtonSize::Medium.with_width(row_height),
                                                        |ui, rect, fg_color| {
                                                            ui.painter().text(
                                                                rect.center(),
                                                                egui::Align2::CENTER_CENTER,
                                                                egui_phosphor::regular::TRASH,
                                                                FontId::monospace(18.0),
                                                                fg_color,
                                                            );
                                                        },
                                                    ) {
                                                        delete_index = Some(index);
                                                    }
                                                },
                                            );
                                        },
                                    );
                                });
                        }
                    });

                if let Some(index) = delete_index {
                    if self.remove_external_screen(index) {
                        self.data
                            .system_message_sender
                            .send(SystemMessage::Log(
                                format!("Removed external screen #{index}."),
                                LogLevel::Info,
                            ))
                            .unwrap();
                    }
                }

                self.render_screens_close_button(ui);
            });
    }

    fn render_screens_close_button(&mut self, ui: &mut egui::Ui) {
        ui.add_space(12.0);
        if components::button(ui, true, "Close", ButtonSize::Medium) {
            self.system_ui_state.screens_dialog_open = false;
        }
    }
}
