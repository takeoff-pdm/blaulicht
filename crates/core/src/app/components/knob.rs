use egui::{vec2, Rect, Response, Sense, Stroke, TextStyle, Ui, Widget};

/// A rotary knob widget inspired by club mixers / CDJ filters.
pub struct Knob<'a> {
    value: &'a mut f32,
    range: std::ops::RangeInclusive<f32>,
    label: Option<String>,
    show_value: bool,
    step: Option<f32>,
}

impl<'a> Knob<'a> {
    pub fn new(value: &'a mut f32, range: std::ops::RangeInclusive<f32>) -> Self {
        Self {
            value,
            range,
            label: None,
            show_value: true,
            step: None,
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

    pub fn with_step(mut self, step: f32) -> Self {
        self.step = Some(step);
        self
    }
}

impl<'a> Widget for Knob<'a> {
    fn ui(mut self, ui: &mut Ui) -> Response {
        let height = if self.label.is_some() { 80.0 } else { 40.0 };
        let desired_size = vec2(30.0, height);
        let (rect, mut response) = ui.allocate_exact_size(desired_size, Sense::click_and_drag());

        let start_raw = *self.range.start();
        let end_raw = *self.range.end();
        let reversed = start_raw > end_raw;
        let range_min = start_raw.min(end_raw);
        let range_max = start_raw.max(end_raw);
        let range_span = range_max - range_min;
        let is_flat_range = range_span.abs() < f32::EPSILON;
        let step = self.step.filter(|s| *s > 0.0 && s.is_finite());

        let mut value = (*self.value).clamp(range_min, range_max);

        if response.hovered() {
            let scroll = ui.ctx().input(|i| i.smooth_scroll_delta.y);

            if scroll != 0.0 {
                let absvalue = if scroll.abs() < 10.0 {
                    step.unwrap_or(1f32)
                } else {
                    range_span / 100.0 * 5.0
                };

                let to_add = match scroll > 0.0 {
                    true => absvalue,
                    false => -absvalue,
                };
                *self.value = (*self.value + to_add).clamp(range_min, range_max);
                response.mark_changed();
                // handle scroll
            }
        }

        if !is_flat_range && response.dragged() {
            let mut normalized_value = if reversed {
                (range_max - value) / range_span
            } else {
                (value - range_min) / range_span
            }
            .clamp(0.0, 1.0);

            let (pointer_delta, modifiers) = ui.input(|i| (i.pointer.delta(), i.modifiers));
            if pointer_delta.y.abs() > f32::EPSILON {
                let drag_speed = if modifiers.shift { 0.0015 } else { 0.005 };
                let previous = normalized_value;
                normalized_value =
                    (normalized_value - pointer_delta.y * drag_speed).clamp(0.0, 1.0);
                if (normalized_value - previous).abs() > f32::EPSILON {
                    let mut new_value = if reversed {
                        range_max - normalized_value * range_span
                    } else {
                        range_min + normalized_value * range_span
                    };

                    if let Some(step) = step {
                        let snapped = range_min + ((new_value - range_min) / step).round() * step;
                        new_value = snapped.clamp(range_min, range_max);
                        normalized_value = if reversed {
                            (range_max - new_value) / range_span
                        } else {
                            (new_value - range_min) / range_span
                        };
                    }

                    if (new_value - *self.value).abs() > f32::EPSILON {
                        *self.value = new_value;
                        value = new_value;
                        response.mark_changed();
                    }
                }
            }
        }

        value = value.clamp(range_min, range_max);
        let normalized_value = if is_flat_range {
            0.0
        } else if reversed {
            (range_max - value) / range_span
        } else {
            (value - range_min) / range_span
        };

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
        const BOTTOM_GAP_FRACTION: f32 = 0.1;
        let half_gap_angle = std::f32::consts::TAU * BOTTOM_GAP_FRACTION * 0.5;
        let sweep = std::f32::consts::TAU * (1.0 - BOTTOM_GAP_FRACTION);
        let start_angle = std::f32::consts::FRAC_PI_2 + half_gap_angle; // just left of bottom

        let tick_color = visuals
            .widgets
            .noninteractive
            .fg_stroke
            .color
            .gamma_multiply(0.6);
        let end_angle = start_angle + sweep;

        // Line markers for the start and end of travel
        let start_inner = knob_center + egui::Vec2::angled(start_angle) * (knob_radius * 1.08);
        let start_outer = knob_center + egui::Vec2::angled(start_angle) * (knob_radius * 1.24);
        painter.line_segment(
            [start_inner, start_outer],
            Stroke::new(3.0, visuals.widgets.active.fg_stroke.color),
        );

        let end_inner = knob_center + egui::Vec2::angled(end_angle) * (knob_radius * 1.08);
        let end_outer = knob_center + egui::Vec2::angled(end_angle) * (knob_radius * 1.24);
        painter.line_segment(
            [end_inner, end_outer],
            Stroke::new(3.0, visuals.widgets.active.fg_stroke.color),
        );

        // Tick markers along the travel arc
        let dot_steps = 8;
        for i in 1..dot_steps {
            let frac = i as f32 / dot_steps as f32;
            let angle = start_angle + frac * sweep;
            let (inner_scale, outer_scale, width) = if i == dot_steps / 2 {
                (1.06, 1.27, 1.6)
            } else {
                (1.1, 1.22, 1.1)
            };
            let inner = knob_center + egui::Vec2::angled(angle) * (knob_radius * inner_scale);
            let outer = knob_center + egui::Vec2::angled(angle) * (knob_radius * outer_scale);
            painter.line_segment([inner, outer], Stroke::new(width, tick_color));
        }

        // Marker indicating current value
        let marker_angle = start_angle + normalized_value * sweep;
        let marker_inner = knob_center + egui::Vec2::angled(marker_angle) * (knob_radius * 0.25);
        let marker_outer = knob_center + egui::Vec2::angled(marker_angle) * (knob_radius * 0.9);
        painter.line_segment(
            [marker_inner, marker_outer],
            Stroke::new(3.0, visuals.widgets.active.fg_stroke.color),
        );
        painter.circle_filled(marker_outer, 3.0, visuals.widgets.active.fg_stroke.color);

        // Value + label text beneath the knob
        let mut baseline_y = knob_rect.bottom() + spacing + 5.0;
        if self.show_value {
            let value_text = if range_span.abs() > 10.0 {
                format!("{:.0}", value)
            } else {
                format!("{:.2}", value)
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
