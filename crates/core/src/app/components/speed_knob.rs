use blaulicht_shared::AnimationSpeedModifier;
use egui::{vec2, PointerButton, Rect, Response, Sense, Stroke, TextStyle, Ui, Widget};

/// Rotary knob specialized for `AnimationSpeedModifier`.
pub struct SpeedKnob<'a> {
    value: &'a mut AnimationSpeedModifier,
    label: Option<String>,
    show_value: bool,
}

impl<'a> SpeedKnob<'a> {
    pub fn new(value: &'a mut AnimationSpeedModifier) -> Self {
        Self {
            value,
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

impl<'a> Widget for SpeedKnob<'a> {
    fn ui(mut self, ui: &mut Ui) -> Response {
        let desired_size = vec2(80.0, 120.0);
        let (rect, mut response) = ui.allocate_exact_size(desired_size, Sense::click_and_drag());
        let id = response.id;

        let options = AnimationSpeedModifier::ALL;
        let option_count = options.len().max(1);
        let max_index = option_count - 1;

        let current_index = options
            .iter()
            .position(|item| item == self.value)
            .unwrap_or(0);

        let mut base_norm = if max_index == 0 {
            0.0
        } else {
            current_index as f32 / max_index as f32
        };
        let pointer_down_primary =
            ui.input(|i| i.pointer.button_down(PointerButton::Primary));
        let mut temp_norm = ui.ctx().data(|d| d.get_temp::<f32>(id));
        let mut display_norm = temp_norm.unwrap_or(base_norm);

        if response.dragged() && max_index > 0 {
            let (pointer_delta, modifiers) = ui.input(|i| (i.pointer.delta(), i.modifiers));
            if pointer_delta.y.abs() > f32::EPSILON {
                let drag_speed = if modifiers.shift { 0.0015 } else { 0.005 };
                if temp_norm.is_none() {
                    display_norm = base_norm;
                }
                display_norm =
                    (display_norm - pointer_delta.y * drag_speed).clamp(0.0, 1.0);
                temp_norm = Some(display_norm);
                ui.ctx().data_mut(|d| d.insert_temp(id, display_norm));
            }
        }

        if response.hovered() && response.ctx.input(|i| i.pointer.any_click()) {
            response.request_focus();
        }

        if !pointer_down_primary {
            if let Some(current_norm) = temp_norm {
                if max_index > 0 {
                    let scaled =
                        (current_norm * max_index as f32).clamp(0.0, max_index as f32);
                    let nearest_index = scaled.round() as usize;
                    let snapped_norm = nearest_index as f32 / max_index as f32;
                    display_norm = snapped_norm;
                    base_norm = snapped_norm;

                    let new_value = options[nearest_index];
                    if new_value != *self.value {
                        *self.value = new_value;
                        response.mark_changed();
                    }
                } else {
                    display_norm = 0.0;
                    base_norm = 0.0;
                }
            } else {
                display_norm = base_norm;
            }
            ui.ctx().data_mut(|d| d.remove_temp::<f32>(id));
            temp_norm = None;
        } else if let Some(current_norm) = temp_norm {
            display_norm = current_norm;
        } else {
            display_norm = base_norm;
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

        // Base circle
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

        if max_index > 0 {
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
            let dot_steps = max_index;
            for i in 1..dot_steps {
                let frac = i as f32 / dot_steps as f32;
                let angle = start_angle + frac * sweep;
                let (inner_scale, outer_scale, width) = if i * 2 == dot_steps {
                    (1.06, 1.27, 1.6)
                } else {
                    (1.1, 1.22, 1.1)
                };
                let inner = knob_center + egui::Vec2::angled(angle) * (knob_radius * inner_scale);
                let outer = knob_center + egui::Vec2::angled(angle) * (knob_radius * outer_scale);
                painter.line_segment([inner, outer], Stroke::new(width, tick_color));
            }
        }

        // Marker indicating current value
        let marker_angle = start_angle + display_norm * sweep;
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
            let value_text = format!("{}", self.value.as_str());
            let galley = painter.layout_no_wrap(
                value_text.to_string(),
                text_style.resolve(ui.style()),
                visuals.text_color(),
            );
            let pos = egui::pos2(knob_center.x - galley.size().x / 2.0, baseline_y + 5.0);
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
