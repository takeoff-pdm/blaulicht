use egui::{Color32, Rect, Ui, Vec2};

use crate::app::components::{button::ButtonSize, text_color_for_bg};

pub fn clickable(
    ui: &mut Ui,
    active: bool,
    primary_color: Color32,
    size: ButtonSize,
    add_contents: impl FnOnce(&mut Ui, Rect, Color32),
) -> bool {
    let radius = 1.0;

    let (rect, response) = ui.allocate_exact_size(size.dim().0, egui::Sense::click());

    let painter = ui.painter();

    let normal_bg_color = match active {
        true => primary_color,
        false => ui.visuals().widgets.active.bg_fill,
    };

    let is_pressed = response.is_pointer_button_down_on();

    let bg_color = if is_pressed {
        normal_bg_color.gamma_multiply(1.2)
    } else if response.hovered() {
        normal_bg_color.gamma_multiply(1.4)
    } else {
        normal_bg_color
    };

    let fg_color = text_color_for_bg(ui, bg_color);

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

    // Draw shadow behind button
    ui.painter()
        .rect_filled(rect.translate(shadow_offset), radius + 1.0, shadow_color);

    // Actual button.

    painter.rect_filled(rect, radius, bg_color);

    add_contents(ui, rect, fg_color);

    response.clicked()
}
