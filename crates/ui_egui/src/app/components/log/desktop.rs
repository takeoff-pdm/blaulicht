//! The desktop Logs page pieces (`design/studio/pages.html#logs`,
//! `pages.css` `.pg-log`). Desktop layout only; the kiosk keeps
//! [`super::LogWindow::draw`]'s touch path.

use super::{log_level_label, LogEntry};
use crate::app::components::studio::text_width;
use crate::app::ui_tokens::{studio, Density, Studio, Weight, PHOSPHOR_FILL_FAMILY, TEXT};
use blaulicht_shared::LogLevel;
use egui::text::{LayoutJob, TextFormat};
use egui::{
    pos2, vec2, Align2, Color32, FontFamily, FontId, Rect, Response, Sense, Stroke, StrokeKind, Ui,
};

/// The filter bar (`.pg-bar`, 30px).
pub(super) const BAR_HEIGHT: f32 = 30.0;
/// The table header (`thead th`).
const HEAD_HEIGHT: f32 = 22.0;
/// The footer (`.lg-foot`).
pub(super) const FOOT_HEIGHT: f32 = 22.0;
/// Cell padding left and right (`td { padding: 2px 8px }`).
pub(super) const CELL_PAD: f32 = 8.0;
const TIME_COL: f32 = 104.0;
const LEVEL_COL: f32 = 74.0;
/// Narrowest source column (`.s { width: 84px }`).
pub(super) const SOURCE_COL: f32 = 84.0;
const MONO: f32 = 12.0;
/// `font: 12px/1.45`.
const LINE: f32 = 17.4;
const BUTTON_HEIGHT: f32 = 24.0;

fn ui_font(ui: &Ui, weight: Weight, size: f32) -> FontId {
    Density::of(ui.ctx()).font(weight, size)
}

fn mono(size: f32) -> FontId {
    FontId::new(size, FontFamily::Monospace)
}

fn icon_font(fill: bool, size: f32) -> FontId {
    if fill {
        FontId::new(size, FontFamily::Name(PHOSPHOR_FILL_FAMILY.into()))
    } else {
        FontId::new(size, FontFamily::Proportional)
    }
}

/// Level icon, whether it is a fill glyph, and the colour of the level
/// cell and message (`.DEBUG`, `.WARN`, `.ERROR`; Info uses the defaults).
fn level_style(t: &Studio, level: &LogLevel) -> (&'static str, bool, Color32, Color32) {
    match level {
        LogLevel::Debug => (egui_phosphor::regular::BUG, false, t.scale, t.scale),
        LogLevel::Info => (egui_phosphor::regular::INFO, false, t.muted, t.text),
        LogLevel::Warn => (egui_phosphor::fill::WARNING, true, t.warn, t.warn),
        LogLevel::Err => (egui_phosphor::fill::X_CIRCLE, true, t.red_text, t.red_text),
    }
}

/// Width of the source tags: all equal, fitting the longest source
/// (`min-width: calc(7ch + 12px)`).
pub(super) fn source_tag_width(ui: &Ui, sources: &[String]) -> f32 {
    let f = mono(11.0);
    let seven = text_width(ui, "0000000", f.clone());
    sources
        .iter()
        .map(|s| text_width(ui, s, f.clone()))
        .fold(seven, f32::max)
        + 12.0
}

/// Width of the level dropdown's anchor: at least 128px, else the label
/// with the funnel and chevron (as `dropdown::button` fits it).
pub(super) fn level_button_width(ui: &Ui, text: &str) -> f32 {
    let fit = 8.0 + 18.0 + text_width(ui, text, ui_font(ui, Weight::Regular, TEXT)) + 8.0 + 16.0;
    fit.max(128.0)
}

