use std::{
    ffi::OsStr,
    mem,
    net::SocketAddr,
    path::{Path, PathBuf},
    process::Command,
    str::FromStr,
    time::Duration,
};

use blaulicht_assets::icons;
use blaulicht_shared::LogLevel;
use egui::{
    Align, Color32, Context, FontFamily, FontId, Frame, Label, Margin, RichText, TextEdit,
    ThemePreference, Vec2, Widget,
};
use egui_extras::{Column, TableBuilder};
use egui_file::FileDialog;

use crate::{
    app::{
        components::{self, clickable, ButtonSize, Dialog},
        ui::FileDialogOpenOrigin,
        BlaulichtApp, PopupSpec,
    },
    audio::defs::AudioThreadControlSignal,
    config,
    mainloop::DMX_TICK_TIME,
    msg::{FromFrontend, SystemMessage},
    plugin::midi::MidiError,
    plugin::serial::SerialError,
    state::{ArtNetReceiver, MidiDeviceState},
    state::{DmxHealthState, SerialDeviceState, NUM_DMX_UNIVERSES},
};

const DMX_ICON: &str = icons::DMX;
const ARTNET_ICON: &str = egui_phosphor::regular::NETWORK;
const MIDI_ICON: &str = icons::MIDI;
const SERIAL_ICON: &str = egui_phosphor::regular::PLUG;
const DMX_OR_ARTNET_HEALTH_LABEL_COLOR: Color32 = Color32::from_gray(150);

pub struct SystemUI {
    open_file_dialog: Option<FileDialog>,
    file_dialog_open_origin: FileDialogOpenOrigin,
    reload_dialog_open: bool,
    confirm_shutdown_open: bool,
    pub debug_open: bool,
    artnet_dialog_open: bool,
    midi_dialog_open: bool,
    serial_dialog_open: bool,
    dmx_dialogs_open: [bool; NUM_DMX_UNIVERSES],
    new_artnet_address: String,
    new_artnet_port: String,
    artnet_input_error: Option<String>,
}

impl Default for SystemUI {
    fn default() -> Self {
        Self {
            open_file_dialog: None,
            file_dialog_open_origin: FileDialogOpenOrigin::Load,
            reload_dialog_open: false,
            confirm_shutdown_open: false,
            debug_open: false,
            artnet_dialog_open: false,
            midi_dialog_open: false,
            serial_dialog_open: false,
            dmx_dialogs_open: [false; NUM_DMX_UNIVERSES],
            new_artnet_address: String::new(),
            new_artnet_port: String::new(),
            artnet_input_error: None,
        }
    }
}

impl BlaulichtApp {
    //
    // TODO: i need to remove this
    //
    fn save_showfile(&mut self) {
        let mut config_mut = self.data.config.lock().unwrap();

        match config_mut.last_open_showfile.clone() {
            Some(ref path) => {
                let mut dmx = self.data.state.dmx_engine.write().unwrap();

                {
                    let plugin_state = self.data.state.plugin_state_storage.lock().unwrap();
                    dmx.0.plugin_state = plugin_state.clone();
                }

                // let string = ron::to_string(&dmx.clone()).unwrap();
                // let pretty = PrettyConfig::new()
                //     .indentor("    ".to_owned())
                //     .new_line("\n".to_owned());

                // let string = serde_json::to_string_pretty(&dmx.clone()).unwrap();

                // let string = ron::ser::to_string_pretty(&dmx.clone(), pretty).unwrap();
                // let string = ron::to_string(&dmx.clone()).unwrap();
                let string = blaulicht_shared::save::engine_state_to_json(dmx.0.clone()).unwrap();
                println!("STRING: {string}");

                // let serialized = postcard::to_allocvec(&dmx.clone()).unwrap();
                std::fs::write(path, &string).unwrap();

                config_mut.last_open_showfile = Some(path.clone());

                let config_path = PathBuf::from_str(&self.data.config_path).unwrap();
                config::write_config(config_path, config_mut.clone()).unwrap();

                self.data
                    .system_message_sender
                    .send(SystemMessage::Log(
                        format!("Saved showfile to {path:?}"),
                        LogLevel::Info,
                    ))
                    .unwrap();

                mem::drop(config_mut);
                mem::drop(dmx);

                self.show_popup(PopupSpec::with_duration(
                    Duration::from_secs(2),
                    "Saved Showfile".to_string(),
                ));
            }
            None => {
                self.data
                    .system_message_sender
                    .send(SystemMessage::Log(
                        "No opened showfile, not saving".to_string(),
                        LogLevel::Err,
                    ))
                    .unwrap();

                mem::drop(config_mut);

                self.show_popup(PopupSpec::with_duration(
                    Duration::from_secs(2),
                    "No Showfile".to_string(),
                ));
            }
        }
    }

