use egui::{Color32, Sense, Ui, Vec2};

use crate::app::components::text_color_for_bg;
use crate::app::ui_tokens::{self, Density, Weight};

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
    /// Like [`ButtonSize::Custom`], but the width only applies on the kiosk
    /// (Touch density): a Compact button sizes to its label instead. For
    /// toolbar and dialog buttons whose width only makes kiosk rows line up.
    TouchWidth(f32, f32, f32),
}

impl ButtonSize {
    pub fn with_width(&self, width: f32) -> Self {
        Self::Custom(width, self.dim().0.y, self.dim().1)
    }

    pub fn with_height(&self, height: f32) -> Self {
        self.rebuild(self.dim().0.x, height, self.dim().1)
    }

    /// The same kind of size (custom or touch-only width) with new values.
    fn rebuild(&self, x: f32, y: f32, font: f32) -> Self {
        match self {
            Self::TouchWidth(..) => Self::TouchWidth(x, y, font),
            _ => Self::Custom(x, y, font),
        }
    }

    /// This size `width` wide on the kiosk; on the desktop (Compact density)
    /// sized to the label. See [`ButtonSize::TouchWidth`].
    pub fn with_touch_width(&self, width: f32) -> Self {
        Self::TouchWidth(width, self.dim().0.y, self.dim().1)
    }

    pub fn with_font_size(&self, size: f32) -> Self {
        self.rebuild(self.dim().0.x, self.dim().0.y, size)
    }

