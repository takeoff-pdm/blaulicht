//! Typing a value in place and moving it with the keyboard (ticket 072),
//! for the desktop studio fader and knobs (`design/studio/chrome.js`
//! `editValue` and `keySteps`).
//!
//! Desktop only: the kiosk widgets (`fader.rs`, `knob.rs`, `speed_knob.rs`)
//! don't use any of this.

use super::{notify_error, EngineLink};
use crate::app::ui_tokens::Studio;
use egui::text::{CCursor, CCursorRange};
use egui::{
    vec2, Context, CornerRadius, Event, EventFilter, FontFamily, FontId, Id, Key, Rect, Response,
    Stroke, StrokeKind, Ui, UiBuilder,
};

fn link_id() -> Id {
    Id::new("studio_engine_link")
}

/// Records the engine link for this frame, so controls can refuse typed
/// values while offline. The desktop status bar calls it every frame.
pub fn set_engine_link(ctx: &Context, link: EngineLink) {
    ctx.data_mut(|d| d.insert_temp(link_id(), link));
}

/// The engine link recorded by [`set_engine_link`]; online if nobody did.
pub fn engine_link_of(ctx: &Context) -> EngineLink {
    ctx.data(|d| d.get_temp(link_id()))
        .unwrap_or(EngineLink::Online)
}

/// How many steps a key press moves a control: one per press; a held key
/// speeds up after 300 ms and reaches `range / 25` steps per repeat about
/// 2 s in (`chrome.js` `keySteps`). Short ranges (speed notches) never
/// accelerate because their cap is 1.
pub fn key_steps(repeat: bool, held_for: f64, range_steps: f32) -> f32 {
    if !repeat {
        return 1.0;
    }
    let cap = (range_steps / 25.0).round().max(1.0);
    let ramp = ((held_for - 0.3).max(0.0) / 1.7) as f32;
    (1.0 + ramp * (cap - 1.0)).min(cap).round()
}

/// Arrow keys on the focused control `response`: Up and Right raise it,
/// Down and Left lower it. Returns the signed number of steps to move this
/// frame (0 when unfocused). Also keeps the arrows from moving focus away
/// and focuses the control on a click or drag.
pub fn arrow_steps(ui: &Ui, response: &Response, range_steps: f32) -> f32 {
    if response.clicked() || response.drag_started() {
        response.request_focus();
    }
    if !response.has_focus() {
        return 0.0;
    }
    let id = response.id;
    ui.memory_mut(|m| {
        m.set_focus_lock_filter(
            id,
            EventFilter {
                horizontal_arrows: true,
                vertical_arrows: true,
                ..Default::default()
            },
        )
    });
    let held_id = id.with("key_held_since");
    let (events, now) = ui.input(|i| (i.events.clone(), i.time));
    let mut total = 0.0;
    for event in events {
        let Event::Key {
            key,
            pressed: true,
            repeat,
            modifiers,
            ..
        } = event
        else {
            continue;
        };
        if modifiers.any() && !modifiers.shift_only() {
            continue;
        }
        let dir = match key {
            Key::ArrowUp | Key::ArrowRight => 1.0,
            Key::ArrowDown | Key::ArrowLeft => -1.0,
            _ => continue,
        };
        if !repeat {
            ui.data_mut(|d| d.insert_temp(held_id, now));
        }
        let since = ui.data(|d| d.get_temp::<f64>(held_id)).unwrap_or(now);
        total += dir * key_steps(repeat, now - since, range_steps);
    }
    total
}

#[derive(Clone)]
struct EditState {
    text: String,
    fresh: bool,
}

fn edit_id(control: Id) -> Id {
    control.with("inplace_edit")
}

/// Whether `control` is being typed into.
pub fn is_editing(ctx: &Context, control: Id) -> bool {
    ctx.data(|d| d.get_temp::<EditState>(edit_id(control)).is_some())
}

/// Starts typing into `control`, prefilled with `text`. Offline, nothing
/// opens and a "`label` not sent" notification says why (070).
pub fn begin_edit(ctx: &Context, control: Id, text: String, label: &str) {
    let link = engine_link_of(ctx);
    if !link.is_online() {
        notify_error(ctx, format!("{label} not sent: engine {}", link.word()));
        return;
    }
    ctx.data_mut(|d| d.insert_temp(edit_id(control), EditState { text, fresh: true }));
}

