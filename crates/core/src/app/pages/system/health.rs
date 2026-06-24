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
            egui::CornerRadius::same(2),
            egui::Stroke::new(1.0, Color32::from_rgb(255, 255, 255)),
            egui::StrokeKind::Middle,
        );
    }

    response.clicked() && clickable
}

impl BlaulichtApp {
    // Returns whether it was clicked.
    fn render_compact_health_box(
        ui: &mut egui::Ui,
        label: &str,
        icon: &str,
        icon_size: f32,
        is_healthy: bool,
        clickable: bool,
        dimensions: Vec2,
        left_inset: f32,
    ) -> bool {
        let status_color = if is_healthy {
            Color32::from_rgb(67, 209, 110)
        } else {
            Color32::from_rgb(226, 69, 69)
        };

        let background_color = ui.visuals().widgets.active.bg_fill;
        let (row_rect, response) = ui.allocate_exact_size(dimensions, egui::Sense::click());
        let rect =
            egui::Rect::from_min_max(row_rect.min + egui::vec2(left_inset, 0.0), row_rect.max);
        let painter = ui.painter_at(rect);
        let text_left = rect.left() + 42.0;
        let center_y = rect.center().y;

        painter.rect_filled(rect, egui::CornerRadius::ZERO, background_color);
        painter.text(
            egui::pos2(rect.left() + 10.0, center_y),
            egui::Align2::LEFT_CENTER,
            icon,
            egui::FontId::proportional(icon_size),
            Color32::from_gray(230),
        );
        painter.text(
            egui::pos2(text_left, center_y - 6.5),
            egui::Align2::LEFT_CENTER,
            label,
            egui::FontId::proportional(11.0),
            DMX_OR_ARTNET_HEALTH_LABEL_COLOR,
        );
        painter.text(
            egui::pos2(text_left, center_y + 6.5),
            egui::Align2::LEFT_CENTER,
            if is_healthy { "ONLINE" } else { "OFFLINE" },
            egui::FontId::proportional(8.0),
            status_color,
        );

        if response.hovered() && clickable {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            ui.painter().rect_stroke(
                rect,
                egui::CornerRadius::same(2),
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

        ui.spacing_mut().item_spacing = egui::vec2(0.0, 3.0);

        let dimensions = egui::vec2(ui.available_width().max(1.0), 46.5);
        let icon_size = 22.0;
        let left_inset = 6.0;

        // DMX outputs
        for (universe_number, dmx_port_health) in
            health_data.dmx_universes_healthy.iter().enumerate()
        {
            let label = format!("DMX {universe_number}");
            if Self::render_compact_health_box(
                ui,
                &label,
                DMX_ICON,
                icon_size,
                dmx_port_health.is_healthy(),
                true,
                dimensions,
                left_inset,
            ) {
                self.system_ui_state.dmx_dialogs_open[universe_number] = true;
            }
        }

        // Artnet output
        if Self::render_compact_health_box(
            ui,
            "ArtNet",
            ARTNET_ICON,
            icon_size,
            artnet_online,
            true,
            dimensions,
            left_inset,
        ) {
            self.system_ui_state.artnet_dialog_open = true;
        }

        // MIDI subsystem
        if Self::render_compact_health_box(
            ui,
            "MIDI",
            MIDI_ICON,
            icon_size,
            midi_online,
            true,
            dimensions,
            left_inset,
        ) {
            self.system_ui_state.midi_dialog_open = true;
        }

        if Self::render_compact_health_box(
            ui,
            "Serial",
            SERIAL_ICON,
            icon_size,
            serial_online,
            true,
            dimensions,
            left_inset,
        ) {
            self.system_ui_state.serial_dialog_open = true;
        }

        if Self::render_compact_health_box(
            ui,
            "Plugins",
            egui_phosphor::regular::PUZZLE_PIECE,
            icon_size,
            !plugins_broken,
            true,
            dimensions,
            left_inset,
        ) {
            self.system_ui_state.plugin_dialog_open = true;
        }

        let screen_count = self.external_screens.len();
        let screens_label = format!("Screens ({screen_count})");
        if Self::render_compact_health_box(
            ui,
            &screens_label,
            egui_phosphor::regular::MONITOR,
            icon_size,
            true,
            screen_count > 0,
            dimensions,
            left_inset,
        ) {
            self.system_ui_state.screens_dialog_open = true;
        }
    }
}
