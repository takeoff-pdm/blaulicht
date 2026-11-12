//! Small pieces shared by the desktop System pages (086): the status square
//! and label (`.state`), red error text (`.err`), a pane at a fixed rect and
//! the "2 s ago" format of the device table. Desktop only.

use crate::app::ui_tokens::{Density, Studio, Weight, TEXT};
use egui::{pos2, vec2, Align2, Painter, Pos2, Rect, Sense, Ui};

/// What a status square says (`RULES.md`, Colour): healthy green, off grey,
/// amber needs attention, red is broken.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum State {
    Ok,
    Off,
    Warn,
    Bad,
}

impl State {
    fn color(self, t: &Studio) -> egui::Color32 {
        match self {
            State::Ok => t.ok,
            State::Off => t.muted,
            State::Warn => t.warn,
            State::Bad => t.red,
        }
    }
}

/// The 8px status square centred at `center`.
pub(crate) fn paint_square(p: &Painter, center: Pos2, state: State, t: &Studio) {
    p.rect_filled(
        Rect::from_center_size(center, vec2(8.0, 8.0)),
        0.0,
        state.color(t),
    );
}

/// The square and its text (`.state`). The text takes the state's colour
/// only when it needs attention; healthy and off states read as body text.
pub(crate) fn state_label(ui: &mut Ui, state: State, text: &str) {
    let t = crate::app::ui_tokens::studio(ui.ctx());
    let f = Density::of(ui.ctx()).font(Weight::Regular, TEXT);
    let w = 14.0 + crate::app::components::studio::text_width(ui, text, f.clone());
    let (rect, _) = ui.allocate_exact_size(vec2(w, 16.0), Sense::hover());
    paint_square(
        ui.painter(),
        pos2(rect.left() + 4.0, rect.center().y),
        state,
        t,
    );
    let color = match state {
        State::Warn => t.warn,
        State::Bad => t.red_text,
        State::Ok | State::Off => t.text,
    };
    ui.painter().text(
        pos2(rect.left() + 14.0, rect.center().y),
        Align2::LEFT_CENTER,
        text,
        f,
        color,
    );
}

/// Red 12px text that wraps (`.err`).
pub(crate) fn error_text(ui: &mut Ui, text: &str) {
    let t = crate::app::ui_tokens::studio(ui.ctx());
    let f = Density::of(ui.ctx()).font(Weight::Regular, 12.0);
    ui.add(egui::Label::new(egui::RichText::new(text).font(f).color(t.red_text)).wrap());
}

/// A pane filling `rect` with `fill`, laid out top-down with no spacing.
pub(crate) fn pane_at(ui: &mut Ui, rect: Rect, fill: egui::Color32, salt: &str) -> Ui {
    ui.painter().rect_filled(rect, 0.0, fill);
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .id_salt(salt)
            .max_rect(rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    child.set_clip_rect(rect.intersect(ui.clip_rect()));
    child.spacing_mut().item_spacing = vec2(0.0, 0.0);
    child
}

/// The whole page area, edge to edge, painted in the gutter colour, like
/// [`duo`](crate::app::pages::setup::duo::duo).
pub(crate) fn page_rect(ui: &mut Ui) -> Rect {
    let t = crate::app::ui_tokens::studio(ui.ctx());
    let full = ui.max_rect().expand(8.0).intersect(ui.clip_rect());
    ui.allocate_rect(ui.max_rect(), Sense::hover());
    ui.painter().rect_filled(full, 0.0, t.gutter);
    full
}

/// How long ago `then_ms` was, both in Unix milliseconds: "2 s ago",
/// "4 min ago", "3 h ago".
pub(crate) fn ago(now_ms: u64, then_ms: u64) -> String {
    let secs = now_ms.saturating_sub(then_ms) / 1000;
    match secs {
        0..=59 => format!("{secs} s ago"),
        60..=3599 => format!("{} min ago", secs / 60),
        _ => format!("{} h ago", secs / 3600),
    }
}

/// A duration in whole minutes or hours ("12 min"), seconds under a minute.
pub(crate) fn duration(secs: u64) -> String {
    match secs {
        0..=59 => format!("{secs} s"),
        60..=3599 => format!("{} min", secs / 60),
        _ => format!("{} h {} min", secs / 3600, secs / 60 % 60),
    }
}

#[cfg(test)]
mod tests {
    use super::{ago, duration};

    #[test]
    fn ago_reads_like_the_mockup() {
        assert_eq!(ago(10_000, 8_000), "2 s ago");
        assert_eq!(ago(300_000, 60_000), "4 min ago");
        assert_eq!(ago(7_200_000, 0), "2 h ago");
        // A clock that runs behind the engine's never shows negative time.
        assert_eq!(ago(1_000, 5_000), "0 s ago");
    }

    #[test]
    fn durations() {
        assert_eq!(duration(12 * 60 + 5), "12 min");
        assert_eq!(duration(3_660), "1 h 1 min");
        assert_eq!(duration(9), "9 s");
    }
}
