use egui::{Color32, Ui};

pub fn text_color_for_bg(ui: &Ui, bg: Color32) -> Color32 {
    // Default text color from visuals
    let default_text = ui.visuals().widgets.inactive.fg_stroke.color;

    // Compute contrast ratio
    if is_contrasting(default_text, bg) {
        default_text
    } else {
        contrasting_bw(bg)
    }
}

/// Check if two colors have "enough" contrast
fn is_contrasting(fg: Color32, bg: Color32) -> bool {
    let contrast = contrast_ratio(fg, bg);
    contrast > 3.0 // WCAG "minimum readable" threshold
}

/// Get contrast ratio between two colors
fn contrast_ratio(c1: Color32, c2: Color32) -> f32 {
    let l1 = luminance(c1);
    let l2 = luminance(c2);
    let (bright, dark) = if l1 > l2 { (l1, l2) } else { (l2, l1) };
    (bright + 0.05) / (dark + 0.05)
}

/// Relative luminance (perceived brightness)
fn luminance(c: Color32) -> f32 {
    fn chan(v: u8) -> f32 {
        let v = v as f32 / 255.0;
        if v <= 0.03928 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    }
    0.2126 * chan(c.r()) + 0.7152 * chan(c.g()) + 0.0722 * chan(c.b())
}

/// Simple fallback: black or white text depending on bg brightness
fn contrasting_bw(bg: Color32) -> Color32 {
    let brightness = 0.299 * bg.r() as f32 + 0.587 * bg.g() as f32 + 0.114 * bg.b() as f32;
    if brightness > 128.0 {
        Color32::BLACK
    } else {
        Color32::WHITE
    }
}
