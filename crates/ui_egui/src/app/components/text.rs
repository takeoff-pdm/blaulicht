use egui::{Color32, FontId};

pub fn elide_text(ui: &egui::Ui, text: &str, font_id: FontId, max_width: f32) -> String {
    if max_width <= 0.0 || text.is_empty() {
        return String::new();
    }

    let full = ui
        .painter()
        .layout_no_wrap(text.to_string(), font_id.clone(), Color32::WHITE);
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
        let width = ui
            .painter()
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
}

/// Label for an entry in one of the id-prefixed pick lists (scenes, views,
/// animations). Entries may carry an empty name, which rendered as a bare
/// `"4 | "` with a dangling separator; fall back to a placeholder instead.
pub fn indexed_label(id: impl std::fmt::Display, name: &str) -> String {
    let name = name.trim();
    if name.is_empty() {
        format!("{id} | (unnamed)")
    } else {
        format!("{id} | {name}")
    }
}

/// A column's title ("Groups:"). On the kiosk (Touch density) a plain label
/// as before; on the desktop (Compact density) a small uppercase header in
/// `muted`, without the colon.
pub fn column_header(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let density = crate::app::ui_tokens::Density::of(ui.ctx());
    if density.is_compact() {
        ui.label(density.overline(text.trim_end_matches(':')))
    } else {
        ui.label(text)
    }
}
