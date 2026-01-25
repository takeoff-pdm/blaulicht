use crate::{
    app::{
        components::{self, clickable, ButtonSize, Dialog},
        pages::health::{
            render_dmx_or_artnet_health_box, ARTNET_ICON, DMX_OR_ARTNET_HEALTH_LABEL_COLOR,
        },
        ui::FileDialogOpenOrigin,
        BlaulichtApp, PopupSpec,
    },
    audio::defs::AudioThreadControlSignal,
    config,
    mainloop::DMX_TICK_TIME,
    msg::{FromFrontend, SystemMessage},
    plugin::midi::MidiError,
    state::{
        ArtNetReceiver, DmxHealthState, MidiDeviceState, PluginOpenState, ScreenId,
        SerialDeviceState, NUM_DMX_UNIVERSES,
    },
};
use blaulicht_assets::icons;
use blaulicht_shared::{
    ControlEvent, ControlEventMessage, EventOriginator, LogLevel, MainUiEvent, SaveEngineState,
    Showfile, ShowfileArtNetReceiver, ShowfileArtNetState,
};
use egui::{
    Color32, Context, FontFamily, FontId, Frame, Label, Margin, RichText, ThemePreference, Vec2,
    Widget,
};
use egui_extras::{Column, TableBuilder};
use egui_file::FileDialog;
use std::{
    ffi::OsStr,
    mem,
    net::SocketAddr,
    path::{Path, PathBuf},
    process::Command,
    str::FromStr,
    time::Duration,
};

impl BlaulichtApp {
    pub fn render_artnet_dialog(&mut self, ctx: &Context) {
        if !self.system_ui_state.artnet_dialog_open {
            return;
        }

        let health_state = self.data.state.health_data.read().unwrap();

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
                        health_state.artnet_health_state,
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
                                                self.system_ui_state.new_artnet_port.clear();
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
                            let row_height = base_button.0.y.max(44.0);

                            for (receiver_index, receiver) in receivers.into_iter().enumerate() {
                                let mut row_enabled = receiver.enabled;
                                let mut toggle_changed = false;
                                let mut delete_clicked = false;

                                ui.add_space(6.0);

                                Frame::none()
                                    .fill(ui.visuals().faint_bg_color)
                                    .inner_margin(Margin::symmetric(12, 4))
                                    .show(ui, |ui| {
                                        ui.set_min_height(row_height);
                                        ui.allocate_ui_with_layout(
                                            egui::vec2(ui.available_width(), row_height),
                                            egui::Layout::left_to_right(egui::Align::Center),
                                            |ui| {
                                                ui.label(
                                                    RichText::new(receiver.address.to_string())
                                                        .font(FontId::monospace(input_font_size)),
                                                );

                                                ui.add_space(12.0);

                                                let status_text = if row_enabled {
                                                    "ENABLED"
                                                } else {
                                                    "DISABLED"
                                                };
                                                let status_color = if row_enabled {
                                                    Color32::from_rgb(67, 209, 110)
                                                } else {
                                                    Color32::from_rgb(226, 69, 69)
                                                };

                                                ui.label(
                                                    RichText::new(status_text)
                                                        .color(status_color)
                                                        .strong(),
                                                );

                                                ui.with_layout(
                                                    egui::Layout::right_to_left(egui::Align::Center),
                                                    |ui| {
                                                        let delete_color =
                                                            Color32::from_rgb(160, 45, 45);
                                                        if clickable(
                                                            ui,
                                                            false,
                                                            delete_color,
                                                            ButtonSize::Medium.with_width(
                                                                row_height,
                                                            ),
                                                            |ui, rect, fg_color| {
                                                                ui.painter().text(
                                                                    rect.center(),
                                                                    egui::Align2::CENTER_CENTER,
                                                                    egui_phosphor::regular::TRASH,
                                                                    FontId::monospace(
                                                                        input_font_size,
                                                                    ),
                                                                    fg_color,
                                                                );
                                                            },
                                                        ) {
                                                            delete_clicked = true;
                                                        }

                                                        ui.add_space(8.0);

                                                        if components::Switch::new(
                                                            &mut row_enabled,
                                                        )
                                                        .ui(ui)
                                                        .changed()
                                                        {
                                                            toggle_changed = true;
                                                        }
                                                    },
                                                );
                                            },
                                        );
                                    });

                                if toggle_changed {
                                    let mut artnet_output =
                                        self.data.state.artnet_output.write().unwrap();
                                    if let Some(entry) =
                                        artnet_output.receivers.get_mut(receiver_index)
                                    {
                                        entry.enabled = row_enabled;
                                    }
                                }

                                if delete_clicked {
                                    let mut artnet_output =
                                        self.data.state.artnet_output.write().unwrap();
                                    if receiver_index < artnet_output.receivers.len() {
                                        artnet_output.receivers.remove(receiver_index);
                                    }
                                    drop(artnet_output);

                                    self.data
                                        .system_message_sender
                                        .send(SystemMessage::Log(
                                            format!(
                                                "Removed ArtNet receiver {}.",
                                                receiver.address
                                            ),
                                            LogLevel::Info,
                                        ))
                                        .unwrap();
                                }
                            }
                        }
                    });
                });

                if components::button(ui, true, "Close", ButtonSize::Medium) {
                    self.system_ui_state.artnet_input_error = None;
                    self.system_ui_state.artnet_dialog_open = false;
                }
            });
    }
}
