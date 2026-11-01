//! The desktop layout's status bar: a slim strip along the bottom of the
//! window with live, read-only state, like the status bars of DAWs and
//! lighting consoles: the show and whether it is saved, tempo and beat, the
//! engine loop time, the preview (LIVE) state and the engine connection.

use blaulicht_engine::client::RemoteStatus;
use blaulicht_engine::persistence::SaveStatus;
use egui::{Align, Align2, Color32, Layout, Rect, Sense, Ui};

use crate::app::ui_tokens::{self, Density, Weight};

/// Height of the status bar.
pub const STATUS_BAR_HEIGHT: f32 = 24.0;

/// Loop times at or above this are shown as a warning (the engine targets
/// a tick every few milliseconds).
const LOOP_WARN_MS: f32 = 20.0;

/// What the status bar shows, gathered once per frame from the client.
pub struct StatusBarInfo<'a> {
    /// The showfile's display name; `None` without a showfile.
    pub show_name: Option<String>,
    pub save_status: &'a SaveStatus,
    pub bpm: f32,
    /// Position in the bar (0..4) of the last beat, `None` before the first.
    pub beat_index: Option<usize>,
    /// Whether the last beat was just now (the beat dot flashes).
    pub beat_flash: bool,
    pub loop_ms: f32,
    pub plugins_ms: f32,
    /// Some session previews its scene (LIVE): overlays are suppressed.
    pub live: bool,
    /// `None` for the desk's local engine.
    pub connection: Option<RemoteStatus>,
}

/// Draws the status bar as a bottom panel of `ui`.
pub fn status_bar(ui: &mut Ui, info: &StatusBarInfo<'_>) {
    let fill = ui_tokens::palette(ui.ctx()).surface;
    egui::Panel::bottom("desktop_status_bar")
        .resizable(false)
        .exact_size(STATUS_BAR_HEIGHT)
        .show_separator_line(true)
        .frame(
            egui::Frame::NONE
                .fill(fill)
                .inner_margin(egui::Margin::symmetric(10, 0)),
        )
        .show_inside(ui, |ui| {
            let layout = Layout::left_to_right(Align::Center);
            ui.with_layout(layout, |ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                show_section(ui, info);
                divider(ui);
                tempo_section(ui, info);
                divider(ui);
                engine_section(ui, info);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    connection_section(ui, info);
                    if info.live {
                        divider(ui);
                        live_pill(ui);
                    }
                });
            });
        });
}

/// Text colours of the theme: (text, muted).
fn text_colors(ui: &Ui) -> (Color32, Color32) {
    let color = ui_tokens::palette(ui.ctx());
    (color.text, color.muted)
}

fn text(ui: &mut Ui, text: &str, weight: Weight, color: Color32) -> egui::Response {
    let density = Density::of(ui.ctx());
    ui.label(
        egui::RichText::new(text)
            .font(density.font(weight, 12.0))
            .color(color),
    )
}

/// A number in the monospace font, so it does not jitter as it changes.
fn value(ui: &mut Ui, text: &str, color: Color32) -> egui::Response {
    ui.label(
        egui::RichText::new(text)
            .font(egui::FontId::monospace(11.5))
            .color(color),
    )
}

fn icon(ui: &mut Ui, icon: &str, color: Color32) {
    let density = Density::of(ui.ctx());
    ui.label(
        egui::RichText::new(icon)
            .font(density.font(Weight::Regular, 13.0))
            .color(color),
    );
}

fn divider(ui: &mut Ui) {
    let color = ui_tokens::palette(ui.ctx());
    let (rect, _) = ui.allocate_exact_size(egui::vec2(9.0, 12.0), Sense::hover());
    let stroke = egui::Stroke::new(ui_tokens::HAIRLINE, color.line_strong);
    ui.painter()
        .vline(rect.center().x.round() + 0.5, rect.y_range(), stroke);
}

/// A small coloured dot, vertically centred.
fn dot(ui: &mut Ui, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(7.0, 12.0), Sense::hover());
    ui.painter().circle_filled(rect.center(), 3.0, color);
}

