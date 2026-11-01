use egui::{pos2, vec2, Color32, Rect, Response, Sense, Stroke, StrokeKind, TextStyle, Ui, Widget};

use crate::app::ui_tokens::{self, Density, Weight};

/// Width of the drawn base plate. The widget may allocate more when its caption is wider.
const PLATE_WIDTH: f32 = 64.0;

/// A hardware-style toggle switch widget.
///
/// Draws a chunky base plate with a sliding lever that snaps between two
/// positions. Clicking toggles the boolean value; dragging across the plate is
/// also supported.
pub struct Switch<'a> {
    state: &'a mut bool,
    label: Option<String>,
}

impl<'a> Switch<'a> {
    pub fn new(state: &'a mut bool) -> Self {
        Self { state, label: None }
    }

    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }
}

impl<'a> Widget for Switch<'a> {
    fn ui(self, ui: &mut Ui) -> Response {
        if Density::of(ui.ctx()).is_compact() {
            return self.compact_ui(ui);
        }
        // Lay the caption out before allocating: the label is painted centered under the
        // plate, so a caption wider than the plate lands outside the allocated rect and is
        // clipped by the surrounding container. Reserve the width the caption actually needs.
        let galley = self.label.map(|label| {
            ui.painter().layout_no_wrap(
                label,
                TextStyle::Body.resolve(ui.style()),
                ui.visuals().text_color(),
            )
        });
        let label_space = if galley.is_some() { 20.0 } else { 0.0 };
        let width = galley
            .as_ref()
            .map_or(PLATE_WIDTH, |g| g.size().x.max(PLATE_WIDTH));
        let desired_size = vec2(width, 44.0 + label_space);

        let (rect, mut response) = ui.allocate_exact_size(desired_size, Sense::click_and_drag());
        let body_height = (rect.height() - label_space).max(32.0);
        // The plate keeps its fixed size and centers inside a wider allocation.
        let switch_rect = Rect::from_min_size(
            pos2(rect.center().x - PLATE_WIDTH / 2.0, rect.top()),
            vec2(PLATE_WIDTH, body_height),
        );

        if response.clicked() {
            *self.state = !*self.state;
            response.mark_changed();
        }

        if response.dragged() {
            if let Some(pointer) = response.interact_pointer_pos() {
                let new_value = pointer.x >= switch_rect.center().x;
                if new_value != *self.state {
                    *self.state = new_value;
                    response.mark_changed();
                }
            }
        }

        let painter = ui.painter();

        // Base plate
        let base_color = if response.hovered() {
            Color32::from_rgb(62, 62, 70)
        } else {
            Color32::from_rgb(46, 46, 54)
        };
        painter.rect_filled(switch_rect, 6.0, base_color);

        let stroke_color = ui.visuals().widgets.noninteractive.bg_stroke.color;
        painter.rect_stroke(
            switch_rect,
            6.0,
            Stroke::new(1.5, stroke_color),
            StrokeKind::Inside,
        );

        // Groove the lever slides in. It has to be tall enough to contain the lever below,
        // otherwise the lever juts out above and below the track and reads as a clipping bug.
        let groove_rect = Rect::from_center_size(
            switch_rect.center(),
            vec2(switch_rect.width() * 0.65, switch_rect.height() * 0.56),
        );
        painter.rect_filled(groove_rect, 4.0, Color32::from_rgb(18, 18, 21));
        painter.rect_stroke(
            groove_rect,
            4.0,
            Stroke::new(1.0, Color32::from_gray(60)),
            StrokeKind::Inside,
        );

        // Lever
        let lever_offset = groove_rect.width() * 0.32;
        let lever_center = pos2(
            groove_rect.center().x
                + if *self.state {
                    lever_offset
                } else {
                    -lever_offset
                },
            groove_rect.center().y,
        );
        // Inset inside the groove on every side so the lever always sits within the track.
        let lever_rect = Rect::from_center_size(
            lever_center,
            vec2(
                groove_rect.width() * 0.35,
                (groove_rect.height() - 5.0).max(4.0),
            ),
        );

        let lever_base = Color32::from_rgb(196, 196, 205);
        painter.rect_filled(lever_rect, 3.0, lever_base);
        painter.rect_stroke(
            lever_rect,
            3.0,
            Stroke::new(1.0, Color32::from_rgb(90, 90, 98)),
            StrokeKind::Inside,
        );

        // Subtle top highlight on the lever for a metallic look.
        let highlight_rect = Rect::from_min_max(
            pos2(lever_rect.left(), lever_rect.top()),
            pos2(lever_rect.right(), lever_rect.center().y),
        );
        painter.rect_filled(
            highlight_rect,
            3.0,
            Color32::from_rgba_premultiplied(255, 255, 255, 24),
        );

        // Optional label centered below the switch.
        if let Some(galley) = galley {
            let text_pos = pos2(
                rect.center().x - galley.size().x / 2.0,
                switch_rect.bottom() + (label_space - galley.size().y) / 2.0,
            );
            painter.galley(text_pos, galley, ui.visuals().text_color());
        }

        response
    }
}

impl Switch<'_> {
    /// The Compact (desktop) look: a small pill toggle, lit in the accent
    /// colour when on, with the caption to its right.
    fn compact_ui(self, ui: &mut Ui) -> Response {
        let color = ui_tokens::palette(ui.ctx());
        const TRACK: egui::Vec2 = vec2(30.0, 16.0);
        let density = Density::of(ui.ctx());
        let height = density.control_height();
        let galley = self.label.map(|label| {
            ui.painter().layout_no_wrap(
                label,
                density.font(Weight::Regular, density.body_size()),
                color.text,
            )
        });
        let label_width = galley.as_ref().map_or(0.0, |g| g.size().x + 6.0);
        let (rect, mut response) =
            ui.allocate_exact_size(vec2(TRACK.x + label_width, height), Sense::click());
        if response.clicked() {
            *self.state = !*self.state;
            response.mark_changed();
        }
        let on = *self.state;
        let hovered = response.hovered();
        let track = Rect::from_min_size(pos2(rect.left(), rect.center().y - TRACK.y / 2.0), TRACK);
        let (fill, border) = match (on, hovered) {
            (true, false) => (color.accent, color.accent),
            (true, true) => (color.accent_hover, color.accent_hover),
            (false, false) => (color.surface_raised, color.line_strong),
            (false, true) => (color.surface_hover, color.line_hover),
        };
        let painter = ui.painter();
        painter.rect(
            track,
            TRACK.y / 2.0,
            fill,
            Stroke::new(ui_tokens::HAIRLINE, border),
            StrokeKind::Inside,
        );
        let knob_x = if on {
            track.right() - TRACK.y / 2.0
        } else {
            track.left() + TRACK.y / 2.0
        };
        let knob_color = if on { color.text_strong } else { color.muted };
        painter.circle_filled(
            pos2(knob_x, track.center().y),
            TRACK.y / 2.0 - 3.0,
            knob_color,
        );
        if let Some(galley) = galley {
            let pos = pos2(track.right() + 6.0, rect.center().y - galley.size().y / 2.0);
            painter.galley(pos, galley, color.text);
        }
        response
            .widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Checkbox, true, on, ""));
        response
    }
}
