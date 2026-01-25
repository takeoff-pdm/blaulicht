use crate::{
    app::BlaulichtApp,
    state::{MidiDeviceState, SerialDeviceState},
};
use blaulicht_assets::icons;
use egui::{Color32, Frame, Label, Margin, RichText, Ui, Vec2};

pub const DMX_ICON: &str = icons::DMX;
pub const ARTNET_ICON: &str = egui_phosphor::regular::NETWORK;
pub const MIDI_ICON: &str = icons::MIDI;
pub const SERIAL_ICON: &str = egui_phosphor::regular::PLUG;
pub const DMX_OR_ARTNET_HEALTH_LABEL_COLOR: Color32 = Color32::from_gray(150);

// Returns whether it was clicked.
pub fn render_dmx_or_artnet_health_box(
    ui: &mut egui::Ui,
    label: Label,
    icon: &str,
    icon_size: f32,
    is_healthy: bool,
    clickable: bool,
    dimensions: Vec2,
) -> bool {
    let status_color = if is_healthy {
        Color32::from_rgb(67, 209, 110)
    } else {
        Color32::from_rgb(226, 69, 69)
    };

    let background_color = ui.visuals().widgets.active.bg_fill;
    let label_text = label.text().to_string();

    let frame_inner_response = Frame::NONE
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
                            .size(icon_size)
                            .color(Color32::from_gray(230)),
                    );

                    ui.add(label);

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

impl BlaulichtApp {
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

        let background_color = ui.visuals().widgets.active.bg_fill;
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

    pub fn render_health_indicators(&mut self, ui: &mut Ui) {
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
        let midi_active = health_data
            .midi_health
            .devices
            .values()
            .any(|state| matches!(state, MidiDeviceState::Open(handles) if *handles > 0));
        let midi_online = midi_active && !midi_has_error;

        let serial_has_error = health_data
            .serial_health
            .devices
            .values()
            .any(|state| matches!(state, SerialDeviceState::Error(_)));
        let serial_active = health_data
            .serial_health
            .devices
            .values()
            .any(|state| matches!(state, SerialDeviceState::Open(handles) if *handles > 0));
        let serial_online = serial_active && !serial_has_error;

        let artnet_online = health_data.artnet_health_state && artnet_receivers_enabled;

        let plugins = self.data.state.plugins.read().unwrap();
        let plugins_broken = plugins.iter().any(|p| p.1.has_errored());

        let dimensions = egui::vec2(60.0, 55.0);
        let text_size = 11.0;
        let icon_size = 26.0;

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
                        .size(text_size),
                ),
                DMX_ICON,
                icon_size,
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
                    .size(text_size),
            ),
            ARTNET_ICON,
            icon_size,
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
                    .size(text_size),
            ),
            MIDI_ICON,
            icon_size,
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
                    .size(text_size),
            ),
            SERIAL_ICON,
            icon_size,
            serial_online,
            true,
            dimensions,
        ) {
            self.system_ui_state.serial_dialog_open = true;
        }

        if render_dmx_or_artnet_health_box(
            ui,
            Label::new(
                RichText::new("Plugins")
                    .color(DMX_OR_ARTNET_HEALTH_LABEL_COLOR)
                    .size(text_size),
            ),
            egui_phosphor::regular::PUZZLE_PIECE,
            icon_size,
            !plugins_broken,
            true,
            dimensions,
        ) {
            self.system_ui_state.plugin_dialog_open = true;
        }
    }
}
