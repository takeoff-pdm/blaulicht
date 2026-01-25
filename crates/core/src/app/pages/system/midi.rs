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
    pub fn render_midi_dialog(&mut self, ctx: &Context) {
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
                                        let text = components::elide_text(
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
                                                let text = components::elide_text(
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

                                                let status_label = components::elide_text(
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
                                                    let detail_label = components::elide_text(
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
}