/// The show's name and whether it is saved.
fn show_section(ui: &mut Ui, info: &StatusBarInfo<'_>) {
    let color = ui_tokens::palette(ui.ctx());
    let (text_color, muted) = text_colors(ui);
    icon(ui, egui_phosphor::regular::FILE, muted);
    match &info.show_name {
        Some(name) => {
            text(ui, name, Weight::Medium, text_color);
        }
        None => {
            text(ui, "No showfile", Weight::Regular, muted);
        }
    }
    let (label, dot_color) = match info.save_status {
        SaveStatus::NoShowfile => return,
        SaveStatus::Clean => ("Saved", color.ok),
        SaveStatus::Dirty => ("Unsaved changes", color.warn),
        SaveStatus::Saving => ("Saving…", color.accent),
        SaveStatus::Failed(_) => ("Save failed", color.danger),
    };
    let response = ui
        .horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            dot(ui, dot_color);
            let label_color = if matches!(info.save_status, SaveStatus::Clean) {
                muted
            } else {
                dot_color
            };
            text(ui, label, Weight::Regular, label_color);
        })
        .response;
    if let SaveStatus::Failed(error) = info.save_status {
        response.on_hover_text(error);
    }
}

/// Tempo and the beat within the bar: four dots, the current one lit.
fn tempo_section(ui: &mut Ui, info: &StatusBarInfo<'_>) {
    let color = ui_tokens::palette(ui.ctx());
    let (text_color, muted) = text_colors(ui);
    icon(ui, egui_phosphor::regular::METRONOME, muted);
    value(ui, &format!("{:>5.1}", info.bpm), text_color);
    text(ui, "BPM", Weight::Regular, muted);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(4.0 * 9.0, 12.0), Sense::hover());
    for beat in 0..4 {
        let center = egui::pos2(rect.left() + 4.5 + beat as f32 * 9.0, rect.center().y);
        let current = info.beat_index == Some(beat);
        let fill = match (current, info.beat_flash, beat == 0) {
            (true, true, true) => color.warn,
            (true, true, false) => color.accent_hover,
            (true, false, _) => color.accent,
            (false, _, _) => color.line_strong,
        };
        let square = Rect::from_center_size(center, egui::vec2(6.0, 6.0));
        ui.painter()
            .rect_filled(square, egui::CornerRadius::same(1), fill);
    }
}

/// The engine loop and plugin tick times.
fn engine_section(ui: &mut Ui, info: &StatusBarInfo<'_>) {
    let color = ui_tokens::palette(ui.ctx());
    let (text_color, muted) = text_colors(ui);
    let loop_color = if info.loop_ms >= LOOP_WARN_MS {
        color.warn
    } else {
        text_color
    };
    icon(ui, egui_phosphor::regular::CPU, muted);
    text(ui, "Loop", Weight::Regular, muted);
    value(ui, &format!("{:.0} ms", info.loop_ms), loop_color);
    text(ui, "Plugins", Weight::Regular, muted);
    value(ui, &format!("{:.1} ms", info.plugins_ms), text_color);
}

/// The LIVE preview pill.
fn live_pill(ui: &mut Ui) {
    let color = ui_tokens::palette(ui.ctx());
    let density = Density::of(ui.ctx());
    let galley = ui.painter().layout_no_wrap(
        "LIVE".to_owned(),
        density.font(Weight::SemiBold, 10.5),
        color.text_strong,
    );
    let size = galley.size() + egui::vec2(12.0, 4.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    ui.painter().rect_filled(
        rect,
        egui::CornerRadius::same(ui_tokens::RADIUS),
        color.danger,
    );
    ui.painter().galley(
        Align2::CENTER_CENTER
            .anchor_size(rect.center(), galley.size())
            .min,
        galley,
        color.text_strong,
    );
    response.on_hover_text("A scene is previewed live: overlays are suppressed.");
}

/// The engine connection: local, or the remote connection's state.
fn connection_section(ui: &mut Ui, info: &StatusBarInfo<'_>) {
    let color = ui_tokens::palette(ui.ctx());
    let (text_color, _) = text_colors(ui);
    let (label, dot_color, hover) = match &info.connection {
        None => ("Local engine", color.ok, None),
        Some(RemoteStatus::Connected) => ("Connected", color.ok, None),
        Some(RemoteStatus::Connecting) => ("Connecting…", color.warn, None),
        Some(RemoteStatus::Disconnected(reason)) => {
            ("Disconnected", color.danger, Some(reason.as_str()))
        }
    };
    let response = ui
        .horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            dot(ui, dot_color);
            text(ui, label, Weight::Regular, text_color);
        })
        .response;
    if let Some(reason) = hover {
        response.on_hover_text(reason);
    }
}
