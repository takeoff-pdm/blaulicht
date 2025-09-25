// use egui::*;

use egui::{vec2, Color32, Rect, Response, Sense, Stroke, StrokeKind, TextStyle, Ui, Widget};

/// A vertical fader widget, like a MIDI/DMX control.
/// - Draws a track with tick marks
/// - Draws a draggable knob
/// - Displays the current value + optional label
pub struct Fader<'a> {
    value: &'a mut f32,
    range: std::ops::RangeInclusive<f32>,
    label: Option<String>,
    show_value: bool,
}

impl<'a> Fader<'a> {
    pub fn new(value: &'a mut f32, range: std::ops::RangeInclusive<f32>) -> Self {
        Self {
            value,
            range,
            label: None,
            show_value: true,
        }
    }

    /// Add a label under the fader
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Toggle whether to show numeric value
    pub fn show_value(mut self, yes: bool) -> Self {
        self.show_value = yes;
        self
    }
}

impl<'a> Widget for Fader<'a> {
    fn ui(self, ui: &mut Ui) -> Response {
        let desired_size = egui::vec2(36.0, 160.0); // width, height
        let (rect, mut response) = ui.allocate_exact_size(desired_size, Sense::click_and_drag());

        let painter = ui.painter();

        // --- TRACK ---
        let track_rect = Rect::from_center_size(
            rect.center(),
            egui::vec2(rect.width() * 0.3, rect.height() * 0.9),
        );

        if response.dragged() {
            if let Some(pointer) = response.interact_pointer_pos() {
                // Clamp pointer x to the track rect to avoid out-of-bounds
                let pointer_x = pointer.x.clamp(track_rect.left(), track_rect.right());

                let t = (pointer_x - track_rect.left()) / track_rect.width();
                let value = self.range.start() + t * (self.range.end() - self.range.start());
                *self.value = value.clamp(*self.range.start(), *self.range.end());
                response.mark_changed();
            }
        }

        painter.rect_filled(track_rect, 2.0, ui.visuals().extreme_bg_color);

        // --- TICK MARKS ---
        let ticks = 10;
        for i in 0..=ticks {
            let frac = i as f32 / ticks as f32;
            let y = egui::lerp(rect.bottom()..=rect.top(), frac);
            let x1 = rect.left();
            let x2 = rect.left() + rect.width() * 0.25;

            painter.line_segment(
                [egui::pos2(x1, y), egui::pos2(x2, y)],
                Stroke::new(1.0, ui.visuals().widgets.noninteractive.fg_stroke.color),
            );
        }

        // --- KNOB ---
        let t = (*self.value - self.range.start()) / (self.range.end() - self.range.start());
        let knob_y = egui::lerp(rect.bottom()..=rect.top(), t);
        let knob_rect = Rect::from_center_size(
            egui::pos2(rect.center().x, knob_y),
            egui::vec2(rect.width() * 0.8, 14.0),
        );
        painter.rect_filled(knob_rect, 3.0, ui.visuals().widgets.active.bg_fill);
        painter.rect_stroke(
            knob_rect,
            3.0,
            Stroke::new(1.0, ui.visuals().widgets.noninteractive.fg_stroke.color),
            StrokeKind::Middle,
        );

        // --- VALUE + LABEL ---
        let mut bottom_y = rect.bottom() + 4.0;
        if self.show_value {
            let value_text = format!("{:.0}", *self.value);
            let galley = ui.painter().layout_no_wrap(
                value_text,
                TextStyle::Body.resolve(ui.style()),
                ui.visuals().text_color(),
            );
            let pos = egui::pos2(rect.center().x - galley.size().x / 2.0, bottom_y);
            painter.galley(pos, galley, Color32::LIGHT_RED);
            bottom_y += 16.0;
        }

        if let Some(label) = self.label {
            let galley = ui.painter().layout_no_wrap(
                label,
                TextStyle::Body.resolve(ui.style()),
                ui.visuals().text_color(),
            );
            let pos = egui::pos2(rect.center().x - galley.size().x / 2.0, bottom_y);
            painter.galley(pos, galley, Color32::LIGHT_RED);
        }

        response
    }
}

