//! Desktop studio components (ticket 058): the list and tree rows, fader,
//! knob, horizontal fader, switch, segmented control, page bar and modal
//! dialog of `design/studio`, painted with the [`Studio`] tokens.
//!
//! Desktop layout only. Nothing here is drawn by the 800x480 kiosk
//! ([`Density::Touch`]): pages call these only on the desktop path, so kiosk
//! drawing stays unchanged. All sizes follow `design/studio/RULES.md`: 24px
//! minimum targets, 28px for show controls, 12px text with 11px minimum.

use crate::app::ui_tokens::{
    studio, Density, Studio, Weight, LIST_ROW_HEIGHT, PAGE_BAR_HEIGHT, PHOSPHOR_FILL_FAMILY,
    TARGET_MIN, TARGET_SHOW, TEXT, TEXT_MIN,
};
use egui::text::TextWrapping;
use egui::{
    pos2, text::LayoutJob, vec2, Align2, Color32, CornerRadius, FontFamily, FontId, Id, Pos2, Rect,
    Response, Sense, Stroke, StrokeKind, Ui,
};
use std::ops::RangeInclusive;

fn font(ui: &Ui, weight: Weight, size: f32) -> FontId {
    Density::of(ui.ctx()).font(weight, size)
}

fn mono(size: f32) -> FontId {
    FontId::new(size, FontFamily::Monospace)
}

/// Paints `text` on one line from `pos` (left centre), cut with an ellipsis
/// at `max_width`. Returns whether it was cut, so the caller can add the
/// full text as a tooltip.
pub fn paint_truncated(
    ui: &Ui,
    left_center: Pos2,
    text: &str,
    font_id: FontId,
    color: Color32,
    max_width: f32,
) -> bool {
    let mut job = LayoutJob::simple_singleline(text.to_owned(), font_id, color);
    job.wrap = TextWrapping::truncate_at_width(max_width.max(0.0));
    let galley = ui.painter().layout_job(job);
    let elided = galley.elided;
    let pos = pos2(left_center.x, left_center.y - galley.size().y / 2.0);
    ui.painter()
        .with_clip_rect(ui.clip_rect())
        .galley(pos, galley, color);
    elided
}

/// Width of `text` in `font_id`.
fn text_width(ui: &Ui, text: &str, font_id: FontId) -> f32 {
    ui.painter()
        .layout_no_wrap(text.to_owned(), font_id, Color32::WHITE)
        .size()
        .x
}

// ---------------------------------------------------------------------------
// Lists and trees
// ---------------------------------------------------------------------------

/// The recessed list background around a list or tree.
pub fn list_frame<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    let t = studio(ui.ctx());
    egui::Frame::new()
        .fill(t.list)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            add(ui)
        })
        .inner
}

/// How a list row is selected (`RULES.md`, Lists).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ListRowState {
    #[default]
    Idle,
    /// Part of the selection but not the one being worked on: thin blue
    /// outline, no fill.
    Included,
    /// The row being worked on: solid blue with a left edge.
    Active,
}

/// One row of a list or tree.
pub struct ListRow<'a> {
    pub name: &'a str,
    /// A faint ID before the name (scene IDs), strong when active.
    pub id: Option<&'a str>,
    /// A number-only count on the right.
    pub count: Option<usize>,
    pub state: ListRowState,
    /// Inside a folder: indented, with the tree line.
    pub in_folder: bool,
}

impl<'a> ListRow<'a> {
    pub fn new(name: &'a str) -> Self {
        Self {
            name,
            id: None,
            count: None,
            state: ListRowState::Idle,
            in_folder: false,
        }
    }
    pub fn id(mut self, id: &'a str) -> Self {
        self.id = Some(id);
        self
    }
    pub fn count(mut self, count: usize) -> Self {
        self.count = Some(count);
        self
    }
    pub fn state(mut self, state: ListRowState) -> Self {
        self.state = state;
        self
    }
    pub fn in_folder(mut self, yes: bool) -> Self {
        self.in_folder = yes;
        self
    }