    fn render_confirm_shutdown_dialog(&mut self, ctx: &Context) {
        if !self.system_ui_state.confirm_shutdown_open {
            return;
        }

        Dialog::new("Confirm Shutdown".to_string(), egui::vec2(200.0, 100.0))
            .with_backdrop()
            .show(ctx, |ui| {
                let blink = ((self.animation_time * 4.0) as i32) % 2 == 0;

                ui.heading(
                    RichText::new("Confirm Shutdown")
                        .color(if blink {
                            Color32::RED
                        } else {
                            ui.visuals().text_color()
                        })
                        .strong(),
                );

                ui.add_space(12.0);

                ui.horizontal(|ui| {
                    if components::button(ui, false, "Confirm", ButtonSize::Large) {
                        let status = Command::new("/usr/bin/shutdown.sh")
                            .status()
                            .expect("Failed to execute shutdown command");

                        if status.success() {
                            println!("Shutdown command executed successfully.");
                        } else {
                            eprintln!("Shutdown command failed!");
                        }

                        self.system_ui_state.confirm_shutdown_open = false;
                    }

                    if components::button(ui, true, "Cancel", ButtonSize::Large) {
                        self.system_ui_state.confirm_shutdown_open = false;
                    }
                });
            });
    }

    fn render_dmx_dialog(&mut self, ctx: &Context, universe_number: usize) {
        if !self.system_ui_state.dmx_dialogs_open[universe_number] {
            return;
        }

        let health_data = self.data.state.health_data.read().unwrap();
        let health_state = &health_data.dmx_universes_healthy[universe_number];

        Dialog::new("DMX".to_string(), egui::vec2(500.0, 200.0))
            .with_backdrop()
            .show(ctx, |ui| {
                let label = format!("(DMX {})", universe_number);
                ui.heading(RichText::new(&label).strong());

                ui.add_space(12.0);

                ui.horizontal_top(|ui| {
                    render_dmx_or_artnet_health_box(
                        ui,
                        Label::new(
                            RichText::new(&health_state.port)
                                .color(DMX_OR_ARTNET_HEALTH_LABEL_COLOR)
                                .size(12.0),
                        ),
                        DMX_ICON,
                        health_state.is_healthy(),
                        false,
                        egui::vec2(100.0, 65.0),
                    );

                    let label = match &health_state.state {
                        DmxHealthState::Error(err) => Label::new(
                            RichText::new(err)
                                .color(Color32::RED)
                                .family(FontFamily::Monospace),
                        )
                        .wrap(),
                        DmxHealthState::Healthy => Label::new("- No Error -"),
                    };

                    ui.add_sized([350.0, 20.0], label);
                });

                if components::button(ui, true, "Close", ButtonSize::Medium) {
                    self.system_ui_state.dmx_dialogs_open[universe_number] = false;
                }
            });
    }