/// The search field with its magnifier (`.lg-search`, 170px).
pub(super) fn search(ui: &mut Ui, text: &mut String) -> Response {
    let t = studio(ui.ctx());
    let (rect, _) = ui.allocate_exact_size(vec2(170.0, BUTTON_HEIGHT), Sense::hover());
    let id = ui.make_persistent_id("log_search");
    let focused = ui.memory(|m| m.has_focus(id));
    let p = ui.painter();
    p.rect_filled(rect, 0.0, t.well);
    p.rect_stroke(
        rect,
        0.0,
        Stroke::new(1.0_f32, if focused { t.blue } else { t.line }),
        StrokeKind::Inside,
    );
    p.text(
        pos2(rect.left() + 12.0, rect.center().y),
        Align2::CENTER_CENTER,
        egui_phosphor::regular::MAGNIFYING_GLASS,
        icon_font(false, 12.0),
        t.muted,
    );
    let text_rect = Rect::from_min_max(pos2(rect.left() + 22.0, rect.top()), rect.max);
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(text_rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    child
        .add(
            egui::TextEdit::singleline(text)
                .id(id)
                .frame(egui::Frame::NONE)
                .margin(egui::Margin::symmetric(0, 0))
                .hint_text(egui::RichText::new("Filter messages").color(t.muted))
                .text_color(t.strong)
                .font(ui_font(ui, Weight::Regular, TEXT))
                .desired_width(text_rect.width() - 6.0),
        )
        .on_hover_text("Filter log text")
}

fn icon_button_width(ui: &Ui, label: Option<&str>) -> f32 {
    match label {
        Some(label) => {
            8.0 + 13.0 + 5.0 + text_width(ui, label, ui_font(ui, Weight::Regular, TEXT)) + 8.0
        }
        None => BUTTON_HEIGHT,
    }
}

/// Width of Follow, Copy and Clear with their gaps.
pub(super) fn actions_width(ui: &Ui) -> f32 {
    let gap = ui.spacing().item_spacing.x;
    icon_button_width(ui, Some("Follow")) + 2.0 * icon_button_width(ui, None) + 2.0 * gap
}

/// A `.mini.lg-ic` button: an icon and an optional label; blue-dim when
/// `on` (`.primary`).
pub(super) fn icon_button(
    ui: &mut Ui,
    icon: &str,
    label: Option<&str>,
    on: bool,
    tip: &str,
) -> Response {
    let t = studio(ui.ctx());
    let w = icon_button_width(ui, label);
    let (rect, resp) = ui.allocate_exact_size(vec2(w, BUTTON_HEIGHT), Sense::click());
    let fill = if on {
        t.blue_dim
    } else if resp.hovered() {
        t.ctl_hi
    } else {
        t.ctl
    };
    let p = ui.painter();
    p.rect_filled(rect, 0.0, fill);
    if resp.has_focus() {
        p.rect_stroke(rect, 0.0, Stroke::new(1.0_f32, t.blue), StrokeKind::Inside);
    }
    let color = if on { t.strong } else { t.text };
    let cy = rect.center().y;
    match label {
        Some(label) => {
            p.text(
                pos2(rect.left() + 8.0 + 6.5, cy),
                Align2::CENTER_CENTER,
                icon,
                icon_font(false, 13.0),
                color,
            );
            p.text(
                pos2(rect.left() + 8.0 + 13.0 + 5.0, cy),
                Align2::LEFT_CENTER,
                label,
                ui_font(ui, Weight::Regular, TEXT),
                color,
            );
        }
        None => {
            p.text(
                rect.center(),
                Align2::CENTER_CENTER,
                icon,
                icon_font(false, 13.0),
                color,
            );
        }
    }
    let name = label.unwrap_or(tip);
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, on, name));
    resp.on_hover_text(tip)
}

/// The sticky table header: TIME, LEVEL, SOURCE, MESSAGE.
pub(super) fn header(ui: &mut Ui, source_col: f32) {
    let t = studio(ui.ctx());
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), HEAD_HEIGHT), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 0.0, t.head);
    p.hline(
        rect.x_range(),
        rect.bottom() - 0.5,
        Stroke::new(1.0_f32, t.line),
    );
    let f = ui_font(ui, Weight::SemiBold, 10.0);
    let mut x = rect.left();
    for (name, w) in [
        ("TIME", TIME_COL),
        ("LEVEL", LEVEL_COL),
        ("SOURCE", source_col),
        ("MESSAGE", 0.0),
    ] {
        paint_spaced(
            ui,
            pos2(x + CELL_PAD, rect.center().y),
            name,
            f.clone(),
            t.muted,
            0.6,
        );
        x += w;
    }
}

/// Text with letter spacing (`letter-spacing: .06em`).
fn paint_spaced(ui: &Ui, pos: egui::Pos2, text: &str, f: FontId, color: Color32, spacing: f32) {
    let mut job = LayoutJob::default();
    job.append(
        text,
        0.0,
        TextFormat {
            font_id: f,
            color,
            extra_letter_spacing: spacing,
            ..Default::default()
        },
    );
    let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
    let at = pos2(pos.x, pos.y - galley.size().y / 2.0);
    ui.painter().galley(at, galley, color);
}

