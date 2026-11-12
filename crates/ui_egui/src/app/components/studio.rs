//! Desktop studio components (ticket 058): the list and tree rows, fader,
//! knob, horizontal fader, switch, segmented control, page bar and modal
//! dialog of `design/studio`, painted with the [`Studio`] tokens.
//!
//! Desktop layout only. Nothing here is drawn by the 800x480 kiosk
//! ([`Density::Touch`]): pages call these only on the desktop path, so kiosk
//! drawing stays unchanged. All sizes follow `design/studio/RULES.md`: 24px
//! minimum targets, 28px for show controls, 12px text with 11px minimum.

use super::value_entry;
use crate::app::ui_tokens::{
    studio, Density, Studio, Weight, LIST_ROW_HEIGHT, PAGE_BAR_HEIGHT, PANE_TITLE_HEIGHT,
    PHOSPHOR_FILL_FAMILY, TARGET_MIN, TARGET_SHOW, TEXT, TEXT_MIN,
};
use blaulicht_shared::AnimationSpeedModifier;
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
pub fn text_width(ui: &Ui, text: &str, font_id: FontId) -> f32 {
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
    /// Muted text on the right, such as a category.
    pub note: Option<&'a str>,
}

impl<'a> ListRow<'a> {
    pub fn new(name: &'a str) -> Self {
        Self {
            name,
            id: None,
            count: None,
            state: ListRowState::Idle,
            in_folder: false,
            note: None,
        }
    }
    pub fn note(mut self, note: &'a str) -> Self {
        self.note = Some(note);
        self
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
        if let Some(note) = self.note {
            let f = font(ui, Weight::Regular, TEXT);
            right -= text_width(ui, note, f.clone());
            ui.painter().text(
                pos2(right, rect.center().y),
                Align2::LEFT_CENTER,
                note,
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
    let count = count.filter(|_| !open).map(|c| c.to_string());
    folder_band_with(ui, name, count.as_deref(), false, open, first, None)
}

/// A [`folder_band`] with free text on the right (`note`, shown open or
/// closed; mono when `mono_note`) and an optional muted icon after the name
/// (such as a link for "set by a palette").
pub fn folder_band_with(
    ui: &mut Ui,
    name: &str,
    note: Option<&str>,
    mono_note: bool,
    open: bool,
    first: bool,
    icon: Option<(&str, &str)>,
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
    if let Some(text) = note {
        let f = if mono_note {
            mono(TEXT)
        } else {
            font(ui, Weight::Regular, TEXT)
        };
        right -= text_width(ui, text, f.clone());
        ui.painter()
            .text(pos2(right, cy), Align2::LEFT_CENTER, text, f, t.muted);
        right -= 8.0;
    }
    let x = rect.left() + 44.0;
    let name_font = font(ui, Weight::SemiBold, TEXT + 1.0);
    let icon_w = if icon.is_some() { 18.0 } else { 0.0 };
    let cut = paint_truncated(
        ui,
        pos2(x, cy),
        name,
        name_font.clone(),
        t.strong,
        right - x - icon_w,
    );
    let mut response = if cut {
        response.on_hover_text(name)
    } else {
        response
    };
    if let Some((glyph, tip)) = icon {
        let nx = x + text_width(ui, name, name_font).min(right - x - icon_w) + 6.0;
        let ir = Rect::from_min_size(pos2(nx, cy - 7.0), vec2(14.0, 14.0));
        ui.painter().text(
            ir.center(),
            Align2::CENTER_CENTER,
            glyph,
            font(ui, Weight::Regular, TEXT),
            t.muted,
        );
        if ui.rect_contains_pointer(ir) {
            response = response.on_hover_text(tip);
        }
    }
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
    // 28px bar buttons get 10px padding, 24px ones 8px (`studio.css`).
    let pad = if height >= TARGET_SHOW { 10.0 } else { 8.0 };
    let w = text_width(ui, label, f.clone()) + 2.0 * pad;
    let (rect, response) = ui.allocate_exact_size(vec2(w.max(height), height), Sense::click());
    paint_mini(ui, t, rect, &response, label, f, pressed);
    response
        .widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, pressed, label));
    response
}

/// A [`mini_button`] `width` wide, its label centred (the Identify panel's
/// grid buttons).
pub fn mini_button_w(ui: &mut Ui, label: &str, width: f32, height: f32) -> Response {
    let t = studio(ui.ctx());
    let f = font(ui, Weight::Regular, TEXT);
    let (rect, response) = ui.allocate_exact_size(vec2(width, height), Sense::click());
    paint_mini(ui, t, rect, &response, label, f, false);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
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

/// The page bar's Blackout (`.okblack`, 059): always filled red with white
/// text, [`TARGET_SHOW`] tall. It reads "Restore" while Grand Master is at 0
/// (`blk()` in the mockup) and "Blackout" otherwise.
pub fn blackout_button(ui: &mut Ui, on: bool) -> Response {
    const RED: Color32 = Color32::from_rgb(0xb8, 0x0f, 0x0f);
    const RED_HI: Color32 = Color32::from_rgb(0xd4, 0x0f, 0x0f);
    let t = studio(ui.ctx());
    let f = font(ui, Weight::Regular, TEXT);
    let label = if on { "Restore" } else { "Blackout" };
    let w = text_width(ui, label, f.clone()) + 16.0;
    let (rect, response) = ui.allocate_exact_size(vec2(w, TARGET_SHOW), Sense::click());
    let fill = if response.hovered() { RED_HI } else { RED };
    let color = Color32::WHITE;
    let p = ui.painter();
    p.rect_filled(rect, 0.0, fill);
    if response.has_focus() {
        p.rect_stroke(rect, 0.0, Stroke::new(1.0_f32, t.blue), StrokeKind::Inside);
    }
    p.text(rect.center(), Align2::CENTER_CENTER, label, f, color);
    response.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, on, label));
    response.on_hover_text(if on {
        "Restore Grand Master (Cmd/Ctrl+B)"
    } else {
        "Grand Master to 0 (Cmd/Ctrl+B)"
    })
}