/// The text field floating over the readout `over` of `control` while it
/// is being typed into. It covers the readout without taking layout space,
/// so nothing moves. Returns the typed text once Enter or leaving the field
/// applies it; Esc cancels (returns `None` and closes).
pub fn edit_field(ui: &mut Ui, t: &Studio, control: Id, over: Rect) -> Option<String> {
    let eid = edit_id(control);
    let mut state = ui.data(|d| d.get_temp::<EditState>(eid))?;
    let size = vec2(
        (over.width() + 8.0).max(44.0),
        (over.height() + 2.0).max(18.0),
    );
    let rect = Rect::from_center_size(over.center(), size);
    let painter = ui.painter().clone();
    painter.rect_filled(rect, CornerRadius::ZERO, t.well);
    painter.rect_stroke(
        rect,
        CornerRadius::ZERO,
        Stroke::new(1.0_f32, t.blue),
        StrokeKind::Outside,
    );
    let field_id = eid.with("field");
    let font = FontId::new(12.0, FontFamily::Monospace);
    // Centre the text: the editable area is as wide as the text (plus the
    // cursor) and sits in the middle of the field.
    let text_w = ui
        .painter()
        .layout_no_wrap(state.text.clone(), font.clone(), t.strong)
        .size()
        .x
        + 2.0;
    let inner = Rect::from_center_size(
        rect.center(),
        vec2(text_w.min(rect.width() - 6.0).max(2.0), rect.height()),
    );
    let mut child = ui.new_child(UiBuilder::new().max_rect(inner).id_salt(field_id));
    let output = egui::TextEdit::singleline(&mut state.text)
        .id(field_id)
        .frame(egui::Frame::NONE)
        .margin(egui::Margin::ZERO)
        .font(font)
        .text_color(t.strong)
        .vertical_align(egui::Align::Center)
        .desired_width(inner.width())
        .min_size(inner.size())
        .clip_text(true)
        .show(&mut child);
    let response = output.response;
    if state.fresh {
        let mut st = output.state;
        let n = state.text.chars().count();
        st.cursor
            .set_char_range(Some(CCursorRange::two(CCursor::new(0), CCursor::new(n))));
        st.store(ui.ctx(), field_id);
        response.request_focus();
        state.fresh = false;
        ui.data_mut(|d| d.insert_temp(eid, state));
        return None;
    }
    if response.lost_focus() || !response.has_focus() {
        ui.data_mut(|d| d.remove::<EditState>(eid));
        let (esc, enter) = ui.input(|i| (i.key_pressed(Key::Escape), i.key_pressed(Key::Enter)));
        if esc || enter {
            ui.memory_mut(|m| m.request_focus(control));
        }
        if esc {
            return None;
        }
        return Some(state.text);
    }
    ui.data_mut(|d| d.insert_temp(eid, state));
    None
}

/// Parses typed text as a number, ignoring spaces and a unit suffix
/// (`%`, `°`, `x`).
pub fn parse_number(text: &str) -> Option<f32> {
    let s = text
        .trim()
        .trim_end_matches(['%', '°', 'x', 'X'])
        .trim()
        .replace(',', ".");
    let v: f32 = s.parse().ok()?;
    v.is_finite().then_some(v)
}

/// Applies typed `text` to `value`: clamped to `range`, unchanged text or
/// an empty or unparsable field leaves it alone. Returns whether it changed.
pub fn apply_typed(
    text: &str,
    shown: &str,
    value: &mut f32,
    range: &std::ops::RangeInclusive<f32>,
    parse: impl Fn(&str) -> Option<f32>,
) -> bool {
    let text = text.trim();
    if text.is_empty() || text == shown {
        return false;
    }
    let Some(v) = parse(text) else {
        return false;
    };
    let new = v.clamp(*range.start(), *range.end());
    let changed = new != *value;
    *value = new;
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_accelerate_only_when_held() {
        assert_eq!(key_steps(false, 5.0, 255.0), 1.0);
        assert_eq!(key_steps(true, 0.2, 255.0), 1.0);
        assert_eq!(key_steps(true, 2.0, 255.0), 10.0);
        assert_eq!(key_steps(true, 10.0, 255.0), 10.0);
        // Speed notches: 9 steps, never more than one per repeat.
        assert_eq!(key_steps(true, 10.0, 9.0), 1.0);
    }

    #[test]
    fn typing_is_refused_offline() {
        let ctx = Context::default();
        let id = Id::new("fader");
        set_engine_link(&ctx, EngineLink::Disconnected);
        begin_edit(&ctx, id, "10".into(), "Hue");
        assert!(!is_editing(&ctx, id));
        set_engine_link(&ctx, EngineLink::Online);
        begin_edit(&ctx, id, "10".into(), "Hue");
        assert!(is_editing(&ctx, id));
    }

    #[test]
    fn typed_values_are_clamped() {
        let mut v = 10.0;
        assert!(apply_typed(
            "300",
            "10",
            &mut v,
            &(0.0..=255.0),
            parse_number
        ));
        assert_eq!(v, 255.0);
        assert!(apply_typed(
            "-4",
            "255",
            &mut v,
            &(0.0..=255.0),
            parse_number
        ));
        assert_eq!(v, 0.0);
        assert!(!apply_typed(
            "abc",
            "0",
            &mut v,
            &(0.0..=255.0),
            parse_number
        ));
        assert!(!apply_typed("", "0", &mut v, &(0.0..=255.0), parse_number));
        assert_eq!(parse_number("45 %"), Some(45.0));
        assert_eq!(parse_number("90°"), Some(90.0));
        assert_eq!(parse_number("2x"), Some(2.0));
    }
}
