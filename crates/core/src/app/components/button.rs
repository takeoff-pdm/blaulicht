use egui::{Color32, Sense, Ui, Vec2};

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

/// Paint `label` centered within `rect`, truncating with an ellipsis if it
/// would otherwise overflow the button horizontally. This keeps long labels
/// from spilling outside the button bounds and over neighbouring widgets.
pub fn paint_button_label(
    painter: &egui::Painter,
    rect: egui::Rect,
    label: &str,
    font_size: f32,
    text_color: Color32,
) {
    const H_PADDING: f32 = 4.0;

    let mut job = egui::text::LayoutJob::single_section(
        label.to_owned(),
        egui::text::TextFormat {
            font_id: egui::FontId::proportional(font_size),
            color: text_color,
            ..Default::default()
        },
    );
    job.wrap =
        egui::text::TextWrapping::truncate_at_width((rect.width() - 2.0 * H_PADDING).max(0.0));

    let galley = painter.layout_job(job);
    let pos = egui::Align2::CENTER_CENTER
        .anchor_size(rect.center(), galley.size())
        .min;
    painter.galley(pos, galley, text_color);
}

//
// Button.
//

pub fn button(ui: &mut Ui, active: bool, label: &str, size: ButtonSize) -> bool {
    button_enabled(ui, true, active, label, size, None)
}

pub fn action_button(
    ui: &mut Ui,
    enabled: bool,
    label: &str,
    size: ButtonSize,
    disabled_reason: Option<&str>,
) -> bool {
    button_enabled(ui, enabled, false, label, size, disabled_reason)
}

fn button_enabled(
    ui: &mut Ui,
    enabled: bool,
    active: bool,
    label: &str,
    size: ButtonSize,
    disabled_reason: Option<&str>,
) -> bool {
    const RADIUS: f32 = 1.0;
    // const BORDER_WIDTH: f32 = 6.0;

    let (rect, response) = ui.allocate_exact_size(
        size.dim().0,
        if enabled {
            Sense::click().union(Sense::focusable_noninteractive())
        } else {
            Sense::hover()
        },
    );

    let is_focussed = response.has_focus();

    let painter = ui.painter();

    let normal_bg_color = if !enabled {
        ui.visuals().widgets.inactive.bg_fill.gamma_multiply(0.65)
    } else {
        match active {
            true => match ui.visuals().dark_mode {
                true => Color32::from_rgb(90, 150, 230),
                false => Color32::from_rgb(60, 120, 200),
            },
            false => ui.visuals().widgets.active.bg_fill,
        }
    };

    let text_color = if enabled {
        text_color_for_bg(ui, normal_bg_color)
    } else {
        ui.visuals().weak_text_color()
    };

    let is_pressed = response.is_pointer_button_down_on();

    let bg_color = if !enabled {
        normal_bg_color
    } else if is_pressed {
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

    // Draw a `border` if focussed.
    if is_focussed {
        ui.painter().rect_stroke(
            rect,
            egui::CornerRadius::same(1),
            egui::Stroke::new(1.0_f32, Color32::from_rgb(255, 255, 255)),
            egui::StrokeKind::Middle,
        );
    }

    paint_button_label(painter, rect, label, size.dim().1, text_color);

    if !enabled {
        if let Some(reason) = disabled_reason {
            response.on_hover_text(reason);
        }
        false
    } else {
        response.clicked()
    }
}
