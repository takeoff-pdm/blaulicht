use crate::app::{
    components::{self, ButtonSize, Dialog},
    BlaulichtApp,
};
use blaulicht_engine::{client::EngineClient, plugin::midi::MidiError, state::MidiDeviceState};
use egui::{Color32, Context, FontId, Frame, Label, Margin, RichText};

impl BlaulichtApp {
    pub fn render_midi_dialog(&mut self, ctx: &Context) {
        if !self.system_ui_state.midi_dialog_open {
            return;
        }

        let (mut available_devices, mut device_states) = self.client.with_health(|health| {
            let devices: Vec<(String, MidiDeviceState)> = health
                .midi_health
                .devices
                .iter()
                .map(|(name, state)| (name.clone(), state.clone()))
                .collect();
            (health.midi_health.available_devices.clone(), devices)
        });

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
                                Frame::NONE
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
                                Frame::NONE
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

/// Column starts of the device table, as fractions of its width
/// (`system.html` `.devtbl`).
const DEVICE_COLS: [f32; 4] = [0.0, 0.283, 0.445, 0.607];

/// One row of the desktop MIDI & Serial table.
struct DeviceRow {
    name: String,
    kind: &'static str,
    state: super::desk::State,
    status: &'static str,
    /// The error, as the status tooltip.
    detail: Option<String>,
    last_activity: Option<u64>,
}

impl BlaulichtApp {
    /// System › MIDI & Serial on the desktop (086, `system.html#devices`):
    /// one table of every MIDI input and serial port with its status, last
    /// activity and Reconnect for a device that dropped out. Reconnect
    /// reloads the plugins, which open their devices again. The kiosk keeps
    /// its MIDI and Serial lists.
    pub(super) fn render_devices_desktop(&mut self, ui: &mut egui::Ui) {
        use super::desk::{self, State};
        use crate::app::components::studio;
        use crate::app::pages::setup::duo;
        use crate::app::ui_tokens::{studio as tokens, Density, Weight, TARGET_MIN, TEXT};
        use blaulicht_engine::{hub::HostCommand, state::SerialDeviceState};
        use egui::{pos2, vec2, Align2, Sense};

        let rows: Vec<DeviceRow> = self.client.with_health(|health| {
            let midi = &health.midi_health;
            let mut names: Vec<&String> = midi
                .available_devices
                .iter()
                .chain(midi.devices.keys())
                .collect();
            names.sort();
            names.dedup();
            let mut rows: Vec<DeviceRow> = names
                .into_iter()
                .map(|name| {
                    let (state, status, detail) = match midi.devices.get(name) {
                        Some(MidiDeviceState::Open(_)) => (State::Ok, "Connected", None),
                        Some(MidiDeviceState::Error(err)) => {
                            let detail = match err {
                                MidiError::DeviceNotFound => "Device not found".to_string(),
                                MidiError::Other(e) => e.clone(),
                            };
                            (State::Warn, "Disconnected", Some(detail))
                        }
                        None => (State::Off, "Available", None),
                    };
                    DeviceRow {
                        name: name.clone(),
                        kind: "MIDI input",
                        state,
                        status,
                        detail,
                        last_activity: midi.last_activity.get(name).copied(),
                    }
                })
                .collect();
            let serial = &health.serial_health;
            let mut ports: Vec<_> = serial.devices.iter().collect();
            ports.sort_by_key(|(name, _)| *name);
            rows.extend(ports.into_iter().map(|(name, state)| {
                let (state, status, detail) = match state {
                    SerialDeviceState::Open(_) => (State::Ok, "Connected", None),
                    SerialDeviceState::Error(err) => {
                        (State::Warn, "Disconnected", Some(format!("{err:?}")))
                    }
                };
                DeviceRow {
                    name: name.clone(),
                    kind: "Serial",
                    state,
                    status,
                    detail,
                    last_activity: serial.last_activity.get(name).copied(),
                }
            }));
            rows
        });

        let t = tokens(ui.ctx());
        let full = desk::page_rect(ui);
        let mut pane = desk::pane_at(ui, full, t.pane, "devices-pane");
        studio::pane_header(&mut pane, None, "MIDI & Serial", false, |ui| {
            let n = rows.len().to_string();
            let w = studio::text_width(ui, &n, duo::mono_font());
            let (r, _) = ui.allocate_exact_size(vec2(w, 24.0), Sense::hover());
            ui.painter().text(
                pos2(r.left(), r.center().y),
                Align2::LEFT_CENTER,
                n,
                duo::mono_font(),
                t.strong,
            );
        });
        if rows.is_empty() {
            // `.body` is the darker list colour (studio.css `--list`).
            pane.painter()
                .rect_filled(pane.available_rect_before_wrap(), 0.0, t.list);
            duo::body(&mut pane, None, |ui| {
                duo::note(
                    ui,
                    "No MIDI inputs or serial ports. Plug a device in, or a plugin opens one.",
                );
            });
            return;
        }

        let body = Density::of(ui.ctx()).font(Weight::Regular, 13.0);
        let small = Density::of(ui.ctx()).font(Weight::Regular, TEXT);
        let width = pane.available_width();
        let col_x = |inner: egui::Rect, i: usize| inner.left() + DEVICE_COLS[i] * inner.width();
        // The header row.
        let (head, _) = pane.allocate_exact_size(vec2(width, 22.0), Sense::hover());
        let inner = head.shrink2(vec2(8.0, 0.0));
        for (i, title) in ["Device", "Kind", "Status", "Last activity"]
            .iter()
            .enumerate()
        {
            pane.painter().text(
                pos2(col_x(inner, i), head.center().y),
                Align2::LEFT_CENTER,
                *title,
                small.clone(),
                t.muted,
            );
        }
        // The rows, and the empty area under them, sit on the list colour.
        pane.painter()
            .rect_filled(pane.available_rect_before_wrap(), 0.0, t.list);
        let now = blaulicht_engine::state::unix_millis_now();
        let mut reconnect = false;
        for (index, row) in rows.iter().enumerate() {
            let (rect, _) = pane.allocate_exact_size(vec2(width, TARGET_MIN), Sense::hover());
            let inner = rect.shrink2(vec2(8.0, 0.0));
            let cy = rect.center().y;
            studio::paint_truncated(
                &pane,
                pos2(inner.left(), cy),
                &row.name,
                body.clone(),
                t.text,
                col_x(inner, 1) - inner.left() - 8.0,
            );
            pane.painter().text(
                pos2(col_x(inner, 1), cy),
                Align2::LEFT_CENTER,
                row.kind,
                small.clone(),
                t.muted,
            );
            let status_rect = egui::Rect::from_min_max(
                pos2(col_x(inner, 2), rect.top()),
                pos2(col_x(inner, 3) - 8.0, rect.bottom()),
            );
            let mut cell = pane.new_child(
                egui::UiBuilder::new()
                    .id_salt(("device-status", index))
                    .max_rect(status_rect)
                    .layout(egui::Layout::left_to_right(egui::Align::Center)),
            );
            desk::state_label(&mut cell, row.state, row.status);
            if let Some(detail) = &row.detail {
                pane.interact(
                    status_rect,
                    pane.id().with(("device-tip", index)),
                    Sense::hover(),
                )
                .on_hover_text(detail);
            }
            let activity = match row.last_activity {
                Some(then) => desk::ago(now, then),
                None => "None".to_string(),
            };
            pane.painter().text(
                pos2(col_x(inner, 3), cy),
                Align2::LEFT_CENTER,
                activity,
                small.clone(),
                t.muted,
            );
            if row.state == State::Warn {
                let label = "Reconnect";
                let w = studio::text_width(ui, label, small.clone()) + 16.0;
                let button = egui::Rect::from_min_size(
                    pos2(inner.right() - w, rect.top()),
                    vec2(w, TARGET_MIN),
                );
                let mut cell = pane.new_child(
                    egui::UiBuilder::new()
                        .id_salt(("device-reconnect", index))
                        .max_rect(button)
                        .layout(egui::Layout::left_to_right(egui::Align::Center)),
                );
                if duo::chip(&mut cell, label) {
                    reconnect = true;
                }
            }
        }
        if reconnect {
            self.send_command(HostCommand::ReloadPlugins, |_, _| {});
        }
    }
}
