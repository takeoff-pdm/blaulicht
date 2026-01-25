use egui::{Color32, FontId};

pub fn elide_text(ui: &egui::Ui, text: &str, font_id: FontId, max_width: f32) -> String {
    if max_width <= 0.0 || text.is_empty() {
        return String::new();
    }

    ui.fonts(|fonts| {
        let full = fonts.layout_no_wrap(text.to_string(), font_id.clone(), Color32::WHITE);
        if full.rect.width() <= max_width {
            return text.to_string();
        }

        let chars: Vec<char> = text.chars().collect();
        if chars.is_empty() {
            return String::new();
        }

        let ellipsis = "…";
        let mut low = 0;
        let mut high = chars.len();
        let mut best = ellipsis.to_string();

        while low < high {
            let mid = (low + high) / 2;
            let candidate: String = chars.iter().take(mid).collect::<String>() + ellipsis;
            let width = fonts
                .layout_no_wrap(candidate.clone(), font_id.clone(), Color32::WHITE)
                .rect
                .width();

            if width <= max_width {
                best = candidate;
                low = mid + 1;
            } else if mid == 0 {
                break;
            } else {
                high = mid;
            }
        }

        best
    })
}