/// `text` with every case-insensitive match of `query` marked
/// (`.pg-log mark`).
fn highlighted(
    text: &str,
    query: &str,
    f: FontId,
    color: Color32,
    mark: Color32,
    width: f32,
) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.wrap.max_width = width;
    let plain = TextFormat {
        font_id: f.clone(),
        color,
        line_height: Some(LINE),
        ..Default::default()
    };
    let marked = TextFormat {
        background: mark,
        ..plain.clone()
    };
    let lower = text.to_lowercase();
    let q = query.to_lowercase();
    // Lowercasing can change byte lengths outside ASCII; only mark when the
    // offsets still line up.
    if q.is_empty() || lower.len() != text.len() {
        job.append(text, 0.0, plain);
        return job;
    }
    let mut at = 0;
    while let Some(i) = lower[at..].find(&q) {
        let start = at + i;
        let end = start + q.len();
        if !text.is_char_boundary(start) || !text.is_char_boundary(end) {
            break;
        }
        job.append(&text[at..start], 0.0, plain.clone());
        job.append(&text[start..end], 0.0, marked.clone());
        at = end;
    }
    job.append(&text[at..], 0.0, plain);
    job
}

/// The key a row is selected by.
pub(super) fn row_key(entry: &LogEntry) -> u64 {
    entry.seq
}

/// One log row. `striped` rows get the faint even-row fill; `selected`
/// rows the blue-dim fill and strong text (`tr.sl`). Click to (de)select.
#[allow(clippy::too_many_arguments)]
pub(super) fn row(
    ui: &mut Ui,
    entry: &LogEntry,
    striped: bool,
    selected: bool,
    source_col: f32,
    tag_width: f32,
    query: &str,
) -> Response {
    let t = studio(ui.ctx());
    let dark = ui.ctx().global_style().visuals.dark_mode;
    let (icon, fill_icon, mut level_color, mut message_color) = level_style(t, &entry.level);
    // `tr.sl td { color: strong }` beats `.t`, `.l` and `.m`, but not the
    // level classes (`.WARN .m`, ...).
    let time_color = if selected { t.strong } else { t.muted };
    if selected && matches!(entry.level, LogLevel::Info) {
        level_color = t.strong;
        message_color = t.strong;
    }
    let full_w = ui.available_width();
    let msg_x = TIME_COL + LEVEL_COL + source_col + CELL_PAD;
    let msg_w = (full_w - msg_x - CELL_PAD).max(40.0);
    let mark = Color32::from_rgba_unmultiplied(242, 177, 76, 89);
    let job = highlighted(
        entry.message.trim_end(),
        query,
        mono(MONO),
        message_color,
        mark,
        msg_w,
    );
    let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
    let text_h = galley.size().y.max(LINE);

    // Reserve the background slot so the fills sit below the text.
    let background = ui.painter().add(egui::Shape::Noop);
    let top = ui.cursor().top();
    let left = ui.max_rect().left();
    let msg_rect = Rect::from_min_size(pos2(left + msg_x, top + 2.0), vec2(msg_w, text_h));
    ui.painter().galley(msg_rect.min, galley, message_color);

    // "Full trace" under the message, as before.
    let mut trace_h = 0.0;
    if let Some(additional) = entry.additional.as_deref() {
        let trace_rect = Rect::from_min_size(
            pos2(msg_rect.left(), msg_rect.bottom()),
            vec2(msg_w, f32::INFINITY),
        );
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(trace_rect)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        egui::CollapsingHeader::new(egui::RichText::new("Full trace").size(11.0).monospace())
            .id_salt((entry.timestamp, &entry.source, &entry.message))
            .default_open(false)
            .show(&mut child, |ui| {
                ui.label(
                    egui::RichText::new(additional)
                        .size(11.0)
                        .monospace()
                        .color(message_color),
                );
            });
        trace_h = child.min_rect().height();
    }

    // `td { padding: 2px 8px }` around the 18px level line, plus the 1px
    // row border: 23px for a one-line row.
    let height = text_h.max(18.0) + trace_h + 5.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(full_w, height), Sense::click());
    let p = ui.painter();
    let line_cy = rect.top() + 2.0 + 9.0;

    // Time: seconds in muted, milliseconds fainter (`.t span`).
    let secs = entry.timestamp.format("%H:%M:%S").to_string();
    let millis = entry.timestamp.format("%.3f").to_string();
    let f = mono(MONO);
    let tx = rect.left() + CELL_PAD;
    p.text(
        pos2(tx, line_cy),
        Align2::LEFT_CENTER,
        &secs,
        f.clone(),
        time_color,
    );
    let sw = text_width(ui, &secs, f.clone());
    p.text(
        pos2(tx + sw, line_cy),
        Align2::LEFT_CENTER,
        &millis,
        f,
        time_color.gamma_multiply(0.55),
    );

    // Level: icon and label.
    let lx = rect.left() + TIME_COL + CELL_PAD;
    p.text(
        pos2(lx + 6.0, line_cy),
        Align2::CENTER_CENTER,
        icon,
        icon_font(fill_icon, 12.0),
        level_color,
    );
    paint_spaced(
        ui,
        pos2(lx + 17.0, line_cy),
        log_level_label(&entry.level),
        ui_font(ui, Weight::SemiBold, 10.0),
        level_color,
        0.5,
    );

    // Source: an outlined tag, all tags the same width, text centred.
    let sx = rect.left() + TIME_COL + LEVEL_COL + CELL_PAD;
    let tag = Rect::from_min_size(pos2(sx, line_cy - 8.5), vec2(tag_width, 17.0));
    let p = ui.painter();
    p.rect_stroke(tag, 2.0, Stroke::new(1.0_f32, t.line), StrokeKind::Inside);
    let tf = mono(11.0);
    let fits = text_width(ui, &entry.source, tf.clone()) <= tag_width - 10.0;
    p.text(
        tag.center(),
        Align2::CENTER_CENTER,
        &entry.source,
        tf,
        t.muted,
    );
    if !fits {
        ui.interact(tag, ui.id().with(("log_tag", top as i32)), Sense::hover())
            .on_hover_text(&entry.source);
    }

    // Backgrounds: stripe, hover, error tint; the level's left edge; the
    // row hairline.
    let white = |a: f32| Color32::from_white_alpha((a * 255.0) as u8);
    let mut fills = Vec::new();
    if selected {
        // `tr.sl td` replaces the stripe and the error tint.
        fills.push(t.blue_dim);
    } else if striped {
        fills.push(if dark {
            white(0.015)
        } else {
            Color32::from_black_alpha(4)
        });
    }
    if matches!(entry.level, LogLevel::Err) && !selected {
        fills.push(Color32::from_rgba_unmultiplied(255, 60, 60, 18));
    }
    if !selected && resp.hovered() {
        fills.push(if dark {
            white(0.04)
        } else {
            Color32::from_black_alpha(10)
        });
    }
    let mut shapes: Vec<egui::Shape> = fills
        .into_iter()
        .map(|c| egui::Shape::rect_filled(rect, 0.0, c))
        .collect();
    shapes.push(egui::Shape::hline(
        rect.x_range(),
        rect.bottom() - 0.5,
        Stroke::new(1.0_f32, t.hair),
    ));
    if matches!(entry.level, LogLevel::Warn | LogLevel::Err) {
        shapes.push(egui::Shape::rect_filled(
            Rect::from_min_size(rect.min, vec2(2.0, rect.height())),
            0.0,
            level_color,
        ));
    }
    ui.painter().set(background, egui::Shape::Vec(shapes));
    resp
}