    fn render_midi_dialog(&mut self, ctx: &Context) {
        if !self.system_ui_state.midi_dialog_open {
            return;
        }

        let health_data = self.data.state.health_data.read().unwrap();
        let mut available_devices = health_data.midi_health.available_devices.clone();
        let mut device_states: Vec<(String, MidiDeviceState)> = health_data
            .midi_health
            .devices
            .iter()
            .map(|(name, state)| (name.clone(), state.clone()))
            .collect();
        drop(health_data);

        available_devices.sort();
        device_states.sort_by(|a, b| a.0.cmp(&b.0));

        let dialog_size = egui::vec2(620.0, 360.0);

        Dialog::new("MIDI".to_string(), dialog_size)
            .with_backdrop()
            .show(ctx, |ui| {
                ui.set_min_size(dialog_size);
                ui.set_max_width(dialog_size.x);
                ui.set_min_height(dialog_size.y);
                ui.set_height(dialog_size.y);

                ui.heading(RichText::new("MIDI Health").strong());
                ui.add_space(12.0);

                let base_button = ButtonSize::Medium.dim();
                let row_height = base_button.0.y;
                let row_font_size = base_button.1 + 2.0;
                let detail_font_size = (row_font_size - 2.0).max(10.0);
                let left_width: f32 = 260.0;
                let right_width: f32 = 260.0;
                let status_width: f32 = 100.0;
                let status_gap: f32 = 6.0;
                let detail_width: f32 = 100.0;
                let name_max_width: f32 = 200.0;
                let name_font = FontId::monospace(row_font_size);
                let status_font = FontId::monospace(row_font_size);
                let detail_font = FontId::monospace(detail_font_size);

                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width(left_width);
                        ui.label(RichText::new("Available Devices").strong());
                        if available_devices.is_empty() {
                            ui.label("No MIDI inputs detected.");
                        } else {
                            for device in &available_devices {
                                ui.add_space(6.0);
                                Frame::none()
                                    .fill(ui.visuals().faint_bg_color)
                                    .inner_margin(Margin::symmetric(12, 4))
                                    .show(ui, |ui| {
                                        ui.set_min_size(egui::vec2(left_width, row_height));
                                        let text = elide_text(
                                            ui,
                                            device,
                                            name_font.clone(),
                                            left_width - 16.0,
                                        );
                                        ui.add_sized(
                                            [left_width, row_height],
                                            Label::new(RichText::new(text).font(name_font.clone())),
                                        );
                                    });
                            }
                        }
                    });

                    ui.add_space(20.0);

                    ui.vertical(|ui| {
                        ui.set_width(right_width);
                        ui.label(RichText::new("Device Status").strong());
                        if device_states.is_empty() {
                            ui.label("No devices opened by Blaulicht.");
                        } else {
                            for (name, state) in &device_states {
                                ui.add_space(6.0);
                                Frame::none()
                                    .fill(ui.visuals().faint_bg_color)
                                    .inner_margin(Margin::symmetric(12, 6))
                                    .show(ui, |card_ui| {
                                        card_ui.set_min_size(egui::vec2(right_width, row_height));

                                        let (status_text, status_color, detail_text) = match state {
                                            MidiDeviceState::Open(handles) => (
                                                format!("OPEN (handles: {handles})"),
                                                Color32::from_rgb(67, 209, 110),
                                                Some(format!("HANDLES: {handles}")),
                                            ),
                                            MidiDeviceState::Error(err) => {
                                                let detail = match err {
                                                    MidiError::DeviceNotFound => {
                                                        "NOT FOUND".to_string()
                                                    }
                                                    MidiError::Other(_) => "ERR MISC".to_string(),
                                                };
                                                (
                                                    "ERROR".to_string(),
                                                    Color32::from_rgb(226, 69, 69),
                                                    Some(detail),
                                                )
                                            }
                                        };

                                        let detail_space = if detail_text.is_some() {
                                            detail_width + status_gap
                                        } else {
                                            0.0
                                        };

                                        let available_name_width = (right_width
                                            - status_width
                                            - detail_space
                                            - status_gap)
                                            .max(0.0_f32);
                                        let name_width =
                                            name_max_width.min(available_name_width).max(80.0_f32);

                                        card_ui.allocate_ui_with_layout(
                                            egui::vec2(right_width, row_height),
                                            egui::Layout::left_to_right(egui::Align::Center),
                                            |row_ui| {
                                                let text = elide_text(
                                                    row_ui,
                                                    name,
                                                    name_font.clone(),
                                                    name_width,
                                                );
                                                row_ui.add_sized(
                                                    [name_width, row_height],
                                                    Label::new(
                                                        RichText::new(text)
                                                            .font(name_font.clone())
                                                            .strong(),
                                                    ),
                                                );

                                                row_ui.add_space(status_gap);

                                                let status_label = elide_text(
                                                    row_ui,
                                                    &status_text,
                                                    status_font.clone(),
                                                    status_width,
                                                );
                                                row_ui.add_sized(
                                                    [status_width, row_height],
                                                    Label::new(
                                                        RichText::new(status_label)
                                                            .color(status_color)
                                                            .font(status_font.clone())
                                                            .strong(),
                                                    ),
                                                );

                                                if let Some(detail) = detail_text {
                                                    row_ui.add_space(status_gap);
                                                    let detail_label = elide_text(
                                                        row_ui,
                                                        &detail,
                                                        detail_font.clone(),
                                                        detail_width,
                                                    );
                                                    row_ui.add_sized(
                                                        [detail_width, row_height],
                                                        Label::new(
                                                            RichText::new(detail_label)
                                                                .color(Color32::from_gray(200))
                                                                .font(detail_font.clone()),
                                                        ),
                                                    );
                                                }
                                            },
                                        );
                                    });
                            }
                        }
                    });
                });

                ui.add_space(12.0);
                let remaining_height = ui.available_height();
                ui.allocate_ui_with_layout(
                    egui::vec2(ui.available_width(), remaining_height),
                    egui::Layout::bottom_up(egui::Align::Center),
                    |ui| {
                        if components::button(ui, true, "Close", ButtonSize::Medium) {
                            self.system_ui_state.midi_dialog_open = false;
                        }
                        ui.add_space(8.0);
                    },
                );
            });
    }

    fn render_serial_dialog(&mut self, ctx: &Context) {
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

                Frame::none()
                    .inner_margin(Margin::symmetric(16, 0))
                    .show(ui, |frame_ui| {
                        let mut table = TableBuilder::new(frame_ui)
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

    fn render_artnet_dialog(&mut self, ctx: &Context) {
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
                        health_state.artnet_health_state,
                        false,
                        egui::vec2(60.0, 65.0),
                    );

                    ui.add_space(16.0);

                    ui.vertical(|ui| {
                        let base_button = ButtonSize::Medium.dim();
                        let input_height = base_button.0.y;
                        let input_font_size = base_button.1 + 7.0;

                        let create_clicked = ui
                            .horizontal(|ui| {
                                ui.add_sized(
                                    [170.0, input_height],
                                    TextEdit::singleline(
                                        &mut self.system_ui_state.new_artnet_address,
                                    )
                                    .hint_text("IPv4 Address")
                                    .font(FontId::monospace(input_font_size))
                                    .vertical_align(Align::Center),
                                );
                                ui.add_sized(
                                    [90.0, input_height],
                                    TextEdit::singleline(
                                        &mut self.system_ui_state.new_artnet_port,
                                    )
                                    .hint_text("Port")
                                    .font(FontId::monospace(input_font_size))
                                    .vertical_align(Align::Center),
                                );

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
                            let row_height = ButtonSize::Medium.dim().0.y.max(44.0);

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

    fn render_showfile_dialog(&mut self, ctx: &Context) {
        let Some(dialog) = &mut self.system_ui_state.open_file_dialog else {
            return;
        };

        if dialog.show(ctx).selected() {
            let mut config = self.data.config.lock().unwrap();

            if let Some(file) = dialog.path() {
                match self.system_ui_state.file_dialog_open_origin {
                    FileDialogOpenOrigin::Save => {
                        // let dmx = self.data.state.dmx_engine.read().unwrap();
                        // let serialized = postcard::to_allocvec(&dmx.clone()).unwrap();
                        // std::fs::write(file, serialized).unwrap();

                        config.last_open_showfile = Some(file.to_path_buf());
                        //
                        // let config_path = PathBuf::from_str(&self.data.config_path).unwrap();
                        // config::write_config(config_path, config.clone()).unwrap();
                        //
                        // self.data
                        //     .system_message_sender
                        //     .send(SystemMessage::Log(
                        //         format!("Saved showfile to {file:?}"),
                        //         LogLevel::Info,
                        //     ))
                        //     .unwrap();

                        mem::drop(config);

                        self.save_showfile();
                    }
                    FileDialogOpenOrigin::Load => {
                        config.last_open_showfile = Some(file.to_path_buf());

                        // let mut f = File::open(file).expect("no file found");
                        // let metadata = fs::metadata(file).expect("unable to read metadata");
                        // let mut buffer = vec![0; metadata.len() as usize];
                        // f.read(&mut buffer).expect("buffer overflow");
                        //
                        // let decoded: blaulicht_shared::EngineState =
                        //     postcard::from_bytes(&buffer).unwrap();
                        //
                        // {
                        //     let mut plugin_state =
                        //         self.data.state.plugin_state_storage.lock().unwrap();
                        //     *plugin_state = decoded.plugin_state.clone();
                        // }
                        //
                        // let mut dmx = self.data.state.dmx_engine.write().unwrap();
                        // // dmx.overwrite(decoded);
                        // dmx.load_showfile(decoded);
                        // mem::drop(dmx);

                        let mut dmx = self.data.state.dmx_engine.write().unwrap();
                        config::read_showfile(
                            file.to_path_buf(),
                            &mut dmx,
                            &self.data.state.plugin_state_storage,
                            self.data.system_message_sender.clone(),
                        );
                        mem::drop(dmx);

                        config.last_open_showfile = Some(file.to_path_buf());

                        let config_path = PathBuf::from_str(&self.data.config_path).unwrap();
                        config::write_config(config_path, config.clone()).unwrap();

                        self.data
                            .system_message_sender
                            .send(SystemMessage::Log(
                                format!("Loaded showfile from {file:?}"),
                                LogLevel::Info,
                            ))
                            .unwrap();

                        mem::drop(config);

                        self.show_popup(PopupSpec::with_duration(
                            Duration::from_secs(2),
                            "Loaded Showfile".to_string(),
                        ));
                    }
                }
            }
        }
    }

    pub fn system_ui(&mut self, ui: &mut egui::Ui, ctx: &Context) {
        self.render_confirm_shutdown_dialog(ctx);
        self.render_showfile_dialog(ctx);

        for universe in 0..NUM_DMX_UNIVERSES {
            self.render_dmx_dialog(ctx, universe);
        }
        self.render_artnet_dialog(ctx);
        self.render_midi_dialog(ctx);
        self.render_serial_dialog(ctx);

        let button_size = ButtonSize::Medium.with_width(110.0);

        ui.vertical_centered(|ui| {
            ui.horizontal(|ui| {
                let showfile = self
                    .data
                    .config
                    .lock()
                    .unwrap()
                    .last_open_showfile
                    .as_ref()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "N/A".to_string());

                ui.label("showfile:");

                ui.label(
                    egui::RichText::new(showfile)
                        .strong()
                        .font(FontId::monospace(14.0)),
                );
            });

            ui.add_space(3.0);

            ui.horizontal(|ui| {
                if components::button(ui, false, "Load Showfile", button_size) {
                    let filter = Box::new({
                        let ext = Some(OsStr::new("json"));
                        move |path: &Path| -> bool { path.extension() == ext }
                    });

                    let config = self.data.config.lock().unwrap();

                    let mut dialog = FileDialog::open_file(config.last_open_showfile.clone())
                        .show_files_filter(filter);

                    dialog.open();
                    self.system_ui_state.open_file_dialog = Some(dialog);
                    self.system_ui_state.file_dialog_open_origin = FileDialogOpenOrigin::Load;
                }

                if components::button(ui, false, "Save to Showfile", button_size) {
                    let filter = Box::new({
                        let ext = Some(OsStr::new("json"));
                        move |path: &Path| -> bool { path.extension() == ext }
                    });

                    let config = self.data.config.lock().unwrap();

                    let mut dialog = FileDialog::open_file(config.last_open_showfile.clone())
                        .show_files_filter(filter);

                    dialog.open();
                    self.system_ui_state.open_file_dialog = Some(dialog);
                    self.system_ui_state.file_dialog_open_origin = FileDialogOpenOrigin::Save;
                }

                {
                    let mut conf = self.data.config.lock().unwrap();
                    let button_enabled = conf.last_open_showfile.is_some();
                    if components::button(ui, button_enabled, "Close Showfile", button_size)
                        && button_enabled
                    {
                        conf.last_open_showfile = None;
                        let path = PathBuf::from_str(&self.data.config_path).unwrap();
                        config::write_config(path, conf.clone()).unwrap();
                        let mut dmx = self.data.state.dmx_engine.write().unwrap();
                        config::close_showfile(&mut dmx, &self.data.state.plugin_state_storage);
                    }
                }

                let (label, allowed) = match &self
                    .data
                    .config
                    .lock()
                    .unwrap()
                    .last_open_showfile
                    .is_some()
                {
                    true => ("Save Showfile", true),
                    false => ("Save Showfile", false),
                };
                if components::button(ui, allowed, label, button_size) {
                    self.save_showfile();
                }
            });

            ui.add_space(15.0);
            ui.separator();
            ui.add_space(15.0);

            ui.horizontal(|ui| {
                if components::button(
                    ui,
                    !ui.ctx().style().visuals.dark_mode,
                    "LIGHT",
                    button_size,
                ) {
                    ui.ctx().set_theme(ThemePreference::Light);
                }

                if components::button(ui, ui.ctx().style().visuals.dark_mode, "DARK", button_size) {
                    ui.ctx().set_theme(ThemePreference::Dark);
                }
            });

            ui.add_space(15.0);
            ui.separator();
            ui.add_space(15.0);

            ui.horizontal(|ui| {
                if components::button(ui, false, "Quit", button_size) {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }

                if components::button(ui, false, "Shutdown", button_size) {
                    self.system_ui_state.confirm_shutdown_open = true;
                }

                if components::button(ui, self.system_ui_state.debug_open, "Debug", button_size) {
                    self.system_ui_state.debug_open = !self.system_ui_state.debug_open;
                }
            });

            // --- Loop Speed & Tick Speed Graphs ---
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    let graph_width = (ui.available_width() - 16.0) / 2.0;
                    let graph_height = 36.0;
                    // Loop Speed Graph
                    let (loop_resp, loop_painter) = ui.allocate_painter(
                        egui::vec2(graph_width, graph_height),
                        egui::Sense::hover(),
                    );
                    // Draw value and label above the bar, left-aligned
                    let loop_val_str = match self.loop_speed {
                        v if v > 1000 => format!("{:.0} ms", self.loop_speed / 1000),
                        _ => format!("{:.0} us", self.loop_speed),
                    };
                    let label_y = loop_resp.rect.top() + 2.0;
                    let label_x = loop_resp.rect.left() + 4.0;
                    loop_painter.text(
                        egui::pos2(label_x, label_y),
                        egui::Align2::LEFT_TOP,
                        &loop_val_str,
                        egui::FontId::monospace(10.0),
                        Color32::WHITE,
                    );
                    loop_painter.text(
                        egui::pos2(label_x + 60.0, label_y), // 60px offset for value
                        egui::Align2::LEFT_TOP,
                        "Loop",
                        egui::FontId::proportional(10.0),
                        Color32::WHITE,
                    );
                    // Draw loop speed as a bar (simple visualization)
                    let loop_val = self.loop_speed as f32;
                    let loop_max = DMX_TICK_TIME.as_micros() as f32;
                    let loop_bar_width = (loop_val / loop_max).min(1.0) * (graph_width - 8.0);
                    let loop_bar_rect = egui::Rect::from_min_max(
                        loop_resp.rect.left_top() + egui::vec2(4.0, 16.0),
                        loop_resp.rect.left_top()
                            + egui::vec2(4.0 + loop_bar_width, graph_height - 8.0),
                    );
                    let color = match self.loop_speed {
                        v if v as f32 > loop_max => Color32::RED,
                        _ => Color32::from_rgb(0, 200, 255),
                    };
                    loop_painter.rect_filled(loop_bar_rect, 2.0, color);

                    // Tick Speed Graph
                    let (tick_resp, tick_painter) = ui.allocate_painter(
                        egui::vec2(graph_width, graph_height),
                        egui::Sense::hover(),
                    );
                    let tick_val_str = format!("{:.0} μs", self.tick_speed);
                    let tick_label_y = tick_resp.rect.top() + 2.0;
                    let tick_label_x = tick_resp.rect.left() + 4.0;
                    tick_painter.text(
                        egui::pos2(tick_label_x, tick_label_y),
                        egui::Align2::LEFT_TOP,
                        &tick_val_str,
                        egui::FontId::monospace(10.0),
                        Color32::WHITE,
                    );
                    tick_painter.text(
                        egui::pos2(tick_label_x + 60.0, tick_label_y),
                        egui::Align2::LEFT_TOP,
                        "Tick",
                        egui::FontId::proportional(10.0),
                        Color32::WHITE,
                    );
                    let tick_val = self.tick_speed as f32;
                    let tick_max = 1000.0; // 1 MS
                    let tick_bar_width = (tick_val / tick_max).min(1.0) * (graph_width - 8.0);
                    let tick_bar_rect = egui::Rect::from_min_max(
                        tick_resp.rect.left_top() + egui::vec2(4.0, 16.0),
                        tick_resp.rect.left_top()
                            + egui::vec2(4.0 + tick_bar_width, graph_height - 8.0),
                    );
                    tick_painter.rect_filled(tick_bar_rect, 2.0, Color32::from_rgb(255, 200, 0));
                });

                ui.add_space(8.0);

                // --- Heartbeat Indicator ---
                ui.horizontal(|ui| {
                    // Heartbeat: small circle, green if recent, gray if not
                    let heartbeat_recent = self.last_heartbeat_frame + 30 > self.frame_count;
                    let color = if heartbeat_recent {
                        Color32::from_rgb(0, 255, 0)
                    } else {
                        Color32::from_gray(80)
                    };
                    ui.painter().circle_filled(
                        ui.cursor().left_top() + egui::vec2(10.0, 10.0),
                        8.0,
                        color,
                    );
                    ui.label("Heartbeat");
                });

                // --- Reload button ---
                {
                    let signal = *self.data.state.mainloop_state.read().unwrap();

                    ui.label(format!("LOOP {:?}", signal));

                    let bg_color = match signal {
                        AudioThreadControlSignal::CONTINUE => egui::Color32::from_gray(40),
                        AudioThreadControlSignal::ABORT
                        | AudioThreadControlSignal::ABORTED
                        | AudioThreadControlSignal::CRASHED => egui::Color32::DARK_RED,
                        AudioThreadControlSignal::RELOAD => egui::Color32::from_rgb(60, 120, 200),
                    };

                    let rect = ui.allocate_exact_size(egui::vec2(90.0, 60.0), egui::Sense::click());
                    let painter = ui.painter();
                    painter.rect_filled(rect.0, 0.0, bg_color);

                    painter.text(
                        rect.0.center(),
                        egui::Align2::CENTER_CENTER,
                        "RELOAD",
                        egui::FontId::proportional(16.0),
                        egui::Color32::WHITE,
                    );

                    if rect.1.clicked() && signal != AudioThreadControlSignal::RELOAD {
                        self.data
                            .from_frontend_sender
                            .send(FromFrontend::Reload)
                            .unwrap();

                        self.show_popup(PopupSpec::with_duration(
                            Duration::from_secs(2),
                            "Reload in progress...".to_string(),
                        ));
                    }
                }

                {
                    let artnet_receivers_enabled = {
                        let artnet_output = self.data.state.artnet_output.read().unwrap();
                        artnet_output
                            .receivers
                            .iter()
                            .any(|receiver| receiver.enabled)
                    };

                    let health_data = self.data.state.health_data.read().unwrap();

                    let midi_has_error = health_data
                        .midi_health
                        .devices
                        .values()
                        .any(|state| matches!(state, MidiDeviceState::Error(_)));
                    let midi_active = health_data.midi_health.devices.values().any(
                        |state| matches!(state, MidiDeviceState::Open(handles) if *handles > 0),
                    );
                    let midi_online = midi_active && !midi_has_error;

                    let serial_has_error = health_data
                        .serial_health
                        .devices
                        .values()
                        .any(|state| matches!(state, SerialDeviceState::Error(_)));
                    let serial_active = health_data.serial_health.devices.values().any(
                        |state| matches!(state, SerialDeviceState::Open(handles) if *handles > 0),
                    );
                    let serial_online = serial_active && !serial_has_error;

                    let artnet_online = health_data.artnet_health_state && artnet_receivers_enabled;

                    ui.horizontal(|ui| {
                        let dimensions = egui::vec2(60.0, 65.0);

                        // DMX outputs
                        for (universe_number, dmx_port_health) in
                            health_data.dmx_universes_healthy.iter().enumerate()
                        {
                            let label = format!("DMX {universe_number}");
                            if render_dmx_or_artnet_health_box(
                                ui,
                                Label::new(
                                    RichText::new(label)
                                        .color(DMX_OR_ARTNET_HEALTH_LABEL_COLOR)
                                        .size(12.0),
                                ),
                                DMX_ICON,
                                dmx_port_health.is_healthy(),
                                true,
                                dimensions,
                            ) {
                                self.system_ui_state.dmx_dialogs_open[universe_number] = true;
                            }
                        }

                        // Artnet output
                        if render_dmx_or_artnet_health_box(
                            ui,
                            Label::new(
                                RichText::new("ArtNet")
                                    .color(DMX_OR_ARTNET_HEALTH_LABEL_COLOR)
                                    .size(12.0),
                            ),
                            ARTNET_ICON,
                            artnet_online,
                            true,
                            dimensions,
                        ) {
                            self.system_ui_state.artnet_dialog_open = true;
                        }

                        // MIDI subsystem
                        if render_dmx_or_artnet_health_box(
                            ui,
                            Label::new(
                                RichText::new("MIDI")
                                    .color(DMX_OR_ARTNET_HEALTH_LABEL_COLOR)
                                    .size(12.0),
                            ),
                            MIDI_ICON,
                            midi_online,
                            true,
                            dimensions,
                        ) {
                            self.system_ui_state.midi_dialog_open = true;
                        }

                        if render_dmx_or_artnet_health_box(
                            ui,
                            Label::new(
                                RichText::new("Serial")
                                    .color(DMX_OR_ARTNET_HEALTH_LABEL_COLOR)
                                    .size(12.0),
                            ),
                            SERIAL_ICON,
                            serial_online,
                            true,
                            dimensions,
                        ) {
                            self.system_ui_state.serial_dialog_open = true;
                        }
                    })
                }
            });
        });

        ui.separator();

        egui::SidePanel::right("right_panel")
            .resizable(true)
            .default_width(250.0)
            .width_range(200.0..=400.0)
            .show(ctx, |ui| {
                // --- Plugin Overview ---
                ui.label("Plugins");

                ui.add_space(4.0);

                {
                    let plugins = self.data.state.plugins.read().unwrap();
                    let current_visibility =
                        self.data.state.plugin_ui_visibility.read().unwrap().clone();

                    for (i, (id, plugin)) in plugins.iter().enumerate() {
                        let box_size = egui::vec2(ui.available_width(), 42.0);
                        ui.allocate_ui_with_layout(
                            box_size,
                            egui::Layout::top_down(egui::Align::Center),
                            |ui| {
                                let (rect, _response) =
                                    ui.allocate_exact_size(box_size, egui::Sense::empty());
                                let painter = ui.painter();

                                // State color and blinking logic
                                let mut show_border = true;
                                let border_color = match (plugin.has_errored(), plugin.is_enabled())
                                {
                                    // Alive and healthy.
                                    (false, true) => egui::Color32::from_rgb(0, 200, 0),
                                    // Dead, crashed.
                                    (true, true) => {
                                        let blink = ((self.animation_time * 8.0) as i32) % 2 == 0;
                                        show_border = blink;
                                        egui::Color32::from_rgb(200, 0, 0)
                                    }
                                    // Disabled.
                                    (_, false) => {
                                        let blink = ((self.animation_time * 2.0) as i32) % 2 == 0;
                                        show_border = blink;
                                        egui::Color32::from_rgb(200, 200, 0)
                                    }
                                };

                                // Draw the main box
                                painter.rect_filled(rect, 0.0, egui::Color32::from_gray(30));

                                // Draw the left border if needed
                                let border_width = 6.0;
                                if show_border {
                                    let border_rect = egui::Rect::from_min_max(
                                        rect.left_top(),
                                        rect.left_bottom() + egui::vec2(border_width, 0.0),
                                    );
                                    painter.rect_filled(border_rect, 0.0, border_color);
                                }

                                // Plugin name
                                let path_str = plugin.path.to_string().to_string();
                                let path = Path::new(&path_str);
                                let basename = path.file_stem().unwrap().to_string_lossy();
                                // let basename = path.file_name().unwrap().to_string_lossy();
                                let name = format!("P:{basename} ({})", i + 1);

                                let text_padding = 5.0;

                                painter.text(
                                    rect.left_center()
                                        + egui::vec2(border_width + text_padding, 0.0),
                                    egui::Align2::LEFT_CENTER,
                                    name,
                                    egui::FontId::monospace(12.0),
                                    if plugin.has_errored() {
                                        Color32::WHITE
                                    } else {
                                        egui::Color32::from_gray(90)
                                    },
                                );
                            },
                        );
                        ui.horizontal(|ui| {
                            let is_open = *current_visibility.get(id).unwrap_or(&false);
                            let label = if is_open { "Hide UI" } else { "Show UI" };
                            if ui.small_button(label).clicked() {
                                let mut map = self.data.state.plugin_ui_visibility.write().unwrap();
                                let entry = map.entry(*id).or_insert(false);
                                *entry = !*entry;
                            }
                        });
                        ui.add_space(8.0);
                    }
                    ui.separator();

                    mem::drop(plugins)
                }
            });
    }
}

// Returns whether it was clicked.
fn render_dmx_or_artnet_health_box(
    ui: &mut egui::Ui,
    label: Label,
    icon: &str,
    is_healthy: bool,
    clickable: bool,
    dimensions: Vec2,
) -> bool {
    let status_color = if is_healthy {
        Color32::from_rgb(67, 209, 110)
    } else {
        Color32::from_rgb(226, 69, 69)
    };

    let background_color = Color32::from_rgb(52, 58, 70);
    // let BacktraceStyle
    let background_color = ui.visuals().widgets.active.bg_fill;
    // a

    let label_text = label.text().to_string();

    let frame_inner_response = Frame::none()
        .fill(background_color)
        .outer_margin(Margin::same(0))
        .inner_margin(Margin::same(8))
        .show(ui, |ui| {
            ui.allocate_ui_with_layout(
                dimensions,
                egui::Layout::top_down(egui::Align::Center),
                |ui| {
                    ui.label(
                        RichText::new(icon)
                            .size(30.0)
                            .color(Color32::from_gray(230)),
                    );

                    ui.label(
                        RichText::new(&label_text)
                            .size(12.0)
                            .color(Color32::from_gray(240)),
                    );

                    ui.label(
                        RichText::new(if is_healthy { "ONLINE" } else { "OFFLINE" })
                            .size(9.0)
                            .color(status_color),
                    );
                },
            )
        });

    let response = ui.interact(
        frame_inner_response.response.rect,
        ui.make_persistent_id(label_text),
        egui::Sense::click(),
    );

    if response.hovered() && clickable {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        ui.painter().rect_stroke(
            frame_inner_response.response.rect,
            egui::Rounding::same(2),
            egui::Stroke::new(1.0, Color32::from_rgb(255, 255, 255)),
            egui::StrokeKind::Middle,
        );
    }

    response.clicked() && clickable
}

fn elide_text(ui: &egui::Ui, text: &str, font_id: FontId, max_width: f32) -> String {
    if max_width <= 0.0 || text.is_empty() {
        return String::new();
    }

    ui.fonts(|fonts| {
        let full = fonts.layout_no_wrap(text.to_string(), font_id.clone(), Color32::WHITE);
        if full.rect.width() <= max_width {
            return text.to_string();
        }

        let chars: Vec<char> = text.chars().collect();
        if chars.is_empty() {
            return String::new();
        }

        let ellipsis = "…";
        let mut low = 0;
        let mut high = chars.len();
        let mut best = ellipsis.to_string();

        while low < high {
            let mid = (low + high) / 2;
            let candidate: String = chars.iter().take(mid).collect::<String>() + ellipsis;
            let width = fonts
                .layout_no_wrap(candidate.clone(), font_id.clone(), Color32::WHITE)
                .rect
                .width();

            if width <= max_width {
                best = candidate;
                low = mid + 1;
            } else if mid == 0 {
                break;
            } else {
                high = mid;
            }
        }

        best
    })
}