    /// Draws the row across the available width, 24px high, with a hairline
    /// under it. A name that does not fit is cut on one line and shown in
    /// full as a tooltip.
    pub fn show(self, ui: &mut Ui) -> Response {
        let t = studio(ui.ctx());
        let (rect, response) =
            ui.allocate_exact_size(vec2(ui.available_width(), LIST_ROW_HEIGHT), Sense::click());
        if !ui.is_rect_visible(rect) {
            return response;
        }
        let p = ui.painter();
        let hovered = response.hovered();
        match self.state {
            ListRowState::Active => {
                p.rect_filled(rect, 0.0, t.blue_dim);
            }
            ListRowState::Included if hovered => {
                p.rect_filled(rect, 0.0, t.blue_tint);
            }
            _ if hovered => {
                p.rect_filled(rect, 0.0, t.head);
            }
            _ => {}
        }
        // Hairline under the row.
        p.hline(
            rect.x_range(),
            rect.bottom() - 0.5,
            Stroke::new(1.0_f32, t.hair),
        );
        let left = if self.in_folder {
            p.vline(
                rect.left() + 12.5,
                rect.y_range(),
                Stroke::new(1.0_f32, t.line),
            );
            rect.left() + 29.0
        } else {
            rect.left() + 8.0
        };
        match self.state {
            ListRowState::Active => {
                p.rect_filled(
                    Rect::from_min_size(rect.min, vec2(2.0, rect.height())),
                    0.0,
                    t.blue,
                );
            }
            ListRowState::Included => {
                p.rect_stroke(rect, 0.0, Stroke::new(1.0_f32, t.blue), StrokeKind::Inside);
            }
            ListRowState::Idle => {}
        }
        let active = self.state == ListRowState::Active;
        let name_color = if active { t.strong } else { t.text };
        let side_color = if active { t.strong } else { t.muted };
        let mut right = rect.right() - 8.0;
        if let Some(count) = self.count {
            let text = count.to_string();
            let f = font(ui, Weight::Regular, TEXT);
            right -= text_width(ui, &text, f.clone());
            ui.painter().text(
                pos2(right, rect.center().y),
                Align2::LEFT_CENTER,
                text,
                f,
                side_color,
            );
            right -= 8.0;
        }
        let mut x = left;
        if let Some(id) = self.id {
            let f = mono(TEXT_MIN);
            ui.painter().text(
                pos2(x, rect.center().y),
                Align2::LEFT_CENTER,
                id,
                f.clone(),
                side_color,
            );
            x += text_width(ui, id, f) + 6.0;
        }
        let cut = paint_truncated(
            ui,
            pos2(x, rect.center().y),
            self.name,
            font(ui, Weight::Regular, TEXT + 1.0),
            name_color,
            right - x,
        );
        let response = if cut {
            response.on_hover_text(self.name)
        } else {
            response
        };
        response.widget_info(|| {
            egui::WidgetInfo::selected(
                egui::WidgetType::SelectableLabel,
                true,
                self.state != ListRowState::Idle,
                self.name,
            )
        });
        response
    }
}

/// A folder band: title-bar grey with a 1px line on top, a chevron, a grey
/// folder icon and the name. The count shows only while collapsed. Toggle
/// `open` when the band is clicked.
pub fn folder_band(
    ui: &mut Ui,
    name: &str,
    count: Option<usize>,
    open: bool,
    first: bool,
) -> Response {
    let t = studio(ui.ctx());
    if !first {
        ui.add_space(2.0);
    }
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), LIST_ROW_HEIGHT), Sense::click());
    if !ui.is_rect_visible(rect) {
        return response;
    }
    let fill = if response.hovered() { t.ctl } else { t.head };
    let p = ui.painter();
    p.rect_filled(rect, 0.0, fill);
    p.hline(
        rect.x_range(),
        rect.top() + 0.5,
        Stroke::new(1.0_f32, t.line),
    );
    let chevron = if open {
        egui_phosphor::regular::CARET_DOWN
    } else {
        egui_phosphor::regular::CARET_RIGHT
    };
    let cy = rect.center().y;
    p.text(
        pos2(rect.left() + 12.5, cy),
        Align2::CENTER_CENTER,
        chevron,
        font(ui, Weight::Regular, TEXT_MIN),
        t.muted,
    );
    // Solid icon (`ph-fill`), open or closed like the band.
    let folder = if open {
        egui_phosphor::fill::FOLDER_OPEN
    } else {
        egui_phosphor::fill::FOLDER
    };
    p.text(
        pos2(rect.left() + 31.0, cy),
        Align2::CENTER_CENTER,
        folder,
        FontId::new(14.0, FontFamily::Name(PHOSPHOR_FILL_FAMILY.into())),
        t.muted,
    );
    let mut right = rect.right() - 8.0;
    if let (Some(count), false) = (count, open) {
        let text = count.to_string();
        let f = font(ui, Weight::Regular, TEXT);
        right -= text_width(ui, &text, f.clone());
        ui.painter()
            .text(pos2(right, cy), Align2::LEFT_CENTER, text, f, t.muted);
        right -= 8.0;
    }
    let x = rect.left() + 44.0;
    let cut = paint_truncated(
        ui,
        pos2(x, cy),
        name,
        font(ui, Weight::SemiBold, TEXT + 1.0),
        t.strong,
        right - x,
    );
    let response = if cut {
        response.on_hover_text(name)
    } else {
        response
    };
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::CollapsingHeader, true, open, name)
    });
    response
}

