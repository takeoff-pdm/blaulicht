use crate::{
    app::{
        components::{self, clickable, ButtonSize, Dialog},
        pages::health::{
            render_dmx_or_artnet_health_box, ARTNET_ICON, DMX_OR_ARTNET_HEALTH_LABEL_COLOR,
        },
        BlaulichtApp,
    },
    msg::SystemMessage,
    state::ArtNetReceiver,
};
use blaulicht_shared::LogLevel;
use egui::{Color32, Context, FontId, Frame, Label, Margin, RichText, ScrollArea, Widget};
use std::net::SocketAddr;

const ARTNET_RECEIVER_LIST_MAX_HEIGHT: f32 = 220.0;

fn artnet_receiver_scroll<R>(
    ui: &mut egui::Ui,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::containers::scroll_area::ScrollAreaOutput<R> {
    ScrollArea::vertical()
        .id_salt("artnet-receiver-list")
        .auto_shrink([false, true])
        .max_height(ARTNET_RECEIVER_LIST_MAX_HEIGHT)
        .show(ui, add_contents)
}

impl BlaulichtApp {
    pub fn render_artnet_dialog(&mut self, ctx: &Context) {
        if !self.system_ui_state.artnet_dialog_open {
            return;
        }

        let artnet_healthy = self
            .data
            .state
            .health_data
            .read()
            .unwrap()
            .artnet_health_state;

        Dialog::new("ArtNet".to_string(), egui::vec2(500.0, 400.0))
            .with_backdrop()
            .show(ctx, |ui| {
                ui.heading(RichText::new("ArtNet").strong());

                ui.add_space(12.0);

                ui.horizontal(|ui| {
                    render_dmx_or_artnet_health_box(
                        ui,
                        Label::new(
                            RichText::new("ArtNet")
                                .color(DMX_OR_ARTNET_HEALTH_LABEL_COLOR)
                                .size(12.0),
                        ),
                        ARTNET_ICON,
                        28.0,
                        artnet_healthy,
                        false,
                        egui::vec2(60.0, 65.0),
                    );

                    ui.add_space(16.0);

                    ui.vertical(|ui| {
                        let base_button = ButtonSize::Medium.dim();
                        let input_font_size = base_button.1 + 7.0;

                        let create_clicked = ui
                            .horizontal(|ui| {
                                components::TextInput::new(170.0)
                                    .with_hint_text("IPv4 Address")
                                    .ui(
                                        ui,
                                        &mut self.system_ui_state.new_artnet_address,
                                    );
                                components::TextInput::new(90.0)
                                    .with_hint_text("Port")
                                    .ui(ui, &mut self.system_ui_state.new_artnet_port);

                                components::button(ui, true, "Create", ButtonSize::Medium)
                            })
                            .inner;

                        if create_clicked {
                            let address = self.system_ui_state.new_artnet_address.trim().to_string();
                            let port_text = self.system_ui_state.new_artnet_port.trim().to_string();
                            let mut error_message = None;

                            if address.is_empty() || port_text.is_empty() {
                                error_message = Some(
                                    "Address and port are required to add an ArtNet receiver."
                                        .to_string(),
                                );
                            } else {
                                match port_text.parse::<u16>() {
                                    Ok(port) => match format!("{address}:{port}").parse::<SocketAddr>()
                                    {
                                        Ok(socket_addr) if socket_addr.is_ipv4() => {
                                            let mut artnet_output =
                                                self.data.state.artnet_output.write().unwrap();

                                            if artnet_output
                                                .receivers
                                                .iter()
                                                .any(|existing| existing.address == socket_addr)
                                            {
                                                error_message = Some(format!(
                                                    "ArtNet receiver {socket_addr} already exists."
                                                ));
                                            } else {
                                                artnet_output
                                                    .receivers
                                                    .push(ArtNetReceiver::new(socket_addr));

                                                self.data
                                                    .system_message_sender
                                                    .send(SystemMessage::Log(
                                                        format!(
                                                            "Added ArtNet receiver {socket_addr}."
                                                        ),
                                                        LogLevel::Info,
                                                    ))
                                                    .unwrap();

                                                self.system_ui_state.new_artnet_address.clear();
                                                self.system_ui_state.new_artnet_port =
                                                    "6454".to_string();
                                            }
                                        }
                                        Ok(_) => {
                                            error_message = Some(format!(
                                                "ArtNet receiver {address}:{port} must be IPv4."
                                            ));
                                        }
                                        Err(err) => {
                                            error_message = Some(format!(
                                                "Failed to parse ArtNet receiver {address}:{port}: {err}"
                                            ));
                                        }
                                    },
                                    Err(err) => {
                                        error_message = Some(format!(
                                            "Invalid ArtNet port '{port_text}': {err}"
                                        ));
                                    }
                                }
                            }

                            self.system_ui_state.artnet_input_error = error_message;
                        }

                        if let Some(error) = &self.system_ui_state.artnet_input_error {
                            ui.add_space(4.0);
                            ui.label(RichText::new(error).color(Color32::RED));
                        }

                        ui.add_space(10.0);

                        let receivers =
                            { self.data.state.artnet_output.read().unwrap().receivers.clone() };

                        ui.label(RichText::new("Configured Outputs").strong());

                        if receivers.is_empty() {
                            ui.label("No ArtNet receivers configured.");
                        } else {
                            let row_height = base_button.0.y.max(58.0);

                            artnet_receiver_scroll(ui, |ui| {
                                for (receiver_index, receiver) in
                                    receivers.into_iter().enumerate()
                                {
                                    self.render_artnet_receiver_row(
                                        ui,
                                        receiver_index,
                                        &receiver,
                                        input_font_size,
                                        row_height,
                                    );
                                }
                            });
                        }
                    });
                });

                if components::button(ui, true, "Close", ButtonSize::Medium) {
                    self.system_ui_state.artnet_input_error = None;
                    self.system_ui_state.artnet_dialog_open = false;
                }
            });
    }

    fn render_artnet_receiver_row(
        &mut self,
        ui: &mut egui::Ui,
        receiver_index: usize,
        receiver: &ArtNetReceiver,
        input_font_size: f32,
        row_height: f32,
    ) {
        let mut row_enabled = receiver.enabled;
        let mut toggle_changed = false;
        let mut delete_clicked = false;
        let owner_plugin_id = receiver.owner_plugin_id;
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
                        let (status_text, status_color) = if row_enabled {
                            ("ENABLED", Color32::from_rgb(67, 209, 110))
                        } else {
                            ("DISABLED", Color32::from_rgb(226, 69, 69))
                        };

                        ui.vertical(|ui| {
                            ui.label(
                                RichText::new(receiver.address.to_string())
                                    .font(FontId::monospace(input_font_size)),
                            );
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(status_text).color(status_color).strong());

                                if let Some(pid) = owner_plugin_id {
                                    ui.add_space(8.0);
                                    ui.label(
                                        RichText::new(format!("PLUGIN({pid})"))
                                            .color(Color32::from_rgb(120, 170, 255))
                                            .strong(),
                                    );
                                }
                            });
                        });

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if !is_plugin_owned {
                                let delete_color = Color32::from_rgb(160, 45, 45);
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
                                            FontId::monospace(input_font_size),
                                            fg_color,
                                        );
                                    },
                                ) {
                                    delete_clicked = true;
                                }
                                ui.add_space(8.0);
                            }

                            if components::Switch::new(&mut row_enabled).ui(ui).changed() {
                                toggle_changed = true;
                            }
                        });
                    },
                );
            });

        if toggle_changed {
            let mut output = self.data.state.artnet_output.write().unwrap();
            if let Some(entry) = output.receivers.iter_mut().find(|entry| {
                entry.address == receiver.address
                    && entry.owner_plugin_id == receiver.owner_plugin_id
                    && entry.handle == receiver.handle
            }) {
                entry.enabled = row_enabled;
            } else {
                tracing::warn!("Art-Net receiver row {receiver_index} changed while editing");
            }
        }

        if delete_clicked && !is_plugin_owned {
            let removed = {
                let mut output = self.data.state.artnet_output.write().unwrap();
                output
                    .receivers
                    .iter()
                    .position(|entry| {
                        entry.address == receiver.address && entry.owner_plugin_id.is_none()
                    })
                    .map(|index| output.receivers.remove(index))
            };
            if removed.is_some() {
                self.data
                    .system_message_sender
                    .send(SystemMessage::Log(
                        format!("Removed ArtNet receiver {}.", receiver.address),
                        LogLevel::Info,
                    ))
                    .unwrap();
            }
        }
    }
}

#[cfg(test)]
mod layout_tests {
    use super::artnet_receiver_scroll;

    #[test]
    fn receiver_list_scrolls_and_keeps_actions_below_it_visible() {
        let ctx = egui::Context::default();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(500.0, 400.0),
            )),
            ..Default::default()
        };
        let mut sizes = None;
        let mut close_button_rect = None;

        let _ = ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let output = artnet_receiver_scroll(ui, |ui| {
                    for receiver in 0..10 {
                        ui.label(format!("192.0.2.{receiver}:6454"));
                        ui.allocate_space(egui::vec2(ui.available_width(), 50.0));
                    }
                });
                sizes = Some((output.content_size, output.inner_rect.size()));
                close_button_rect = Some(ui.button("Close").rect);
            });
        });

        let (content, viewport) = sizes.unwrap();
        assert!(content.y > viewport.y, "{content:?} vs {viewport:?}");
        assert!(viewport.y <= 220.0);
        assert!(close_button_rect.unwrap().max.y <= 400.0);
    }
}