/// Paints a title bar: `head` fill, a light line on top and, on a 40px top
/// bar, the 1px line at the bottom (`studio.css` "Top bars").
fn paint_title_bar(ui: &Ui, t: &Studio, rect: Rect, top_bar: bool) {
    let p = ui.painter();
    p.rect_filled(rect, 0.0, t.head);
    let dark = ui.ctx().global_style().visuals.dark_mode;
    let lit = Color32::from_white_alpha(if dark { 15 } else { 128 });
    p.hline(rect.x_range(), rect.top() + 0.5, Stroke::new(1.0_f32, lit));
    if top_bar {
        p.hline(
            rect.x_range(),
            rect.bottom() - 0.5,
            Stroke::new(1.0_f32, t.line),
        );
    }
}

/// The one page bar: 40px, title-bar grey, view tabs on the left and page
/// actions on the right (28px buttons). Both closures lay out left to right.
pub fn page_bar(ui: &mut Ui, left: impl FnOnce(&mut Ui), right: impl FnOnce(&mut Ui)) {
    let t = studio(ui.ctx());
    let (rect, _) =
        ui.allocate_exact_size(vec2(ui.available_width(), PAGE_BAR_HEIGHT), Sense::hover());
    paint_title_bar(ui, t, rect, true);
    let inner = rect.shrink2(vec2(8.0, 0.0));
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

/// A pane title bar. The first row of panes on a master-detail page
/// (`top`) is 40px like the page bar with 28px buttons; lower panes keep
/// 26px. `prefix` is a muted label before the name ("Selected group:",
/// "Fixture:"). `actions` lays out its buttons flush right.
pub fn pane_header(
    ui: &mut Ui,
    prefix: Option<&str>,
    name: &str,
    top: bool,
    actions: impl FnOnce(&mut Ui),
) -> Rect {
    let t = studio(ui.ctx());
    let height = if top {
        PAGE_BAR_HEIGHT
    } else {
        PANE_TITLE_HEIGHT
    };
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    paint_title_bar(ui, t, rect, top);
    let inner = rect.shrink2(vec2(8.0, 0.0));
    // Actions flush right, measured last frame like the page bar's.
    let width_id = ui.id().with(("pane_header_actions", name));
    let width: f32 = ui.data(|d| d.get_temp(width_id)).unwrap_or(0.0);
    let mut right_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(Rect::from_min_max(
                pos2(inner.right() - width, inner.top()),
                inner.max,
            ))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    right_ui.spacing_mut().item_spacing.x = 6.0;
    actions(&mut right_ui);
    let used = if right_ui.min_rect().width() > 0.5 {
        right_ui.min_rect().width()
    } else {
        0.0
    };
    if (used - width).abs() > 0.5 {
        ui.data_mut(|d| d.insert_temp(width_id, used));
        ui.ctx().request_repaint();
    }
    let size = TEXT + 1.0;
    let cy = rect.center().y;
    let mut x = inner.left();
    let right = inner.right() - if used > 0.0 { used + 8.0 } else { 0.0 };
    if let Some(prefix) = prefix {
        let f = font(ui, Weight::Regular, size);
        let w = text_width(ui, prefix, f.clone());
        ui.painter()
            .text(pos2(x, cy), Align2::LEFT_CENTER, prefix, f, t.muted);
        x += w + 6.0;
    }
    let cut = paint_truncated(
        ui,
        pos2(x, cy),
        name,
        font(ui, Weight::Medium, size),
        t.strong,
        (right - x).max(0.0),
    );
    if cut {
        ui.interact(rect, width_id.with("tip"), Sense::hover())
            .on_hover_text(name);
    }
    rect
}

/// The lit on/off button (`.livebtn`): a dot before a fixed label, grey
/// when off and blinking red when on (steady with reduced motion). The
/// label never changes, so the button keeps its width when toggled. Used
/// by Exclusive preview, Identify and Initialize.
pub fn live_button(ui: &mut Ui, label: &str, on: bool, height: f32) -> Response {
    let t = studio(ui.ctx());
    let f = font(ui, Weight::Regular, TEXT);
    let pad = if height >= TARGET_SHOW { 10.0 } else { 8.0 };
    let w = live_button_width(ui, label, height);
    let (rect, response) = ui.allocate_exact_size(vec2(w, height), Sense::click());
    if ui.is_rect_visible(rect) {
        let fill = if on {
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
        let cy = rect.center().y;
        let dot = pos2(rect.left() + pad + 4.0, cy);
        let color = if on {
            let reduced = reduced_motion();
            let time = ui.input(|i| i.time);
            // 1 s period, dim for the second half (`liveblink`, steps(1)).
            let dim = !reduced && time.fract() >= 0.5;
            if !reduced {
                let wait = if dim {
                    1.0 - time.fract()
                } else {
                    0.5 - time.fract()
                };
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_secs_f64(wait.max(0.01)));
            }
            t.red.gamma_multiply(if dim { 0.2 } else { 1.0 })
        } else {
            t.muted.gamma_multiply(0.6)
        };
        p.circle_filled(dot, 4.0, color);
        let text = if on { t.strong } else { t.text };
        p.text(
            pos2(rect.left() + pad + 8.0 + 7.0, cy),
            Align2::LEFT_CENTER,
            label,
            f,
            text,
        );
    }
    response.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, on, label));
    response
}