// ---------------------------------------------------------------------------
// Buttons, page bar, segmented control, switch
// ---------------------------------------------------------------------------

/// A flat studio button (`.mini`): `ctl` fill, blue-dim when pressed.
/// `height` is [`TARGET_MIN`] or [`TARGET_SHOW`].
pub fn mini_button(ui: &mut Ui, label: &str, pressed: bool, height: f32) -> Response {
    let t = studio(ui.ctx());
    let f = font(ui, Weight::Regular, TEXT);
    let w = text_width(ui, label, f.clone()) + 16.0;
    let (rect, response) = ui.allocate_exact_size(vec2(w.max(height), height), Sense::click());
    paint_mini(ui, t, rect, &response, label, f, pressed);
    response
        .widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, pressed, label));
    response
}

fn paint_mini(
    ui: &Ui,
    t: &Studio,
    rect: Rect,
    response: &Response,
    label: &str,
    f: FontId,
    pressed: bool,
) {
    let fill = if pressed {
        t.blue_dim
    } else if response.hovered() {
        t.ctl_hi
    } else {
        t.ctl
    };
    let p = ui.painter();
    p.rect_filled(rect, 0.0, fill);
    if response.has_focus() {
        p.rect_stroke(rect, 0.0, Stroke::new(1.0_f32, t.blue), StrokeKind::Inside);
    }
    let color = if pressed { t.strong } else { t.text };
    p.text(rect.center(), Align2::CENTER_CENTER, label, f, color);
}

/// The one page bar: 30px, title-bar grey, view tabs on the left and page
/// actions on the right. Both closures lay out left to right.
pub fn page_bar(ui: &mut Ui, left: impl FnOnce(&mut Ui), right: impl FnOnce(&mut Ui)) {
    let t = studio(ui.ctx());
    let (rect, _) =
        ui.allocate_exact_size(vec2(ui.available_width(), PAGE_BAR_HEIGHT), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, t.head);
    let inner = rect.shrink2(vec2(6.0, 0.0));
    let mut bar = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(inner)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    bar.spacing_mut().item_spacing.x = 6.0;
    left(&mut bar);
    // The actions are laid out left to right from where they ended last
    // frame, so they sit flush right in their written order.
    let width_id = ui.id().with("page_bar_actions_width");
    let width: f32 = ui.data(|d| d.get_temp(width_id)).unwrap_or(0.0);
    let actions = Rect::from_min_max(pos2(inner.right() - width, inner.top()), inner.max);
    let mut right_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(actions)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    right_ui.spacing_mut().item_spacing.x = 6.0;
    right(&mut right_ui);
    let used = right_ui.min_rect().width();
    if (used - width).abs() > 0.5 {
        ui.data_mut(|d| d.insert_temp(width_id, used));
        ui.ctx().request_repaint();
    }
}

/// View tabs for the left of the page bar (Map / Plan): a segmented control.
pub fn view_tabs(ui: &mut Ui, selected: &mut usize, tabs: &[&str]) -> Response {
    segmented(ui, selected, tabs, TARGET_MIN)
}

