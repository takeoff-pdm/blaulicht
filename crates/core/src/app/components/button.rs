use egui::{Color32, Label, Ui, Vec2, WidgetText};

use crate::app::components::text_color_for_bg;

pub enum ButtonColor {
    Blue,
}

impl From<ButtonColor> for Color32 {
    fn from(value: ButtonColor) -> Self {
        match value {
            ButtonColor::Blue => Color32::from_rgb(60, 120, 200),
        }
    }
}

#[derive(Copy, Clone)]
pub enum ButtonSize {
    Small,
    Medium,
    Large,
    Custom(f32, f32, f32),
}

impl ButtonSize {
    pub fn with_width(&self, width: f32) -> Self {
        Self::Custom(width, self.dim().0.y, self.dim().1)
    }

    pub fn with_height(&self, height: f32) -> Self {
        Self::Custom(self.dim().0.x, height, self.dim().1)
    }

    pub fn with_font_size(&self, size: f32) -> Self {
        Self::Custom(self.dim().0.x, self.dim().0.y, size)
    }

    // Returns button and font size.
    pub const fn dim(&self) -> (Vec2, f32) {
        match self {
            ButtonSize::Small => (egui::vec2(90.0, 12.0), 9.0),
            ButtonSize::Medium => (egui::vec2(90.0, 32.0), 11.0),
            ButtonSize::Large => (egui::vec2(90.0, 64.0), 16.0),
            ButtonSize::Custom(x, y, f) => (egui::vec2(*x, *y), *f),
        }
    }
}

//
// Button.
//

pub fn button(ui: &mut Ui, active: bool, label: &str, size: ButtonSize) -> bool {
    const RADIUS: f32 = 1.0;
    const BORDER_WIDTH: f32 = 6.0;

    let (rect, response) = ui.allocate_exact_size(size.dim().0, egui::Sense::click());

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
        .rect_filled(rect.translate(shadow_offset), RADIUS + 1.0, shadow_color);

    // Actual button.
    painter.rect_filled(rect, RADIUS, bg_color);

    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(size.dim().1),
        text_color,
    );

    response.clicked()
}

// pub fn widget_button(
//     ui: &mut Ui,
//     active: bool,
//     label: impl Into<WidgetText>,
//     size: ButtonSize,
// ) -> bool {
//     const RADIUS: f32 = 1.0;
//     const BORDER_WIDTH: f32 = 6.0;
//
//     let (rect, response) = ui.allocate_exact_size(size.dim().0, egui::Sense::click());
//
//     let painter = ui.painter();
//
//     let normal_bg_color = match active {
//         true => match ui.visuals().dark_mode {
//             true => Color32::from_rgb(90, 150, 230),
//             false => Color32::from_rgb(60, 120, 200),
//         },
//         false => ui.visuals().widgets.active.bg_fill,
//     };
//
//     let text_color = text_color_for_bg(ui, normal_bg_color);
//
//     let is_pressed = response.is_pointer_button_down_on();
//
//     let bg_color = if is_pressed {
//         normal_bg_color.gamma_multiply(1.2)
//     } else if response.hovered() {
//         normal_bg_color.gamma_multiply(1.4)
//     } else {
//         normal_bg_color
//     };
//
//     // Shadow parameters
//     let shadow_offset = if is_pressed {
//         Vec2::new(1.0, 1.0)
//     } else {
//         Vec2::new(3.0, 3.0)
//     };
//     let shadow_color = if is_pressed {
//         Color32::from_rgba_unmultiplied(0, 0, 0, 100)
//     } else {
//         Color32::from_rgba_unmultiplied(0, 0, 0, 50)
//     };
//
//     // Draw shadow behind button
//     ui.painter()
//         .rect_filled(rect.translate(shadow_offset), RADIUS + 1.0, shadow_color);
//
//     // Actual button.
//     painter.rect_filled(rect, RADIUS, bg_color);
//
//     ui.label(label);
//
//     painter.text(
//         rect.center(),
//         egui::Align2::CENTER_CENTER,
//         label,
//         egui::FontId::proportional(size.dim().1),
//         text_color,
//     );
//
//     response.clicked()
// }
