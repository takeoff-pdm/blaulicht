mod button;
mod button_new;
mod clickable;
mod color;
mod dialog;
mod fader;
mod fixture_value_binding;
mod fixtures_shared;
mod graph;
mod horizontal_nav;
mod id_selection_dialog;
mod knob;
mod log;
mod navbar;
mod numberpad;
mod pagination;
mod progress;
mod selection_dialog;
mod spectrogram;
mod speed_knob;
mod switch;
mod text;
mod text_input;

pub use button::*;
pub use button_new::*;
pub use clickable::*;
pub use color::*;
pub use dialog::*;
pub use fader::*;
pub use fixture_value_binding::*;
pub use fixtures_shared::*;
pub use graph::*;
pub use horizontal_nav::*;
pub use id_selection_dialog::*;
pub use knob::*;
pub use log::*;
pub use navbar::*;
pub use numberpad::*;
pub use pagination::*;
pub use selection_dialog::*;
pub use spectrogram::*;
pub use speed_knob::*;
pub use switch::*;
pub use text::*;
pub use text_input::*;

/// Display label (with a state glyph) and color for a detected audio section.
/// Shared by the audio page and the scene-graph page so they stay in sync.
pub fn section_label(state: blaulicht_shared::SectionState) -> (&'static str, egui::Color32) {
    use blaulicht_shared::SectionState;
    match state {
        SectionState::Drop => ("\u{25CF} DROP", egui::Color32::from_rgb(255, 120, 80)),
        SectionState::ActiveBeat => ("\u{25D0} BEAT ACTIVE", egui::Color32::LIGHT_GREEN),
        SectionState::Breakdown => ("\u{25CB} BREAKDOWN", egui::Color32::from_gray(140)),
    }
}

/// A bounded divider for horizontal toolbars. `Ui::separator` uses the full
/// available cross-axis and can make a toolbar consume the entire page height.
pub fn toolbar_separator(ui: &mut egui::Ui, height: f32) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(1.0, height.max(1.0)),
        egui::Sense::hover(),
    );
    ui.painter().vline(
        rect.center().x,
        rect.y_range(),
        ui.visuals().widgets.noninteractive.bg_stroke,
    );
}