/// One of 2 to 5 visible options. Segments sit 2px apart; the chosen one is
/// blue-dim. Left/Right arrow keys move the choice while it has focus.
pub fn segmented(ui: &mut Ui, selected: &mut usize, options: &[&str], height: f32) -> Response {
    let t = studio(ui.ctx());
    let f = font(ui, Weight::Regular, TEXT);
    let response = ui
        .horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            let mut out: Option<Response> = None;
            for (i, label) in options.iter().enumerate() {
                let w = text_width(ui, label, f.clone()) + 16.0;
                let (rect, r) = ui.allocate_exact_size(vec2(w.max(height), height), Sense::click());
                if r.clicked() {
                    *selected = i;
                }
                if r.has_focus() {
                    let (l, rgt) = ui.input(|i| {
                        (
                            i.key_pressed(egui::Key::ArrowLeft),
                            i.key_pressed(egui::Key::ArrowRight),
                        )
                    });
                    if l && *selected > 0 {
                        *selected -= 1;
                    }
                    if rgt && *selected + 1 < options.len() {
                        *selected += 1;
                    }
                }
                paint_mini(ui, t, rect, &r, label, f.clone(), *selected == i);
                let sel = *selected == i;
                r.widget_info(|| {
                    egui::WidgetInfo::selected(egui::WidgetType::RadioButton, true, sel, *label)
                });
                out = Some(match out {
                    Some(o) => o.union(r),
                    None => r,
                });
            }
            out
        })
        .inner;
    response.unwrap_or_else(|| ui.allocate_response(vec2(0.0, height), Sense::hover()))
}

/// An on/off setting that is saved: a 34x24 well with a cap that slides
/// right when on.
pub fn switch(ui: &mut Ui, on: &mut bool) -> Response {
    let t = studio(ui.ctx());
    let (rect, mut response) = ui.allocate_exact_size(vec2(34.0, TARGET_MIN), Sense::click());
    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }
    let p = ui.painter();
    p.rect_filled(rect, 0.0, if *on { t.ctl_hi } else { t.well });
    let edge = if response.has_focus() { t.blue } else { t.line };
    p.rect_stroke(rect, 0.0, Stroke::new(1.0_f32, edge), StrokeKind::Inside);
    let x = if *on { 17.0 } else { 3.0 };
    let cap = Rect::from_min_size(rect.min + vec2(x, 4.0), vec2(14.0, 16.0));
    p.rect_filled(cap, 0.0, if *on { t.strong } else { t.capface });
    p.rect_stroke(
        cap,
        0.0,
        Stroke::new(1.0_f32, t.capedge),
        StrokeKind::Outside,
    );
    let state = *on;
    response
        .widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Checkbox, true, state, ""));
    response
}

// ---------------------------------------------------------------------------
// Fader, horizontal fader, knob
// ---------------------------------------------------------------------------

/// Paints the raised fader cap: lit top edge, shaded bottom edge, hard drop,
/// grip ridges and the indicator line (amber when the value is changed).
fn paint_cap(ui: &Ui, t: &Studio, cap: Rect, hovered: bool, ind: Color32, vertical: bool) {
    let p = ui.painter();
    let r = CornerRadius::same(3);
    p.rect_filled(cap.translate(vec2(0.0, 2.0)), r, t.capdrop);
    p.rect_filled(cap, r, t.capface);
    let inner = cap.shrink(1.0);
    p.hline(
        inner.x_range(),
        inner.top() + 1.0,
        Stroke::new(2.0_f32, t.caphi),
    );
    p.hline(
        inner.x_range(),
        inner.bottom() - 1.5,
        Stroke::new(3.0_f32, t.caplo),
    );
    if vertical {
        let mid = cap.top() + 17.0;
        for band in [(cap.top() + 4.0, 10.0), (cap.bottom() - 14.0, 9.0)] {
            let mut y = band.0;
            while y < band.0 + band.1 {
                p.hline(
                    (cap.left() + 3.0)..=(cap.right() - 3.0),
                    y + 0.5,
                    Stroke::new(1.0_f32, t.caplo),
                );
                p.hline(
                    (cap.left() + 3.0)..=(cap.right() - 3.0),
                    y + 1.5,
                    Stroke::new(1.0_f32, t.caphi),
                );
                y += 3.0;
            }
        }
        p.hline(cap.x_range(), mid, Stroke::new(2.0_f32, ind));
    } else {
        let mid = cap.center().x;
        for x0 in [cap.left() + 3.0, cap.right() - 9.0] {
            let mut x = x0;
            while x < x0 + 7.0 {
                p.vline(
                    x + 0.5,
                    (cap.top() + 4.0)..=(cap.bottom() - 5.0),
                    Stroke::new(1.0_f32, t.caplo),
                );
                p.vline(
                    x + 1.5,
                    (cap.top() + 4.0)..=(cap.bottom() - 5.0),
                    Stroke::new(1.0_f32, t.caphi),
                );
                x += 3.0;
            }
        }
        p.vline(mid, cap.y_range(), Stroke::new(2.0_f32, ind));
    }
    let edge = if hovered { t.strong } else { t.capedge };
    p.rect_stroke(cap, r, Stroke::new(1.0_f32, edge), StrokeKind::Inside);
}