    // Returns button and font size.
    pub const fn dim(&self) -> (Vec2, f32) {
        match self {
            ButtonSize::Small => (egui::vec2(90.0, 12.0), 9.0),
            ButtonSize::Medium => (egui::vec2(90.0, 32.0), 11.0),
            ButtonSize::Large => (egui::vec2(90.0, 64.0), 16.0),
            ButtonSize::Custom(x, y, f) | ButtonSize::TouchWidth(x, y, f) => {
                (egui::vec2(*x, *y), *f)
            }
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

/// A button with an explicit [`ButtonVariant`]. On the kiosk (Touch density)
/// `Primary` looks like an active [`button`] and the other variants like an
/// inactive one, so call sites can switch to it without changing the kiosk.
pub fn variant_button(ui: &mut Ui, variant: ButtonVariant, label: &str, size: ButtonSize) -> bool {
    button_as(ui, variant == ButtonVariant::Primary, variant, label, size)
}

/// A button that looks like `button(ui, active, ..)` on the kiosk (Touch
/// density) and like `variant` on the desktop (Compact density): for call
/// sites whose kiosk look must not change, e.g. a confirm "Delete" that is
/// active on the kiosk and a danger button on the desktop.
pub fn button_as(
    ui: &mut Ui,
    active: bool,
    variant: ButtonVariant,
    label: &str,
    size: ButtonSize,
) -> bool {
    action_button_as(ui, true, active, variant, label, size, None)
}

/// [`action_button`] with a desktop [`ButtonVariant`] (see [`button_as`]).
pub fn action_button_as(
    ui: &mut Ui,
    enabled: bool,
    active: bool,
    variant: ButtonVariant,
    label: &str,
    size: ButtonSize,
    disabled_reason: Option<&str>,
) -> bool {
    if Density::of(ui.ctx()).is_compact() {
        let response = studio_button(ui, variant, enabled, label, size, disabled_reason);
        return enabled && response.clicked();
    }
    button_enabled(ui, enabled, active, label, size, disabled_reason)
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
    if Density::of(ui.ctx()).is_compact() {
        let variant = if active {
            ButtonVariant::On
        } else {
            ButtonVariant::Tonal
        };
        let response = studio_button(ui, variant, enabled, label, size, disabled_reason);
        return enabled && response.clicked();
    }

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

//
// Studio buttons (Compact density, the desktop layout).
//

/// How a button reads. On the kiosk (Touch density) only `On` / `Primary`
/// (the active look) and the rest (the inactive look) differ.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonVariant {
    /// The default: a flat raised button with a hairline border.
    Tonal,
    /// A toggle or selection that is on: lit in the accent colour.
    On,
    /// The main action of a dialog or toolbar: filled with the accent.
    Primary,
    /// Destructive: the label and border in the danger colour.
    Danger,
    /// Borderless until hovered, for secondary row actions.
    Quiet,
}

impl ButtonSize {
    /// The height of this size at `density`. Compact maps the kiosk heights
    /// to its control height (tall custom buttons shrink but stay taller).
    pub fn height_at(&self, density: Density) -> f32 {
        if !density.is_compact() {
            return self.dim().0.y;
        }
        match self {
            ButtonSize::Small => 20.0,
            ButtonSize::Medium => density.control_height(),
            ButtonSize::Large => 28.0,
            ButtonSize::Custom(_, y, _) | ButtonSize::TouchWidth(_, y, _) if *y <= 40.0 => {
                density.control_height()
            }
            ButtonSize::Custom(_, y, _) | ButtonSize::TouchWidth(_, y, _) => {
                (y * 0.6).round().max(28.0)
            }
        }
    }

    /// The width a Compact button keeps at least: none for the presets
    /// (they size to their label), the given width for a custom size.
    fn compact_min_width(&self) -> f32 {
        match self {
            ButtonSize::Small | ButtonSize::Medium | ButtonSize::TouchWidth(..) => 0.0,
            ButtonSize::Large => 64.0,
            ButtonSize::Custom(x, _, _) => *x,
        }
    }
}

/// Fill, border and label colours of a studio button.
struct StudioColors {
    fill: Color32,
    border: Color32,
    text: Color32,
}

fn studio_colors(
    color: &ui_tokens::Palette,
    variant: ButtonVariant,
    enabled: bool,
    hovered: bool,
    pressed: bool,
) -> StudioColors {
    if !enabled {
        return StudioColors {
            fill: Color32::TRANSPARENT,
            border: color.line,
            text: color.faint,
        };
    }
    match variant {
        ButtonVariant::Tonal => StudioColors {
            fill: if pressed {
                color.surface_pressed
            } else if hovered {
                color.surface_hover
            } else {
                color.surface_raised
            },
            border: if hovered || pressed {
                color.line_hover
            } else {
                color.line_strong
            },
            text: if hovered || pressed {
                color.text_strong
            } else {
                color.text
            },
        },
        ButtonVariant::On => StudioColors {
            fill: if hovered || pressed {
                color.on_fill_hover
            } else {
                color.on_fill
            },
            border: if hovered || pressed {
                color.accent_hover
            } else {
                color.accent
            },
            text: color.on_text,
        },
        ButtonVariant::Primary => StudioColors {
            fill: if pressed {
                color.primary_fill
            } else if hovered {
                color.primary_fill_hover
            } else {
                color.primary_fill
            },
            border: if hovered {
                color.accent_hover
            } else {
                color.primary_fill
            },
            text: color.primary_text,
        },
        ButtonVariant::Danger => StudioColors {
            fill: if hovered || pressed {
                color.danger_fill_hover
            } else {
                color.surface_raised
            },
            border: if hovered || pressed {
                color.danger
            } else {
                color.surface_raised.lerp_to_gamma(color.danger, 0.55)
            },
            text: color.danger,
        },
        ButtonVariant::Quiet => StudioColors {
            fill: if pressed {
                color.surface_pressed
            } else if hovered {
                color.surface_hover
            } else {
                Color32::TRANSPARENT
            },
            border: Color32::TRANSPARENT,
            text: if hovered || pressed {
                color.text_strong
            } else {
                color.muted
            },
        },
    }
}

/// A studio-style button (Compact density): sized to its label (at least
/// the custom width of `size`), never truncated, flat with a hairline border,
/// hover / pressed / focus states and the colours of `variant`. A disabled
/// button (or one in a disabled `Ui`) shows `disabled_reason` on hover.
pub fn studio_button(
    ui: &mut Ui,
    variant: ButtonVariant,
    enabled: bool,
    label: &str,
    size: ButtonSize,
    disabled_reason: Option<&str>,
) -> egui::Response {
    let density = Density::of(ui.ctx());
    let enabled = enabled && ui.is_enabled();
    let font = density.font(Weight::Medium, density.button_text_size());
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_owned(), font, Color32::PLACEHOLDER);
    let height = size.height_at(density);
    let width = (galley.size().x + 2.0 * density.button_padding_x())
        .max(size.compact_min_width())
        .max(height)
        .ceil();

    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, height), sense);

    if ui.is_rect_visible(rect) {
        let pressed = response.is_pointer_button_down_on();
        let color = ui_tokens::palette(ui.ctx());
        let colors = studio_colors(color, variant, enabled, response.hovered(), pressed);
        let radius = egui::CornerRadius::same(ui_tokens::RADIUS);
        let painter = ui.painter();
        painter.rect(
            rect,
            radius,
            colors.fill,
            egui::Stroke::new(ui_tokens::HAIRLINE, colors.border),
            egui::StrokeKind::Inside,
        );
        if response.has_focus() {
            painter.rect_stroke(
                rect.expand(1.0),
                radius,
                egui::Stroke::new(ui_tokens::HAIRLINE, color.accent_hover),
                egui::StrokeKind::Outside,
            );
        }
        let pos = egui::Align2::CENTER_CENTER
            .anchor_size(rect.center(), galley.size())
            .min;
        painter.galley(pos.round(), galley, colors.text);
    }

    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::Button,
            enabled,
            variant == ButtonVariant::On,
            label,
        )
    });
    if !enabled {
        if let Some(reason) = disabled_reason {
            return response.on_hover_text(reason);
        }
    }
    response
}
