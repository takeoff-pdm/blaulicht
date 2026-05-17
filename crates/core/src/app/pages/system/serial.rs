use crate::{
    app::{
        components::{self, ButtonSize, Dialog},
        BlaulichtApp,
    },
    state::SerialDeviceState,
};
use egui::{Color32, Context, FontFamily, Frame, Margin, RichText};
use egui_extras::{Column, TableBuilder};

impl BlaulichtApp {
    pub fn render_serial_dialog(&mut self, ctx: &Context) {
        if !self.system_ui_state.serial_dialog_open {
            return;
        }

        let serial_health = {
            let health_state = self.data.state.health_data.read().unwrap();
            health_state.serial_health.clone()
        };

        let mut devices: Vec<(String, SerialDeviceState)> = Vec::new();

        for (device, state) in serial_health.devices.iter() {
            devices.push((device.clone(), state.clone()));
        }

        devices.sort_by(|a, b| a.0.cmp(&b.0));

        Dialog::new("Serial".to_string(), egui::vec2(520.0, 360.0))
            .with_backdrop()
            .show(ctx, |ui| {
                ui.heading(RichText::new("Serial Devices").strong());

                ui.add_space(12.0);

                Frame::NONE
                    .inner_margin(Margin::symmetric(16, 0))
                    .show(ui, |frame_ui| {
                        let table = TableBuilder::new(frame_ui)
                            .striped(true)
                            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                            .column(Column::exact(200.0))
                            .column(Column::remainder());

                        table
                            .header(32.0, |mut header| {
                                for heading in ["Device", "Status"] {
                                    header.col(|ui| {
                                        ui.label(RichText::new(heading).strong());
                                    });
                                }
                            })
                            .body(|mut body| {
                                for (device, state) in &devices {
                                    body.row(32.0, |mut row| {
                                        row.col(|ui| {
                                            ui.label(
                                                RichText::new(device)
                                                    .family(FontFamily::Monospace)
                                                    .strong(),
                                            );
                                        });
                                        row.col(|ui| match state {
                                            SerialDeviceState::Open(handles) => {
                                                let text = if *handles > 1 {
                                                    format!("CONNECTED ({} handles)", handles)
                                                } else {
                                                    "CONNECTED".to_string()
                                                };
                                                ui.label(
                                                    RichText::new(text)
                                                        .color(Color32::from_rgb(0, 200, 0))
                                                        .family(FontFamily::Monospace),
                                                );
                                            }
                                            SerialDeviceState::Error(err) => {
                                                ui.label(
                                                    RichText::new(err.to_string())
                                                        .color(Color32::RED)
                                                        .family(FontFamily::Proportional),
                                                );
                                            }
                                        });
                                    });
                                }
                            });
                    });

                ui.add_space(20.0);

                if components::button(ui, true, "Close", ButtonSize::Medium) {
                    self.system_ui_state.serial_dialog_open = false;
                }
            });
    }
}