fn drag_value(
    response: &Response,
    value: &mut f32,
    range: &RangeInclusive<f32>,
    delta_px: f32,
    travel: f32,
) -> bool {
    if !response.dragged() || travel <= 0.0 || delta_px == 0.0 {
        return false;
    }
    let span = range.end() - range.start();
    let new = (*value + delta_px / travel * span).clamp(*range.start(), *range.end());
    let changed = new != *value;
    *value = new;
    changed
}

fn format_value(value: f32, range: &RangeInclusive<f32>) -> String {
    if range.end() - range.start() > 10.0 {
        format!("{}", value.round() as i64)
    } else {
        format!("{value:.2}")
    }
}

/// The vertical hardware fader for fixture attributes played live: printed
/// scale, recessed slot, raised 36px cap, value and name below. `changed`
/// colours the value and indicator amber.
pub fn fader(
    ui: &mut Ui,
    value: &mut f32,
    range: RangeInclusive<f32>,
    name: &str,
    changed: bool,
    height: f32,
) -> Response {
    let t = studio(ui.ctx());
    let width = 52.0;
    let (rect, mut response) = ui.allocate_exact_size(vec2(width, height), Sense::click_and_drag());
    let track = Rect::from_min_max(rect.min, pos2(rect.right(), rect.bottom() - 34.0));
    let travel = track.height() - 36.0;
    if drag_value(&response, value, &range, -response.drag_delta().y, travel) {
        response.mark_changed();
    }
    let p = ui.painter();
    let cx = track.center().x.round();
    // Printed scale: 11 ticks either side of the slot.
    let scale = Rect::from_min_max(
        pos2(track.left() + 7.0, track.top() + 18.0),
        pos2(track.right() - 7.0, track.bottom() - 18.0),
    );
    for i in 0..=10 {
        let y = (scale.bottom() - scale.height() * i as f32 / 10.0).round() + 0.5;
        p.hline(scale.left()..=(cx - 9.0), y, Stroke::new(1.0_f32, t.scale));
        p.hline((cx + 9.0)..=scale.right(), y, Stroke::new(1.0_f32, t.scale));
    }
    let slot = Rect::from_min_max(
        pos2(cx - 3.0, track.top() + 10.0),
        pos2(cx + 3.0, track.bottom() - 10.0),
    );
    p.rect_filled(slot, CornerRadius::same(2), Color32::BLACK);
    p.rect_stroke(
        slot,
        CornerRadius::same(2),
        Stroke::new(1.0_f32, t.line),
        StrokeKind::Inside,
    );
    let frac = ((*value - range.start()) / (range.end() - range.start())).clamp(0.0, 1.0);
    let cap_top = track.bottom() - 36.0 - frac * travel;
    let cap = Rect::from_min_size(pos2(cx - 18.0, cap_top), vec2(36.0, 36.0));
    let ind = if changed { t.changed } else { t.strong };
    paint_cap(
        ui,
        t,
        cap,
        response.hovered() || response.dragged(),
        ind,
        true,
    );
    let val_color = if changed { t.changed } else { t.strong };
    ui.painter().text(
        pos2(cx, track.bottom() + 9.0),
        Align2::CENTER_CENTER,
        format_value(*value, &range),
        mono(TEXT),
        val_color,
    );
    let name_color = if response.has_focus() { t.blue } else { t.text };
    let cut = paint_truncated(
        ui,
        pos2(rect.left(), track.bottom() + 26.0),
        name,
        font(ui, Weight::Regular, TEXT),
        name_color,
        width,
    );
    let response = if cut {
        response.on_hover_text(name)
    } else {
        response
    };
    let v = *value as f64;
    response.widget_info(|| egui::WidgetInfo::slider(true, v, name));
    response
}

