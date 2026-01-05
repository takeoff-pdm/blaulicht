use egui::{vec2, Rect, Response, Sense, Stroke, TextStyle, Ui, Widget};

/// A rotary knob widget inspired by club mixers / CDJ filters.
pub struct Knob<'a> {
    value: &'a mut f32,
    range: std::ops::RangeInclusive<f32>,
    label: Option<String>,
    show_value: bool,
}

impl<'a> Knob<'a> {
    pub fn new(value: &'a mut f32, range: std::ops::RangeInclusive<f32>) -> Self {
        Self {
            value,
            range,
            label: None,
            show_value: true,
        }
    }

    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn show_value(mut self, yes: bool) -> Self {
        self.show_value = yes;
        self
    }
}

impl<'a> Widget for Knob<'a> {
    fn ui(mut self, ui: &mut Ui) -> Response {
        let desired_size = vec2(30.0, 120.0);
        let (rect, mut response) = ui.allocate_exact_size(desired_size, Sense::click_and_drag());

        let start_raw = *self.range.start();
        let end_raw = *self.range.end();
        let reversed = start_raw > end_raw;
        let range_min = start_raw.min(end_raw);
        let range_max = start_raw.max(end_raw);
        let range_span = range_max - range_min;
        let is_flat_range = range_span.abs() < f32::EPSILON;

        let mut normalized_value = if is_flat_range {
            0.0
        } else if reversed {
            (range_max - (*self.value).clamp(range_min, range_max)) / range_span
        } else {
            ((*self.value).clamp(range_min, range_max) - range_min) / range_span
        }
        .clamp(0.0, 1.0);

        if !is_flat_range && response.dragged() {
            let (pointer_delta, modifiers) = ui.input(|i| (i.pointer.delta(), i.modifiers));
            if pointer_delta.y.abs() > f32::EPSILON {
                let drag_speed = if modifiers.shift { 0.0015 } else { 0.005 };
                let previous = normalized_value;
                normalized_value =
                    (normalized_value - pointer_delta.y * drag_speed).clamp(0.0, 1.0);
                if (normalized_value - previous).abs() > f32::EPSILON {
                    let new_value = if reversed {
                        range_max - normalized_value * range_span
                    } else {
                        range_min + normalized_value * range_span
                    };
                    if (new_value - *self.value).abs() > f32::EPSILON {
                        *self.value = new_value;
                        response.mark_changed();
                    }
                }
            }
        }

        if !is_flat_range && response.hovered() && response.ctx.input(|i| i.pointer.any_click()) {
            response.request_focus();
        }

        let visuals = ui.visuals();
        let painter = ui.painter();

        let has_label = self.label.is_some();
        let text_style = TextStyle::Body;
        let line_height = ui.text_style_height(&text_style);
        let spacing = ui.spacing().item_spacing.y;

        let mut reserved_text_height = 0.0;
        if self.show_value {
            reserved_text_height += line_height;
        }
        if has_label {
            if self.show_value {
                reserved_text_height += spacing;
            }
            reserved_text_height += line_height;
        }

        let vertical_padding = spacing;
        let knob_area_height = (rect.height() - vertical_padding - reserved_text_height).max(24.0);
        let knob_diameter = rect.width().min(knob_area_height);
        let knob_center = egui::pos2(
            rect.center().x,
            rect.top() + vertical_padding + knob_diameter / 2.0,
        );
        let knob_rect = Rect::from_center_size(knob_center, vec2(knob_diameter, knob_diameter));
        let knob_radius = knob_diameter / 2.0;

        // Base circle with a subtle ridge to suggest depth
        painter.circle_filled(knob_center, knob_radius, visuals.widgets.inactive.bg_fill);
        painter.circle_stroke(
            knob_center,
            knob_radius,
            Stroke::new(2.0, visuals.widgets.noninteractive.fg_stroke.color),
        );
        // Marker indicating current value
        const START_ANGLE: f32 = std::f32::consts::TAU * 0.75; // 270° (top)
        const SWEEP: f32 = std::f32::consts::TAU * 0.8; // ~288°
        let marker_angle = START_ANGLE - normalized_value * SWEEP;
        let marker_inner = knob_center + egui::Vec2::angled(marker_angle) * (knob_radius * 0.25);
        let marker_outer = knob_center + egui::Vec2::angled(marker_angle) * (knob_radius * 0.9);
        painter.line_segment(
            [marker_inner, marker_outer],
            Stroke::new(3.0, visuals.widgets.active.fg_stroke.color),
        );
        painter.circle_filled(marker_outer, 3.0, visuals.widgets.active.fg_stroke.color);

        // Value + label text beneath the knob
        let mut baseline_y = knob_rect.bottom() + spacing;
        if self.show_value {
            let value_text = if range_span.abs() > 10.0 {
                format!("{:.0}", *self.value)
            } else {
                format!("{:.2}", *self.value)
            };
            let galley = painter.layout_no_wrap(
                value_text,
                text_style.resolve(ui.style()),
                visuals.text_color(),
            );
            let pos = egui::pos2(knob_center.x - galley.size().x / 2.0, baseline_y);
            painter.galley(pos, galley, visuals.text_color());
            baseline_y += line_height + spacing;
        }

        if let Some(label) = self.label.take() {
            let galley = painter.layout_no_wrap(
                label,
                text_style.resolve(ui.style()),
                visuals.weak_text_color(),
            );
            let pos = egui::pos2(knob_center.x - galley.size().x / 2.0, baseline_y);
            painter.galley(pos, galley, visuals.weak_text_color());
        }

        response
    }
}