/// Width of a [`live_button`]: padding, dot, gap and label. It does not
/// depend on the state.
pub fn live_button_width(ui: &Ui, label: &str, height: f32) -> f32 {
    let pad = if height >= TARGET_SHOW { 10.0 } else { 8.0 };
    pad + 8.0 + 7.0 + text_width(ui, label, font(ui, Weight::Regular, TEXT)) + pad
}

/// Whether the system asks for reduced motion (`BLAULICHT_REDUCED_MOTION`
/// set to anything but `0`; egui has no platform query for it).
pub fn reduced_motion() -> bool {
    static REDUCED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *REDUCED.get_or_init(|| {
        std::env::var("BLAULICHT_REDUCED_MOTION").is_ok_and(|v| !v.is_empty() && v != "0")
    })
}

/// One step of a scrub drag: `start` moved by `step` per 4px of `dx`
/// (Shift: `step / 10`), rounded to the step and clamped to `range`.
pub fn scrub_value(start: f64, dx: f32, step: f64, fine: bool, range: &RangeInclusive<f64>) -> f64 {
    let st = if fine { step / 10.0 } else { step };
    let v = start + (dx / 4.0).round() as f64 * st;
    let v = (v / st).round() * st;
    v.clamp(*range.start(), *range.end())
}

#[derive(Clone, Copy)]
struct ScrubState {
    start: f64,
    origin_x: f32,
    on: bool,
    cancelled: bool,
}