/// "No log lines match the filter." (`tr.none`).
pub(super) fn empty(ui: &mut Ui) {
    let t = studio(ui.ctx());
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 66.0), Sense::hover());
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        "No log lines match the filter.",
        ui_font(ui, Weight::Regular, TEXT),
        t.muted,
    );
}

fn plural(n: usize, word: &str) -> String {
    format!("{n} {word}{}", if n == 1 { "" } else { "s" })
}

/// The footer: shown of total lines, warning and error dots, follow state.
pub(super) fn footer(
    ui: &mut Ui,
    shown: usize,
    total: usize,
    warnings: usize,
    errors: usize,
    following: bool,
) {
    let t = studio(ui.ctx());
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), FOOT_HEIGHT), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 0.0, t.head);
    p.hline(
        rect.x_range(),
        rect.top() + 0.5,
        Stroke::new(1.0_f32, t.line),
    );
    let f = ui_font(ui, Weight::Regular, 11.0);
    let cy = rect.center().y;
    let mut x = rect.left() + 8.0;
    let lines = format!("{shown} of {total} lines");
    p.text(pos2(x, cy), Align2::LEFT_CENTER, &lines, f.clone(), t.muted);
    x += text_width(ui, &lines, f.clone()) + 16.0;
    for (text, color) in [
        (plural(warnings, "warning"), t.warn),
        (plural(errors, "error"), t.red_text),
    ] {
        p.circle_filled(pos2(x + 3.0, cy), 3.0, color);
        x += 12.0;
        p.text(pos2(x, cy), Align2::LEFT_CENTER, &text, f.clone(), t.muted);
        x += text_width(ui, &text, f.clone()) + 16.0;
    }
    let (state, color) = if following {
        ("Following", t.ok)
    } else {
        ("Paused", t.muted)
    };
    let r = p.text(
        pos2(rect.right() - 8.0, cy),
        Align2::RIGHT_CENTER,
        state,
        f,
        color,
    );
    if following {
        p.circle_filled(pos2(r.left() - 7.0, cy), 3.0, color);
    }
}