/// The horizontal fader for long-range values with a level feel (audio
/// gain): the vertical fader on its side, then the value and the name.
pub fn hfader(
    ui: &mut Ui,
    value: &mut f32,
    range: RangeInclusive<f32>,
    name: &str,
    track_width: f32,
) -> Response {
    let t = studio(ui.ctx());
    let height = 32.0;
    let ui_id = ui
        .horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let (track, mut response) =
                ui.allocate_exact_size(vec2(track_width, height), Sense::click_and_drag());
            let cw = 26.0;
            let travel = track.width() - cw;
            if drag_value(&response, value, &range, response.drag_delta().x, travel) {
                response.mark_changed();
            }
            let p = ui.painter();
            let cy = track.center().y.round();
            let lx = track.left() + cw / 2.0;
            let rx = track.right() - cw / 2.0;
            for i in 0..=10 {
                let x = (lx + (rx - lx) * i as f32 / 10.0).round() + 0.5;
                p.vline(
                    x,
                    (track.top() + 3.0)..=(track.top() + 10.0),
                    Stroke::new(1.0_f32, t.scale),
                );
                p.vline(
                    x,
                    (track.bottom() - 10.0)..=(track.bottom() - 3.0),
                    Stroke::new(1.0_f32, t.scale),
                );
            }
            let slot = Rect::from_min_max(pos2(lx, cy - 3.0), pos2(rx, cy + 3.0));
            p.rect_filled(slot, CornerRadius::same(2), Color32::BLACK);
            p.rect_stroke(
                slot,
                CornerRadius::same(2),
                Stroke::new(1.0_f32, t.line),
                StrokeKind::Inside,
            );
            let frac = ((*value - range.start()) / (range.end() - range.start())).clamp(0.0, 1.0);
            let cap = Rect::from_min_max(
                pos2(track.left() + frac * travel, track.top() + 1.0),
                pos2(track.left() + frac * travel + cw, track.bottom() - 3.0),
            );
            paint_cap(
                ui,
                t,
                cap,
                response.hovered() || response.dragged(),
                t.strong,
                false,
            );
            // Value right after the track, 6px apart like the masters'
            // value and name (no fixed-width box).
            ui.spacing_mut().item_spacing.x = 6.0;
            let text = format_value(*value, &range);
            let vw = text_width(ui, &text, mono(TEXT));
            let (vrect, _) = ui.allocate_exact_size(vec2(vw, height), Sense::hover());
            ui.painter().text(
                pos2(vrect.left(), vrect.center().y),
                Align2::LEFT_CENTER,
                text,
                mono(TEXT),
                t.strong,
            );
            let name_color = if response.has_focus() { t.blue } else { t.text };
            ui.label(
                egui::RichText::new(name)
                    .font(font(ui, Weight::Regular, TEXT))
                    .color(name_color),
            );
            let v = *value as f64;
            response.widget_info(|| egui::WidgetInfo::slider(true, v, name));
            response
        })
        .inner;
    ui_id
}

/// Knob size: [`KnobSize::Normal`] (58px) for masters, [`KnobSize::Small`]
/// (44px) beside the page bar's scene picker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KnobSize {
    Normal,
    Small,
}

