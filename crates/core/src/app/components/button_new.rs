use crate::app::components::{text_color_for_bg, ButtonSize};
use egui::{Color32, Sense, Ui, Vec2};
use std::borrow::Cow;

pub struct Button<'t> {
    with_shadow: bool,
    size: ButtonSize,
    label: Cow<'t, str>,
}

//
// Button.
//

impl<'t> Button<'t> {
    pub fn new(label: &'t str, size: ButtonSize) -> Self {
        Self {
            with_shadow: false,
            size,
            label: label.into(),
        }
    }

    pub fn with_shadow(self) -> Self {
        Self {
            with_shadow: true,
            ..self
        }
    }

    pub fn ui(&self, ui: &mut Ui, active: bool) -> bool {
        const RADIUS: f32 = 1.0;
        // const BORDER_WIDTH: f32 = 6.0;

        let (rect, response) = ui.allocate_exact_size(
            self.size.dim().0,
            Sense::click().union(Sense::focusable_noninteractive()),
        );

        let is_focussed = response.has_focus();

        let painter = ui.painter();

        let normal_bg_color = match active {
            true => match ui.visuals().dark_mode {
                true => Color32::from_rgb(90, 150, 230),
                false => Color32::from_rgb(60, 120, 200),
            },
            false => ui.visuals().widgets.active.bg_fill,
        };

        let text_color = text_color_for_bg(ui, normal_bg_color);

        let is_pressed = response.is_pointer_button_down_on();

        let bg_color = if is_pressed {
            normal_bg_color.gamma_multiply(1.2)
        } else if response.hovered() {
            normal_bg_color.gamma_multiply(1.4)
        } else {
            normal_bg_color
        };

        // Draw shadow behind button
        if self.with_shadow {
            // Shadow parameters
            let shadow_offset = if is_pressed {
                Vec2::new(1.0, 1.0)
            } else {
                Vec2::new(3.0, 3.0)
            };
            let shadow_color = if is_pressed {
                Color32::from_rgba_unmultiplied(0, 0, 0, 100)
            } else {
                Color32::from_rgba_unmultiplied(0, 0, 0, 50)
            };

            ui.painter()
                .rect_filled(rect.translate(shadow_offset), RADIUS + 1.0, shadow_color);
        }

        // Actual button.
        painter.rect_filled(rect, RADIUS, bg_color);

        // Draw a `border` if focussed.
        if is_focussed {
            ui.painter().rect_stroke(
                rect,
                egui::CornerRadius::same(1),
                egui::Stroke::new(1.0, Color32::from_rgb(255, 255, 255)),
                egui::StrokeKind::Middle,
            );
        }

        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            &self.label,
            egui::FontId::proportional(self.size.dim().1),
            text_color,
        );

        response.clicked()
    }
}