/// A number field that scrubs (`input.num`): drag sideways to change it by
/// `step` per 4px, Shift is ten times finer, Esc restores the start value;
/// the value applies live while dragging. A click without a drag edits the
/// text: Enter or leaving the field applies, Esc cancels. Returns the
/// response; `changed()` is set on every applied change.
pub fn num_field(
    ui: &mut Ui,
    value: &mut f64,
    step: f64,
    range: RangeInclusive<f64>,
    decimals: usize,
    width: f32,
) -> Response {
    let t = studio(ui.ctx());
    let id = ui.next_auto_id();
    let edit_id = id.with("edit");
    let text_id = id.with("text");
    let scrub_id = id.with("scrub");
    let editing = ui.data(|d| d.get_temp::<bool>(edit_id)).unwrap_or(false);
    let f = mono(TEXT);
    if editing {
        let mut text: String = ui
            .data(|d| d.get_temp(text_id))
            .unwrap_or_else(|| format!("{:.*}", decimals, value));
        let out = egui::TextEdit::singleline(&mut text)
            .id(id.with("te"))
            .font(f)
            .desired_width(width - 12.0)
            .min_size(vec2(width, TARGET_MIN))
            .vertical_align(egui::Align::Center)
            .show(ui);
        let mut response = out.response.response;
        if !response.has_focus() && ui.data(|d| d.get_temp::<bool>(id.with("focused")).is_none()) {
            response.request_focus();
            ui.data_mut(|d| d.insert_temp(id.with("focused"), true));
        }
        let esc = ui.input(|i| i.key_pressed(egui::Key::Escape));
        let done = response.lost_focus() || esc;
        if done {
            if !esc {
                if let Ok(v) = text.trim().replace(',', ".").parse::<f64>() {
                    let v = v.clamp(*range.start(), *range.end());
                    if v != *value {
                        *value = v;
                        response.mark_changed();
                    }
                }
            }
            ui.data_mut(|d| {
                d.remove::<bool>(edit_id);
                d.remove::<String>(text_id);
                d.remove::<bool>(id.with("focused"));
            });
        } else {
            ui.data_mut(|d| d.insert_temp(text_id, text));
        }
        return response;
    }

    let (rect, mut response) =
        ui.allocate_exact_size(vec2(width, TARGET_MIN), Sense::click_and_drag());
    let mut scrub: Option<ScrubState> = ui.data(|d| d.get_temp(scrub_id));
    if response.drag_started() {
        let x = ui.input(|i| i.pointer.press_origin().map_or(0.0, |p| p.x));
        scrub = Some(ScrubState {
            start: *value,
            origin_x: x,
            on: false,
            cancelled: false,
        });
    }
    if let Some(s) = scrub.as_mut() {
        // egui aborts the drag itself on Esc, so `dragged()` is already false
        // in that frame: check Esc for any running scrub.
        let esc_now = !s.cancelled && ui.input(|i| i.key_pressed(egui::Key::Escape));
        if esc_now {
            s.cancelled = true;
            if *value != s.start {
                *value = s.start;
                response.mark_changed();
            }
        } else if response.dragged() && !s.cancelled {
            let (x, shift, esc) = ui.input(|i| {
                (
                    i.pointer.latest_pos().map_or(s.origin_x, |p| p.x),
                    i.modifiers.shift,
                    i.key_pressed(egui::Key::Escape),
                )
            });
            let dx = x - s.origin_x;
            if !s.on && dx.abs() >= 3.0 {
                s.on = true;
            }
            if esc {
                s.cancelled = true;
                if *value != s.start {
                    *value = s.start;
                    response.mark_changed();
                }
            } else if s.on {
                let v = scrub_value(s.start, dx, step, shift, &range);
                if v != *value {
                    *value = v;
                    response.mark_changed();
                }
            }
        }
        if response.drag_stopped() || !response.is_pointer_button_down_on() {
            scrub = None;
        }
    }
    let scrubbing = scrub.is_some_and(|s| s.on && !s.cancelled);
    match scrub {
        Some(s) => ui.data_mut(|d| {
            d.insert_temp(scrub_id, s);
        }),
        None => ui.data_mut(|d| d.remove::<ScrubState>(scrub_id)),
    }
    if response.clicked() {
        ui.data_mut(|d| d.insert_temp(edit_id, true));
        ui.ctx().request_repaint();
    }
    if response.hovered() || scrubbing {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
    }
    if ui.is_rect_visible(rect) {
        let p = ui.painter();
        p.rect_filled(rect, 0.0, t.well);
        let edge = if scrubbing || response.has_focus() {
            t.blue
        } else {
            t.line
        };
        p.rect_stroke(rect, 0.0, Stroke::new(1.0_f32, edge), StrokeKind::Inside);
        p.text(
            pos2(rect.left() + 6.0, rect.center().y),
            Align2::LEFT_CENTER,
            format!("{:.*}", decimals, value),
            f,
            t.strong,
        );
    }
    response
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
                // 28px bar buttons get 10px padding, 24px ones 8px (`studio.css`).
                let pad = if height >= TARGET_SHOW { 10.0 } else { 8.0 };
                let w = text_width(ui, label, f.clone()) + 2.0 * pad;
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

/// The smallest unit of a control: 1 on wide ranges (DMX values, percent,
/// degrees), 0.01 on short ones, matching [`format_value`].
fn unit_of(range: &RangeInclusive<f32>) -> f32 {
    if range.end() - range.start() > 10.0 {
        1.0
    } else {
        0.01
    }
}

/// Moves `value` by `steps` units of `unit`, snapped to the unit and
/// clamped. Returns whether it changed.
fn nudge(value: &mut f32, range: &RangeInclusive<f32>, unit: f32, steps: f32) -> bool {
    if steps == 0.0 {
        return false;
    }
    let new = (((*value - range.start()) / unit).round() + steps) * unit + range.start();
    let new = new.clamp(*range.start(), *range.end());
    let changed = new != *value;
    *value = new;
    changed
}

/// Arrow keys (focused) and typing in place (double-click, see
/// `value_entry`) for a control with a numeric readout. `edit_here` says
/// whether a double-click this frame should open the field; `readout` is
/// where the value is painted. Returns whether the value changed.
#[allow(clippy::too_many_arguments)]
fn value_keys_and_typing(
    ui: &mut Ui,
    t: &Studio,
    response: &Response,
    value: &mut f32,
    range: &RangeInclusive<f32>,
    unit: f32,
    edit_here: bool,
    shown: &str,
    readout: Rect,
    name: &str,
    parse: &dyn Fn(&str) -> Option<f32>,
) -> bool {
    let mut changed = false;
    let steps = value_entry::arrow_steps(ui, response, (range.end() - range.start()) / unit);
    if !value_entry::is_editing(ui.ctx(), response.id) {
        changed |= nudge(value, range, unit, steps);
    }
    if edit_here && response.double_clicked() {
        value_entry::begin_edit(ui.ctx(), response.id, shown.to_owned(), name);
    }
    if let Some(text) = value_entry::edit_field(ui, t, response.id, readout) {
        changed |= value_entry::apply_typed(&text, shown, value, range, parse);
    }
    changed
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
    let cx = track.center().x.round();
    // Double-click the value or the name below the track to type a value.
    let shown = format_value(*value, &range);
    let readout = Rect::from_center_size(
        pos2(cx, track.bottom() + 9.0),
        vec2(text_width(ui, &shown, mono(TEXT)), 16.0),
    );
    let on_text = response
        .interact_pointer_pos()
        .or(response.hover_pos())
        .is_some_and(|p| p.y >= track.bottom());
    if value_keys_and_typing(
        ui,
        t,
        &response,
        value,
        &range,
        unit_of(&range),
        on_text,
        &shown,
        readout,
        name,
        &value_entry::parse_number,
    ) {
        response.mark_changed();
    }
    let editing = value_entry::is_editing(ui.ctx(), response.id);
    let p = ui.painter();
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
    if !editing {
        ui.painter().text(
            pos2(cx, track.bottom() + 9.0),
            Align2::CENTER_CENTER,
            format_value(*value, &range),
            mono(TEXT),
            val_color,
        );
    }
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
            // Typing in place over the value right of the track.
            let shown = format_value(*value, &range);
            let readout = Rect::from_min_size(
                pos2(track.right() + 6.0, track.center().y - 8.0),
                vec2(text_width(ui, &shown, mono(TEXT)), 16.0),
            );
            if value_keys_and_typing(
                ui,
                t,
                &response,
                value,
                &range,
                unit_of(&range),
                true,
                &shown,
                readout,
                name,
                &value_entry::parse_number,
            ) {
                response.mark_changed();
            }
            let editing = value_entry::is_editing(ui.ctx(), response.id);
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
            if !editing {
                ui.painter().text(
                    pos2(vrect.left(), vrect.center().y),
                    Align2::LEFT_CENTER,
                    text,
                    mono(TEXT),
                    t.strong,
                );
            }
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
/// (32px, `.prog #masters`) beside the page bar's scene picker.
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
    let unit = unit_of(&range);
    knob_impl(
        ui,
        value,
        range,
        name,
        size,
        KnobKind::Linear(unit),
        &format_value,
        &value_entry::parse_number,
    )
}

/// The speed knob: the [`knob`] with one detent per
/// [`AnimationSpeedModifier`], shown and typed as `1/4`, `1x`, `2x`. Arrow
/// keys move one notch per press and never accelerate.
pub fn speed_knob(
    ui: &mut Ui,
    speed: &mut AnimationSpeedModifier,
    name: &str,
    size: KnobSize,
) -> Response {
    let last = (AnimationSpeedModifier::ALL.len() - 1) as f32;
    let mut index = speed.as_index() as f32;
    let response = knob_impl(
        ui,
        &mut index,
        0.0..=last,
        name,
        size,
        KnobKind::Notched,
        &|v, _| speed_label(AnimationSpeedModifier::from_index(v.round() as usize)),
        &parse_speed,
    );
    let new = AnimationSpeedModifier::from_index(index.round().max(0.0) as usize);
    let changed = new != *speed;
    *speed = new;
    let mut response = response;
    if changed {
        response.mark_changed();
    }
    response
}

/// `1/4`, `1/2`, `1x`, `2x`: fractions as they are, whole speeds with `x`.
pub fn speed_label(speed: AnimationSpeedModifier) -> String {
    let s = speed.as_str();
    if s.contains('/') {
        s.to_owned()
    } else {
        format!("{s}x")
    }
}

/// Typed speed text to the index of the nearest speed notch: `1/4`,
/// `0.25`, `2`, `2x` all work.
fn parse_speed(text: &str) -> Option<f32> {
    let s = text.trim().trim_end_matches(['x', 'X']).trim();
    let v = match s.split_once('/') {
        Some((a, b)) => a.trim().parse::<f32>().ok()? / b.trim().parse::<f32>().ok()?,
        None => value_entry::parse_number(s)?,
    };
    if !v.is_finite() || v <= 0.0 {
        return None;
    }
    let target = v.log2();
    AnimationSpeedModifier::ALL
        .iter()
        .map(|m| {
            let label = m.as_str();
            let mv = match label.split_once('/') {
                Some((a, b)) => a.parse::<f32>().unwrap_or(1.0) / b.parse::<f32>().unwrap_or(1.0),
                None => label.parse::<f32>().unwrap_or(1.0),
            };
            ((mv.log2() - target).abs(), m.as_index())
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, i)| i as f32)
}

#[derive(Clone, Copy)]
enum KnobKind {
    /// Continuous, moved by this unit with the arrow keys.
    Linear(f32),
    /// Whole-number detents (speeds): drags snap, arrows move one notch.
    Notched,
}

#[allow(clippy::too_many_arguments)]
fn knob_impl(
    ui: &mut Ui,
    value: &mut f32,
    range: RangeInclusive<f32>,
    name: &str,
    size: KnobSize,
    kind: KnobKind,
    fmt: &dyn Fn(f32, &RangeInclusive<f32>) -> String,
    parse: &dyn Fn(&str) -> Option<f32>,
) -> Response {
    let t = studio(ui.ctx());
    let d: f32 = match size {
        KnobSize::Normal => 58.0,
        KnobSize::Small => 32.0,
    };
    let value_text = fmt(*value, &range);
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
    let unit = match kind {
        KnobKind::Linear(unit) => {
            if drag_value(&response, value, &range, -response.drag_delta().y, 150.0) {
                response.mark_changed();
            }
            unit
        }
        KnobKind::Notched => {
            // Drag a hidden continuous position and snap the value to it.
            let acc_id = response.id.with("notch_drag");
            let mut acc = if response.drag_started() {
                *value
            } else {
                ui.data(|d| d.get_temp::<f32>(acc_id)).unwrap_or(*value)
            };
            if drag_value(&response, &mut acc, &range, -response.drag_delta().y, 150.0) {
                let snapped = acc.round();
                if snapped != *value {
                    *value = snapped;
                    response.mark_changed();
                }
            }
            ui.data_mut(|d| d.insert_temp(acc_id, acc));
            1.0
        }
    };
    let c = if small {
        pos2(rect.left() + d / 2.0, rect.center().y)
    } else {
        pos2(rect.center().x, rect.top() + d / 2.0)
    };
    let shown = value_text.clone();
    let vw = text_width(ui, &shown, mono(TEXT));
    let readout = if small {
        let x = rect.left() + d + 6.0;
        Rect::from_min_size(pos2(x, c.y - 16.0), vec2(vw, 16.0))
    } else {
        Rect::from_center_size(pos2(c.x, rect.top() + d + 8.0), vec2(vw, 16.0))
    };
    if value_keys_and_typing(
        ui, t, &response, value, &range, unit, true, &shown, readout, name, parse,
    ) {
        response.mark_changed();
    }
    let editing = value_entry::is_editing(ui.ctx(), response.id);
    let value_text = fmt(*value, &range);
    let r = d / 2.0;
    let frac = ((*value - range.start()) / (range.end() - range.start())).clamp(0.0, 1.0);
    let start = 135f32.to_radians();
    let sweep = 270f32.to_radians();
    let p = ui.painter();
    let ticks = match kind {
        KnobKind::Notched => (range.end() - range.start()).round().max(1.0) as usize,
        KnobKind::Linear(_) => 10,
    };
    for i in 0..=ticks {
        let a = start + sweep * i as f32 / ticks as f32;
        let dir = vec2(a.cos(), a.sin());
        let lit = (i as f32) / ticks as f32 <= frac + 1e-4;
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
        if !editing {
            p.text(
                pos2(x, c.y),
                Align2::LEFT_BOTTOM,
                value_text,
                mono(TEXT),
                t.strong,
            );
        }
        let name_color = if response.has_focus() {
            t.blue
        } else {
            t.muted
        };
        p.text(pos2(x, c.y), Align2::LEFT_TOP, name, name_font, name_color);
    } else {
        if !editing {
            p.text(
                pos2(c.x, rect.top() + d + 8.0),
                Align2::CENTER_CENTER,
                value_text,
                mono(TEXT),
                t.strong,
            );
        }
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
    fn scrub_steps_per_4px_finer_with_shift_and_clamps() {
        let r = 0.0..=10.0;
        assert_eq!(scrub_value(5.0, 8.0, 1.0, false, &r), 7.0);
        assert_eq!(scrub_value(5.0, 1.0, 1.0, false, &r), 5.0);
        assert!((scrub_value(5.0, 8.0, 1.0, true, &r) - 5.2).abs() < 1e-9);
        assert_eq!(scrub_value(5.0, -400.0, 1.0, false, &r), 0.0);
        assert_eq!(scrub_value(5.0, 400.0, 1.0, false, &r), 10.0);
    }

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
    const _: () = assert!(TEXT_MIN >= 11.0 && PAGE_BAR_HEIGHT == 40.0 && PANE_TITLE_HEIGHT == 26.0);
}

#[cfg(test)]
mod value_tests {
    use super::*;

    #[test]
    fn speeds_parse_to_the_nearest_notch() {
        let idx = |m: AnimationSpeedModifier| Some(m.as_index() as f32);
        assert_eq!(parse_speed("1/4"), idx(AnimationSpeedModifier::_1_4));
        assert_eq!(parse_speed("0.25"), idx(AnimationSpeedModifier::_1_4));
        assert_eq!(parse_speed("2x"), idx(AnimationSpeedModifier::_2));
        assert_eq!(parse_speed("100"), idx(AnimationSpeedModifier::_32));
        assert_eq!(parse_speed("abc"), None);
        assert_eq!(speed_label(AnimationSpeedModifier::_2), "2x");
        assert_eq!(speed_label(AnimationSpeedModifier::_1_8), "1/8");
    }

    #[test]
    fn nudge_moves_by_the_unit_and_clamps() {
        let r = 0.0..=255.0;
        let mut v = 254.0;
        assert!(nudge(&mut v, &r, 1.0, 1.0));
        assert_eq!(v, 255.0);
        assert!(!nudge(&mut v, &r, 1.0, 10.0));
        let mut s = 4.0;
        assert!(nudge(&mut s, &(0.0..=9.0), 1.0, -1.0));
        assert_eq!(s, 3.0);
    }
}