/// The hardware knob for masters and trims set occasionally: a printed
/// 270° scale, a face with an inner ring and a pointer, value and name
/// below (to the right on a small knob). Drag up or down to turn it.
pub fn knob(
    ui: &mut Ui,
    value: &mut f32,
    range: RangeInclusive<f32>,
    name: &str,
    size: KnobSize,
) -> Response {
    let t = studio(ui.ctx());
    let d: f32 = match size {
        KnobSize::Normal => 58.0,
        KnobSize::Small => 44.0,
    };
    let value_text = format_value(*value, &range);
    // Small knobs (page bar masters) put the value and the name to the
    // right of the knob, 6px apart (`#masters .knob`); normal knobs below.
    let small = size == KnobSize::Small;
    let name_font = if small {
        font(ui, Weight::Regular, TEXT_MIN)
    } else {
        font(ui, Weight::Regular, TEXT)
    };
    let size_v = if small {
        let tw =
            text_width(ui, &value_text, mono(TEXT)).max(text_width(ui, name, name_font.clone()));
        vec2(d + 6.0 + tw, d)
    } else {
        vec2(d.max(52.0), d + 34.0)
    };
    let (rect, mut response) = ui.allocate_exact_size(size_v, Sense::click_and_drag());
    if drag_value(&response, value, &range, -response.drag_delta().y, 150.0) {
        response.mark_changed();
    }
    let c = if small {
        pos2(rect.left() + d / 2.0, rect.center().y)
    } else {
        pos2(rect.center().x, rect.top() + d / 2.0)
    };
    let r = d / 2.0;
    let frac = ((*value - range.start()) / (range.end() - range.start())).clamp(0.0, 1.0);
    let start = 135f32.to_radians();
    let sweep = 270f32.to_radians();
    let p = ui.painter();
    for i in 0..=10 {
        let a = start + sweep * i as f32 / 10.0;
        let dir = vec2(a.cos(), a.sin());
        let lit = (i as f32) / 10.0 <= frac + 1e-4;
        let color = if lit { t.strong } else { t.scale };
        p.line_segment(
            [c + dir * (r - 6.0), c + dir * (r - 1.0)],
            Stroke::new(1.5_f32, color),
        );
    }
    let face = r - 9.0;
    p.circle_filled(c, face, t.capface);
    p.circle_stroke(c, face, Stroke::new(2.0_f32, t.capedge));
    p.circle_stroke(c, face * 0.62, Stroke::new(1.5_f32, t.caphi));
    let a = start + sweep * frac;
    let dir = vec2(a.cos(), a.sin());
    p.line_segment(
        [c + dir * (face * 0.25), c + dir * (face - 3.0)],
        Stroke::new(3.0_f32, t.strong),
    );
    if response.hovered() || response.dragged() {
        p.circle_stroke(c, face + 1.0, Stroke::new(1.0_f32, t.strong));
    }
    if small {
        let x = rect.left() + d + 6.0;
        p.text(
            pos2(x, c.y),
            Align2::LEFT_BOTTOM,
            value_text,
            mono(TEXT),
            t.strong,
        );
        let name_color = if response.has_focus() {
            t.blue
        } else {
            t.muted
        };
        p.text(pos2(x, c.y), Align2::LEFT_TOP, name, name_font, name_color);
    } else {
        p.text(
            pos2(c.x, rect.top() + d + 8.0),
            Align2::CENTER_CENTER,
            value_text,
            mono(TEXT),
            t.strong,
        );
        let name_color = if response.has_focus() { t.blue } else { t.text };
        p.text(
            pos2(c.x, rect.top() + d + 25.0),
            Align2::CENTER_CENTER,
            name,
            name_font,
            name_color,
        );
    }
    let v = *value as f64;
    response.widget_info(|| egui::WidgetInfo::slider(true, v, name));
    response
}

// ---------------------------------------------------------------------------
// Modal dialog
// ---------------------------------------------------------------------------

fn open_modal_id() -> Id {
    Id::new("studio_open_modal")
}

/// Opens the modal dialog `id`. Only one dialog is open at a time: this
/// returns `false` and does nothing while another one is open.
pub fn open_modal(ctx: &egui::Context, id: Id) -> bool {
    let open = ctx.data(|d| d.get_temp::<Id>(open_modal_id()));
    match open {
        Some(other) if other != id => false,
        _ => {
            ctx.data_mut(|d| d.insert_temp(open_modal_id(), id));
            true
        }
    }
}

/// The open modal dialog, if any.
pub fn open_modal_of(ctx: &egui::Context) -> Option<Id> {
    ctx.data(|d| d.get_temp::<Id>(open_modal_id()))
}

/// Closes the modal dialog `id` if it is the open one.
pub fn close_modal(ctx: &egui::Context, id: Id) {
    if open_modal_of(ctx) == Some(id) {
        ctx.data_mut(|d| d.remove::<Id>(open_modal_id()));
    }
}

/// How a [`modal`] ended this frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModalOutcome {
    /// Still open.
    Open,
    /// Cancel, Esc or a click on the backdrop.
    Cancelled,
    /// The primary action.
    Confirmed,
}