///
/// A horizontal fader widget (like a MIDI/DMX console strip).
///

pub struct HFader<'a> {
    value: &'a mut f32,
    range: std::ops::RangeInclusive<f32>,
    label: Option<String>,
    show_value: bool,
}

impl<'a> HFader<'a> {
    pub fn new(value: &'a mut f32, range: std::ops::RangeInclusive<f32>) -> Self {
        Self {
            value,
            range,
            label: None,
            show_value: true,
        }
    }

    /// Add a label under the fader
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Toggle numeric value display
    pub fn show_value(mut self, yes: bool) -> Self {
        self.show_value = yes;
        self
    }
}

impl<'a> Widget for HFader<'a> {
    fn ui(self, ui: &mut Ui) -> Response {
        let desired_size = egui::vec2(120.0, 28.0); // width, height

        let (rect, mut response) = ui.allocate_exact_size(desired_size, Sense::click_and_drag());

        if response.dragged() {
            if let Some(pointer) = response.interact_pointer_pos() {
                let t = ((pointer.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
                *self.value = self.range.start() + t * (self.range.end() - self.range.start());
                response.mark_changed();
            }
        }

        let painter = ui.painter();

        // --- TRACK ---
        let track_dim = egui::vec2(rect.width() * 0.9, rect.height() * 0.3);

        let track_rect = Rect::from_center_size(rect.center(), track_dim);

        // --- TICK MARKS ---
        let ticks = 10;

        let tick_height = 7.0;
        let tick_spacing = 3.0;

        for i in 0..=ticks {
            let frac = i as f32 / ticks as f32;
            let x = egui::lerp(rect.left()..=rect.right(), frac);

            let y1 = rect.center().y - (track_dim.y / 2.0) - tick_spacing;
            let y2 = rect.center().y - (track_dim.y / 2.0) - tick_spacing - tick_height;

            painter.line_segment(
                [egui::pos2(x, y1), egui::pos2(x, y2)],
                Stroke::new(1.0, ui.visuals().widgets.noninteractive.fg_stroke.color),
            );

            let y1 = rect.center().y + (track_dim.y / 2.0) + tick_spacing;
            let y2 = rect.center().y + (track_dim.y / 2.0) + tick_spacing + tick_height;

            painter.line_segment(
                [egui::pos2(x, y1), egui::pos2(x, y2)],
                Stroke::new(1.0, ui.visuals().widgets.noninteractive.fg_stroke.color),
            );
        }

        painter.rect_filled(track_rect, 2.0, ui.visuals().extreme_bg_color);

        // --- KNOB ---
        let t = (*self.value - self.range.start()) / (self.range.end() - self.range.start());
        let knob_x = egui::lerp(rect.left()..=rect.right(), t);
        let knob_rect = Rect::from_center_size(
            egui::pos2(knob_x, rect.center().y),
            egui::vec2(14.0, rect.height() * 0.8),
        );
        painter.rect_filled(knob_rect, 3.0, ui.visuals().widgets.active.bg_fill);
        painter.rect_stroke(
            knob_rect,
            3.0,
            Stroke::new(1.0, ui.visuals().widgets.noninteractive.fg_stroke.color),
            StrokeKind::Middle,
        );

        // --- VALUE + LABEL ---
        let pad = 15.0;
        if self.show_value {
            let value_text = format!("{:.0}", *self.value);
            let galley = ui.painter().layout_no_wrap(
                value_text,
                TextStyle::Body.resolve(ui.style()),
                ui.visuals().text_color(),
            );
            let pos = egui::pos2(rect.right() + pad, rect.top());
            painter.galley(pos, galley, Color32::LIGHT_RED);
        }

        if let Some(label) = self.label {
            let galley = ui.painter().layout_no_wrap(
                label,
                TextStyle::Body.resolve(ui.style()),
                ui.visuals().text_color(),
            );
            let pos = egui::pos2(rect.right() + pad, rect.bottom() - galley.size().y);
            painter.galley(pos, galley, Color32::LIGHT_RED);
        }

        response
    }
}