/// The studio modal dialog `id`, if it is the open one: a 420px pane with a
/// 30px title bar over a 55% dimmed page. Cancel takes focus when the dialog
/// opens and is the default; Esc closes it; Tab cycles only through the
/// dialog's own controls. `body` adds the fields; `primary` labels the
/// confirming button (red when `danger`).
pub fn modal(
    ctx: &egui::Context,
    id: Id,
    title: &str,
    primary: &str,
    danger: bool,
    body: impl FnOnce(&mut Ui),
) -> ModalOutcome {
    if open_modal_of(ctx) != Some(id) {
        return ModalOutcome::Open;
    }
    let t = studio(ctx);
    let focus_ids = id.with("focusables");
    let mut outcome = ModalOutcome::Open;
    let shown = egui::Modal::new(id)
        .backdrop_color(Color32::from_black_alpha(140))
        .frame(
            egui::Frame::new()
                .fill(t.pane)
                .stroke(Stroke::new(1.0_f32, t.line)),
        )
        .show(ctx, |ui| {
            ui.set_width(420.0);
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            let (bar, _) = ui.allocate_exact_size(vec2(420.0, 30.0), Sense::hover());
            ui.painter().rect_filled(bar, 0.0, t.head);
            ui.painter().text(
                pos2(bar.left() + 10.0, bar.center().y),
                Align2::LEFT_CENTER,
                title,
                font(ui, Weight::Medium, TEXT + 1.0),
                t.strong,
            );
            let before: Vec<Id> = Vec::new();
            ui.data_mut(|d| d.insert_temp(focus_ids, before));
            egui::Frame::new()
                .inner_margin(egui::Margin::symmetric(10, 12))
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
                    body(ui);
                    ui.add_space(4.0);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        let ok = dialog_button(ui, primary, danger);
                        let cancel = dialog_button(ui, "Cancel", false);
                        if ui.memory(|m| m.focused()).is_none() {
                            cancel.request_focus();
                        }
                        if cancel.clicked() {
                            outcome = ModalOutcome::Cancelled;
                        }
                        if ok.clicked() {
                            outcome = ModalOutcome::Confirmed;
                        }
                        (cancel.id, ok.id)
                    })
                    .inner
                })
                .inner
        });
    if shown.should_close() && outcome == ModalOutcome::Open {
        outcome = ModalOutcome::Cancelled;
    }
    // Keep focus inside: a focused widget that is not on the dialog's layer
    // (Tab moved past the last control) goes back to Cancel.
    let (cancel_id, _) = shown.inner;
    let layer = shown.response.layer_id;
    let focused = ctx.memory(|m| m.focused());
    let inside = focused.is_some_and(|f| ctx.read_response(f).is_some_and(|r| r.layer_id == layer));
    if !inside {
        ctx.memory_mut(|m| m.request_focus(cancel_id));
    }
    if outcome != ModalOutcome::Open {
        close_modal(ctx, id);
        ctx.memory_mut(|m| m.surrender_focus(cancel_id));
    }
    outcome
}

fn dialog_button(ui: &mut Ui, label: &str, danger: bool) -> Response {
    let t = studio(ui.ctx());
    let f = font(ui, Weight::Regular, TEXT);
    let w = (text_width(ui, label, f.clone()) + 20.0).max(72.0);
    let (rect, response) = ui.allocate_exact_size(vec2(w, TARGET_SHOW), Sense::click());
    if danger {
        let fill = if response.hovered() {
            Color32::from_rgb(0xd4, 0x0f, 0x0f)
        } else {
            Color32::from_rgb(0xb8, 0x0f, 0x0f)
        };
        ui.painter().rect_filled(rect, 0.0, fill);
        if response.has_focus() {
            // The focus-visible outline (1px blue, inset), with a white
            // line inside it so it reads on the red fill.
            ui.painter()
                .rect_stroke(rect, 0.0, Stroke::new(1.0_f32, t.blue), StrokeKind::Inside);
            ui.painter().rect_stroke(
                rect.shrink(1.0),
                0.0,
                Stroke::new(1.0_f32, Color32::WHITE),
                StrokeKind::Inside,
            );
        }
        ui.painter().text(
            rect.center(),
            Align2::CENTER_CENTER,
            label,
            f,
            Color32::WHITE,
        );
    } else {
        paint_mini(ui, t, rect, &response, label, f, false);
    }
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_one_modal_opens() {
        let ctx = egui::Context::default();
        let a = Id::new("a");
        let b = Id::new("b");
        assert!(open_modal(&ctx, a));
        assert!(!open_modal(&ctx, b));
        assert_eq!(open_modal_of(&ctx), Some(a));
        close_modal(&ctx, a);
        assert!(open_modal(&ctx, b));
    }

    // The rules' minimum targets and text size, checked at compile time.
    const _: () = assert!(TARGET_MIN >= 24.0 && TARGET_SHOW >= 28.0);
    const _: () = assert!(TEXT_MIN >= 11.0 && PAGE_BAR_HEIGHT == 30.0);
}
