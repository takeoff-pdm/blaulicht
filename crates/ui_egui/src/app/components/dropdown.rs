//! One dropdown for every choice list (ticket 071), the egui port of
//! `BL.dropdown` in `design/studio/chrome.js`.
//!
//! A popover on the pane surface with a 1px line. It opens below its anchor
//! (above when there is no room), at most 480px tall. From 8 options it gets
//! a filter field. Arrow keys move, Home/End jump, Enter (or Space without a
//! filter) picks, Esc, Tab or a click outside closes. Groups draw as folder
//! bands and every row can carry a meta text on the right (counts, ids).
//! The multi-select variant has checks and an All row, Alt-click picks only
//! that row, and the list stays open.
//!
//! Desktop only ([`Density::Compact`]). The kiosk keeps egui's `ComboBox`:
//! [`kiosk_combo`] draws it exactly as the pages did before, so kiosk
//! drawing stays the same. Pages branch with [`is_desktop`].

use crate::app::ui_tokens::{studio, Density, Studio, Weight, PHOSPHOR_FILL_FAMILY, TEXT};
use egui::{
    pos2, vec2, Align2, Color32, FontFamily, FontId, Id, Key, LayerId, Modifiers, Order, Rect,
    Response, Sense, Stroke, StrokeKind, Ui,
};
use std::hash::Hash;

/// Row height of an option (`.ddrow`).
const ROW: f32 = 26.0;
/// Height of a folder band (`.ddgroup`).
const GROUP: f32 = 24.0;
/// Height of the filter strip including its bottom line (`.ddq`).
const FILTER: f32 = 37.0;
/// Space above and below the rows (`.ddlist` padding).
const LIST_PAD: f32 = 2.0;
const MAX_HEIGHT: f32 = 480.0;
const MIN_WIDTH: f32 = 180.0;
const MAX_WIDTH: f32 = 360.0;
/// From this many options the list gets a filter field.
const FILTER_FROM: usize = 8;

/// Whether `ui` draws the desktop layout, the only one that uses the
/// studio dropdown.
pub fn is_desktop(ui: &Ui) -> bool {
    Density::of(ui.ctx()).is_compact()
}

/// The kiosk's choice list: egui's `ComboBox`, unchanged. With a `label` it
/// is `ComboBox::from_label` (the label is then its id), else
/// `ComboBox::from_id_salt(id_salt)`.
pub fn kiosk_combo(
    ui: &mut Ui,
    id_salt: impl Hash,
    label: Option<&str>,
    selected_text: &str,
    add_contents: impl FnOnce(&mut Ui),
) -> Response {
    let combo = match label {
        Some(label) => egui::ComboBox::from_label(label),
        None => egui::ComboBox::from_id_salt(id_salt),
    };
    combo
        .selected_text(selected_text)
        .show_ui(ui, add_contents)
        .response
}

/// One option.
#[derive(Clone, Debug, Default)]
pub struct Item {
    pub label: String,
    /// Muted mono text on the right: a count or an id.
    pub meta: Option<String>,
    /// Folder band the row sits under. Consecutive rows with the same group
    /// share one band.
    pub group: Option<String>,
    /// A leading icon glyph and its colour.
    pub icon: Option<(&'static str, Color32)>,
    /// The icon is a solid (`ph-fill`) glyph.
    pub icon_fill: bool,
    /// The "none" option ("Select plugin"): label in muted.
    pub none: bool,
}

impl Item {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            ..Self::default()
        }
    }
    pub fn meta(mut self, meta: impl Into<String>) -> Self {
        self.meta = Some(meta.into());
        self
    }
    pub fn group(mut self, group: impl Into<String>) -> Self {
        self.group = Some(group.into());
        self
    }
    pub fn icon(mut self, glyph: &'static str, color: Color32) -> Self {
        self.icon = Some((glyph, color));
        self
    }
    /// A solid (`egui_phosphor::fill`) leading icon.
    pub fn icon_fill(mut self, glyph: &'static str, color: Color32) -> Self {
        self.icon = Some((glyph, color));
        self.icon_fill = true;
        self
    }
    pub fn none(mut self) -> Self {
        self.none = true;
        self
    }
}

/// Labels as plain options.
pub fn items<S: AsRef<str>>(labels: impl IntoIterator<Item = S>) -> Vec<Item> {
    labels.into_iter().map(|l| Item::new(l.as_ref())).collect()
}

/// Options for [`Popover`].
#[derive(Clone, Debug)]
pub struct Options {
    /// Minimum width; the anchor's width when `None` (at least 180px).
    pub width: Option<f32>,
    /// Show the filter field; from 8 options when `None`.
    pub filter: Option<bool>,
    pub placeholder: String,
    pub empty: String,
    /// Dim the page behind the list. Only the Programmer scene picker does.
    pub dim: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            width: None,
            filter: None,
            placeholder: "Filter".into(),
            empty: "Nothing matches.".into(),
            dim: false,
        }
    }
}

/// What happened in the popover this frame.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum Outcome {
    #[default]
    None,
    /// Single-select: this option was picked; the list closed.
    Picked(usize),
    /// Multi-select: the checks changed; the list stays open.
    Changed,
}

/// Which list is open. One at a time, like the mockup.
fn open_key() -> Id {
    Id::new("bl_dropdown_open")
}

/// The open dropdown's id.
pub fn open_id(ctx: &egui::Context) -> Option<Id> {
    ctx.data(|d| d.get_temp::<Option<Id>>(open_key())).flatten()
}

pub fn is_open(ctx: &egui::Context, id: Id) -> bool {
    open_id(ctx) == Some(id)
}

/// Opens the dropdown `id`, closing any other one.
pub fn open(ctx: &egui::Context, id: Id) {
    ctx.data_mut(|d| {
        d.insert_temp(open_key(), Some(id));
        d.remove_temp::<State>(id.with("state"));
    });
}

/// Closes the open dropdown. `refocus` gives the keyboard back to
/// `anchor` (after a pick or Esc).
pub fn close(ctx: &egui::Context, anchor: Option<Id>) {
    ctx.data_mut(|d| d.remove_temp::<Option<Id>>(open_key()));
    if let Some(anchor) = anchor {
        ctx.memory_mut(|m| m.request_focus(anchor));
    }
}

/// Opens or closes `id` (a click on its anchor).
pub fn toggle(ctx: &egui::Context, id: Id) {
    if is_open(ctx, id) {
        close(ctx, None);
    } else {
        open(ctx, id);
    }
}

#[derive(Clone, Debug, Default)]
struct State {
    query: String,
    /// The keyboard row (index into the navigable rows).
    act: Option<usize>,
    /// Set until the first frame has run (focus, the selected row).
    started: bool,
    /// Scroll the active row into view this frame.
    scroll: bool,
}

/// A row as drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Row {
    All,
    Group(usize),
    Item(usize),
}

fn font(ctx: &egui::Context, weight: Weight, size: f32) -> FontId {
    Density::of(ctx).font(weight, size)
}

fn text_width(ui: &Ui, text: &str, font: FontId) -> f32 {
    ui.painter()
        .layout_no_wrap(text.to_owned(), font, Color32::WHITE)
        .size()
        .x
}

/// The rows to draw for `query`.
fn rows(items: &[Item], query: &str, multi: bool) -> Vec<Row> {
    let q = query.trim().to_lowercase();
    let mut out = Vec::new();
    if multi && q.is_empty() && items.len() > 2 {
        out.push(Row::All);
    }
    let mut group: Option<&str> = None;
    for (i, item) in items.iter().enumerate() {
        if !q.is_empty() {
            let hay = format!(
                "{} {}",
                item.label.to_lowercase(),
                item.meta.as_deref().unwrap_or("").to_lowercase()
            );
            if !hay.contains(&q) {
                continue;
            }
        }
        if let Some(g) = item.group.as_deref() {
            if group != Some(g) {
                group = Some(g);
                out.push(Row::Group(i));
            }
        }
        out.push(Row::Item(i));
    }
    out
}

fn row_height(row: Row, first: bool) -> f32 {
    match row {
        Row::All => ROW + 1.0,
        Row::Group(_) => GROUP + if first { 0.0 } else { 2.0 },
        Row::Item(_) => ROW,
    }
}

/// The popover. Draw it right after its anchor while [`is_open`].
pub struct Popover<'a> {
    pub id: Id,
    pub anchor: Rect,
    /// Gets the keyboard back after a pick or Esc.
    pub anchor_id: Option<Id>,
    pub items: &'a [Item],
    pub options: Options,
}

impl Popover<'_> {
    /// Single-select: `selected` is marked and gets the keyboard row first.
    pub fn show_single(self, ctx: &egui::Context, selected: Option<usize>) -> Outcome {
        self.show(ctx, Selection::Single(selected))
    }

    /// Multi-select over `checked` (one flag per item).
    pub fn show_multi(self, ctx: &egui::Context, checked: &mut [bool]) -> Outcome {
        self.show(ctx, Selection::Multi(checked))
    }

    fn show(self, ctx: &egui::Context, mut sel: Selection<'_>) -> Outcome {
        if !is_open(ctx, self.id) {
            return Outcome::None;
        }
        let t = studio(ctx);
        let state_id = self.id.with("state");
        let mut st: State = ctx.data(|d| d.get_temp(state_id)).unwrap_or_default();
        let multi = matches!(sel, Selection::Multi(_));
        let filter = self
            .options
            .filter
            .unwrap_or(self.items.len() >= FILTER_FROM);
        let filter_id = self.id.with("filter");

        let shown = rows(self.items, &st.query, multi);
        let nav: Vec<usize> = shown
            .iter()
            .enumerate()
            .filter(|(_, r)| !matches!(r, Row::Group(_)))
            .map(|(k, _)| k)
            .collect();

        // The key that opened the list must not also pick in it.
        let keys = st.started;
        if !st.started {
            st.started = true;
            st.scroll = true;
            if let Selection::Single(Some(s)) = sel {
                st.act = nav.iter().position(|&k| shown[k] == Row::Item(s));
            }
            if let Some(anchor) = self.anchor_id {
                ctx.memory_mut(|m| m.surrender_focus(anchor));
            }
        }

        // Keys first, so the filter field and the page don't see them.
        let mut outcome = Outcome::None;
        let mut close_now: Option<bool> = None; // Some(refocus)
        let mut pick: Option<(usize, bool)> = None; // (nav index, only)
        let n = nav.len();
        ctx.input_mut(|i| {
            if !keys {
                return;
            }
            if i.consume_key(Modifiers::NONE, Key::Escape) {
                close_now = Some(true);
            }
            if i.consume_key(Modifiers::NONE, Key::Tab) || i.consume_key(Modifiers::SHIFT, Key::Tab)
            {
                close_now = Some(false);
            }
            if n > 0 {
                if i.consume_key(Modifiers::NONE, Key::ArrowDown) {
                    st.act = Some(st.act.map_or(0, |a| (a + 1) % n));
                    st.scroll = true;
                }
                if i.consume_key(Modifiers::NONE, Key::ArrowUp) {
                    st.act = Some(st.act.map_or(n - 1, |a| (a + n - 1) % n));
                    st.scroll = true;
                }
                // In the filter field Home/End move the text cursor.
                if !filter {
                    if i.consume_key(Modifiers::NONE, Key::Home) {
                        st.act = Some(0);
                        st.scroll = true;
                    }
                    if i.consume_key(Modifiers::NONE, Key::End) {
                        st.act = Some(n - 1);
                        st.scroll = true;
                    }
                }
                // ALT first: `consume_key` matches modifiers logically, so a
                // NONE pattern would also eat Alt+Enter as a plain toggle.
                for (mods, only) in [(Modifiers::ALT, true), (Modifiers::NONE, false)] {
                    if i.consume_key(mods, Key::Enter)
                        || (!filter && i.consume_key(mods, Key::Space))
                    {
                        pick = Some((st.act.unwrap_or(0).min(n - 1), only));
                    }
                }
            } else {
                i.consume_key(Modifiers::NONE, Key::Enter);
            }
        });

        // Size and place.
        let mut content_h = LIST_PAD * 2.0;
        for (k, r) in shown.iter().enumerate() {
            content_h += row_height(*r, k == 0);
        }
        if shown.is_empty() {
            content_h += ROW;
        }
        if filter {
            content_h += FILTER;
        }
        let screen = ctx.content_rect();
        let a = self.anchor;
        let below = screen.bottom() - a.bottom() - 12.0;
        let above = a.top() - screen.top() - 12.0;
        let down = below >= content_h.min(240.0) || below >= above;
        let max_h = MAX_HEIGHT.min(if down { below } else { above }).max(ROW);
        let h = content_h.min(max_h);

        let width = {
            let body = font(ctx, Weight::Regular, TEXT);
            let meta = FontId::new(11.0, FontFamily::Monospace);
            let mut w: f32 = 0.0;
            ctx.fonts_mut(|f| {
                for item in self.items {
                    let mut rw = 8.0 + 10.0;
                    if item.group.is_some() {
                        rw += 14.0;
                    }
                    if multi {
                        rw += 20.0;
                    }
                    if item.icon.is_some() {
                        rw += 19.0;
                    }
                    rw += f
                        .layout_no_wrap(item.label.clone(), body.clone(), Color32::WHITE)
                        .size()
                        .x;
                    if let Some(m) = &item.meta {
                        rw += 16.0
                            + f.layout_no_wrap(m.clone(), meta.clone(), Color32::WHITE)
                                .size()
                                .x;
                    }
                    w = w.max(rw);
                }
            });
            let min = self.options.width.unwrap_or(a.width()).max(MIN_WIDTH);
            w.ceil()
                .max(min)
                .min(MAX_WIDTH.max(min))
                .min(screen.width() - 16.0)
        };
        let x = a
            .left()
            .min(screen.right() - width - 8.0)
            .max(screen.left());
        let y = if down {
            a.bottom() + 4.0
        } else {
            a.top() - 4.0 - h
        };
        let pop = Rect::from_min_size(pos2(x, y), vec2(width, h));

        if self.options.dim {
            ctx.layer_painter(LayerId::new(Order::Foreground, self.id.with("scrim")))
                .rect_filled(screen, 0.0, Color32::from_black_alpha(140));
        }

        let mut hovered_nav: Option<usize> = None;
        let mut clicked: Option<(usize, bool)> = None;
        let area = egui::Area::new(self.id.with("pop"))
            .order(Order::Foreground)
            .fixed_pos(pop.min)
            .constrain(false)
            .show(ctx, |ui| {
                ui.set_min_size(pop.size());
                ui.set_max_size(pop.size());
                let p = ui.painter();
                p.rect_filled(pop, 0.0, t.pane);
                let mut top = pop.top();
                if filter {
                    draw_filter(ui, t, pop, filter_id, &mut st, &self.options.placeholder);
                    top += FILTER;
                }
                let list_rect = Rect::from_min_max(pos2(pop.left(), top), pop.max);
                let mut list_ui = ui.new_child(egui::UiBuilder::new().max_rect(list_rect));
                egui::ScrollArea::vertical()
                    .id_salt(self.id.with("scroll"))
                    .max_height(list_rect.height())
                    .auto_shrink([false, false])
                    .show(&mut list_ui, |ui| {
                        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                        ui.add_space(LIST_PAD);
                        if shown.is_empty() {
                            let (r, _) = ui.allocate_exact_size(
                                vec2(ui.available_width(), ROW),
                                Sense::hover(),
                            );
                            ui.painter().text(
                                pos2(r.left() + 8.0, r.center().y),
                                Align2::LEFT_CENTER,
                                &self.options.empty,
                                font(ui.ctx(), Weight::Regular, TEXT),
                                t.muted,
                            );
                        }
                        for (k, row) in shown.iter().enumerate() {
                            let nav_ix = nav.iter().position(|&x| x == k);
                            let active = nav_ix.is_some() && nav_ix == st.act;
                            let resp = match *row {
                                Row::Group(i) => {
                                    draw_group(
                                        ui,
                                        t,
                                        self.items[i].group.as_deref().unwrap_or(""),
                                        k == 0,
                                    );
                                    None
                                }
                                Row::All => {
                                    let all = sel.all();
                                    Some(draw_row(
                                        ui,
                                        t,
                                        self.id.with(("all", k)),
                                        &Item::new("All"),
                                        RowLook {
                                            active,
                                            on: false,
                                            check: Some(all),
                                            leaf: false,
                                            all_row: true,
                                        },
                                    ))
                                }
                                Row::Item(i) => {
                                    let item = &self.items[i];
                                    let on = sel.is_on(i);
                                    Some(draw_row(
                                        ui,
                                        t,
                                        self.id.with(("row", i)),
                                        item,
                                        RowLook {
                                            active,
                                            on: on && !multi,
                                            check: multi.then_some(on),
                                            leaf: item.group.is_some(),
                                            all_row: false,
                                        },
                                    ))
                                }
                            };
                            if let (Some(resp), Some(ni)) = (resp, nav_ix) {
                                if active && st.scroll {
                                    ui.scroll_to_rect(resp.rect, None);
                                }
                                if resp.hovered()
                                    && ui.input(|i| i.pointer.delta() != egui::Vec2::ZERO)
                                {
                                    hovered_nav = Some(ni);
                                }
                                if resp.clicked() {
                                    clicked = Some((ni, ui.input(|i| i.modifiers.alt)));
                                }
                            }
                        }
                        ui.add_space(LIST_PAD);
                    });
                // The 1px line around the popover (`box-shadow: 0 0 0 1px`).
                ui.painter().rect_stroke(
                    pop,
                    0.0,
                    Stroke::new(1.0_f32, t.line),
                    StrokeKind::Outside,
                );
            });
        st.scroll = false;
        if let Some(h) = hovered_nav {
            st.act = Some(h);
        }
        if let Some((ni, only)) = clicked {
            st.act = Some(ni);
            pick = Some((ni, only));
        }

        if let Some((ni, only)) = pick {
            match shown.get(nav[ni]) {
                Some(Row::Item(i)) => match &mut sel {
                    Selection::Single(_) => {
                        outcome = Outcome::Picked(*i);
                        close_now = Some(true);
                    }
                    Selection::Multi(c) => {
                        if only {
                            c.iter_mut().for_each(|x| *x = false);
                            c[*i] = true;
                        } else {
                            c[*i] = !c[*i];
                        }
                        outcome = Outcome::Changed;
                    }
                },
                Some(Row::All) => {
                    if let Selection::Multi(c) = &mut sel {
                        let all = c.iter().all(|x| *x);
                        c.iter_mut().for_each(|x| *x = !all);
                        outcome = Outcome::Changed;
                    }
                }
                _ => {}
            }
        }

        // A press outside the popover and its anchor closes it.
        let pop_rect = area.response.rect.union(pop);
        let outside = ctx.input(|i| {
            i.pointer.any_pressed()
                && i.pointer
                    .interact_pos()
                    .is_some_and(|p| !pop_rect.contains(p) && !a.contains(p))
        });
        if outside && close_now.is_none() {
            close_now = Some(false);
        }

        if let Some(refocus) = close_now {
            close(ctx, if refocus { self.anchor_id } else { None });
            ctx.data_mut(|d| d.remove_temp::<State>(state_id));
        } else {
            if filter {
                ctx.memory_mut(|m| m.request_focus(filter_id));
            }
            ctx.data_mut(|d| d.insert_temp(state_id, st));
        }
        outcome
    }
}

enum Selection<'a> {
    Single(Option<usize>),
    Multi(&'a mut [bool]),
}

impl Selection<'_> {
    fn is_on(&self, i: usize) -> bool {
        match self {
            Selection::Single(s) => *s == Some(i),
            Selection::Multi(c) => c.get(i).copied().unwrap_or(false),
        }
    }
    fn all(&self) -> bool {
        match self {
            Selection::Single(_) => false,
            Selection::Multi(c) => c.iter().all(|x| *x),
        }
    }
}

fn draw_filter(ui: &mut Ui, t: &Studio, pop: Rect, id: Id, st: &mut State, placeholder: &str) {
    let strip = Rect::from_min_size(pop.min, vec2(pop.width(), FILTER));
    let field = Rect::from_min_max(
        pos2(strip.left() + 6.0, strip.top() + 6.0),
        pos2(strip.right() - 6.0, strip.top() + 30.0),
    );
    let focused = ui.memory(|m| m.has_focus(id));
    let p = ui.painter();
    p.rect_filled(field, 0.0, t.well);
    p.rect_stroke(
        field,
        0.0,
        Stroke::new(1.0_f32, if focused { t.blue } else { t.line }),
        StrokeKind::Inside,
    );
    p.hline(
        strip.x_range(),
        strip.bottom() - 0.5,
        Stroke::new(1.0_f32, t.line),
    );
    p.text(
        pos2(strip.left() + 18.0, field.center().y),
        Align2::CENTER_CENTER,
        egui_phosphor::regular::MAGNIFYING_GLASS,
        font(ui.ctx(), Weight::Regular, TEXT),
        t.muted,
    );
    let text_rect = Rect::from_min_max(pos2(field.left() + 22.0, field.top()), field.max);
    let before = st.query.clone();
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(text_rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    child.add(
        egui::TextEdit::singleline(&mut st.query)
            .id(id)
            .frame(egui::Frame::NONE)
            .margin(egui::Margin::symmetric(0, 0))
            .hint_text(egui::RichText::new(placeholder).color(t.muted))
            .text_color(t.strong)
            .font(font(ui.ctx(), Weight::Regular, TEXT))
            .desired_width(text_rect.width() - 6.0),
    );
    if st.query != before {
        st.act = if st.query.trim().is_empty() {
            None
        } else {
            Some(0)
        };
        st.scroll = true;
    }
}

fn draw_group(ui: &mut Ui, t: &Studio, name: &str, first: bool) {
    if !first {
        ui.add_space(2.0);
    }
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), GROUP), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 0.0, t.head);
    p.hline(
        rect.x_range(),
        rect.top() + 0.5,
        Stroke::new(1.0_f32, t.line),
    );
    p.text(
        pos2(rect.left() + 14.5, rect.center().y),
        Align2::CENTER_CENTER,
        egui_phosphor::fill::FOLDER,
        FontId::new(13.0, FontFamily::Name(PHOSPHOR_FILL_FAMILY.into())),
        t.muted,
    );
    crate::app::components::studio::paint_truncated(
        ui,
        pos2(rect.left() + 27.0, rect.center().y),
        name,
        font(ui.ctx(), Weight::Medium, TEXT),
        t.strong,
        rect.width() - 35.0,
    );
}

struct RowLook {
    active: bool,
    /// Single-select: the current value.
    on: bool,
    /// Multi-select: the check.
    check: Option<bool>,
    leaf: bool,
    all_row: bool,
}

fn draw_row(ui: &mut Ui, t: &Studio, id: Id, item: &Item, look: RowLook) -> Response {
    let (rect, _) = ui.allocate_exact_size(
        vec2(
            ui.available_width(),
            ROW + if look.all_row { 1.0 } else { 0.0 },
        ),
        Sense::hover(),
    );
    let resp = ui.interact(rect, id, Sense::click());
    let row = Rect::from_min_size(rect.min, vec2(rect.width(), ROW));
    let p = ui.painter();
    let mut text_color = t.text;
    if look.on {
        p.rect_filled(row, 0.0, t.blue_dim);
        p.rect_filled(Rect::from_min_size(row.min, vec2(2.0, ROW)), 0.0, t.blue);
        text_color = t.strong;
    } else if look.active {
        p.rect_filled(row, 0.0, t.head);
        text_color = t.strong;
    }
    if look.all_row {
        p.hline(
            rect.x_range(),
            rect.bottom() - 0.5,
            Stroke::new(1.0_f32, t.hair),
        );
    }
    let cy = row.center().y;
    let mut x = row.left() + if look.leaf { 22.0 } else { 8.0 };
    if let Some(on) = look.check {
        let b = Rect::from_center_size(pos2(x + 7.0, cy), vec2(14.0, 14.0));
        if on {
            p.rect_filled(b, 0.0, t.blue);
            p.text(
                b.center(),
                Align2::CENTER_CENTER,
                egui_phosphor::regular::CHECK,
                FontId::new(10.0, FontFamily::Proportional),
                Color32::WHITE,
            );
        } else {
            p.rect_filled(b, 0.0, t.well);
            p.rect_stroke(b, 0.0, Stroke::new(1.0_f32, t.line), StrokeKind::Inside);
        }
        x += 20.0;
    }
    if let Some((glyph, color)) = item.icon {
        p.text(
            pos2(x + 6.5, cy),
            Align2::CENTER_CENTER,
            glyph,
            FontId::new(
                13.0,
                if item.icon_fill {
                    FontFamily::Name(PHOSPHOR_FILL_FAMILY.into())
                } else {
                    FontFamily::Proportional
                },
            ),
            color,
        );
        x += 19.0;
    }
    let mut right = row.right() - 10.0;
    if let Some(meta) = &item.meta {
        let f = FontId::new(11.0, FontFamily::Monospace);
        let w = text_width(ui, meta, f.clone());
        ui.painter()
            .text(pos2(right, cy), Align2::RIGHT_CENTER, meta, f, t.muted);
        right -= w + 16.0;
    }
    let color = if item.none { t.muted } else { text_color };
    let cut = crate::app::components::studio::paint_truncated(
        ui,
        pos2(x, cy),
        &item.label,
        font(ui.ctx(), Weight::Regular, TEXT),
        color,
        right - x,
    );
    let resp = if cut {
        resp.on_hover_text(&item.label)
    } else {
        resp
    };
    resp.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::SelectableLabel,
            true,
            look.on || look.check == Some(true),
            &item.label,
        )
    });
    resp
}

/// The anchor button (`.ddbtn`): a 24px well with a 1px line, the text and
/// a chevron; blue line while open. `width` fixes the width, else it fits
/// the text. Enter, Space or Alt+Down open it from the keyboard.
pub fn button(
    ui: &mut Ui,
    id: Id,
    icon: Option<&'static str>,
    text: &str,
    width: Option<f32>,
) -> Response {
    let t = studio(ui.ctx());
    let f = font(ui.ctx(), Weight::Regular, TEXT);
    let icon_w = if icon.is_some() { 18.0 } else { 0.0 };
    let w = width.unwrap_or_else(|| 8.0 + icon_w + text_width(ui, text, f.clone()) + 8.0 + 16.0);
    let (rect, _) = ui.allocate_exact_size(vec2(w, 24.0), Sense::hover());
    let resp = ui.interact(rect, id, Sense::click());
    let open = is_open(ui.ctx(), id.with("dd"));
    let p = ui.painter();
    p.rect_filled(rect, 0.0, if resp.hovered() { t.ctl } else { t.well });
    let line = if open || resp.has_focus() {
        t.blue
    } else {
        t.line
    };
    p.rect_stroke(rect, 0.0, Stroke::new(1.0_f32, line), StrokeKind::Inside);
    let cy = rect.center().y;
    let mut x = rect.left() + 8.0;
    if let Some(icon) = icon {
        p.text(
            pos2(x + 6.0, cy),
            Align2::CENTER_CENTER,
            icon,
            FontId::new(13.0, FontFamily::Proportional),
            t.text,
        );
        x += icon_w;
    }
    p.text(
        pos2(rect.right() - 11.0, cy),
        Align2::CENTER_CENTER,
        egui_phosphor::regular::CARET_DOWN,
        FontId::new(10.0, FontFamily::Proportional),
        t.muted,
    );
    let cut = crate::app::components::studio::paint_truncated(
        ui,
        pos2(x, cy),
        text,
        f,
        t.strong,
        rect.right() - 22.0 - x,
    );
    let resp = if cut { resp.on_hover_text(text) } else { resp };
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::ComboBox, true, text));
    resp
}

/// Opens the list on a click, or on Alt+Down while the anchor has focus.
fn anchor_toggles(ui: &Ui, resp: &Response, dd: Id) {
    let alt_down =
        resp.has_focus() && ui.input_mut(|i| i.consume_key(Modifiers::ALT, Key::ArrowDown));
    if resp.clicked() || alt_down {
        toggle(ui.ctx(), dd);
    }
}

/// A single-select dropdown: the anchor button showing `selected`'s label
/// (or `placeholder`) and its list. Returns the picked option.
pub fn select(
    ui: &mut Ui,
    id_salt: impl Hash,
    list: &[Item],
    selected: Option<usize>,
    placeholder: &str,
    width: Option<f32>,
) -> Option<usize> {
    let id = ui.make_persistent_id(id_salt);
    let text = selected
        .and_then(|s| list.get(s))
        .map_or(placeholder, |i| i.label.as_str());
    let resp = button(ui, id, None, text, width);
    let dd = id.with("dd");
    anchor_toggles(ui, &resp, dd);
    match (Popover {
        id: dd,
        anchor: resp.rect,
        anchor_id: Some(id),
        items: list,
        options: Options {
            width,
            ..Options::default()
        },
    })
    .show_single(ui.ctx(), selected)
    {
        Outcome::Picked(i) => Some(i),
        _ => None,
    }
}

/// [`select`] over a value: sets `value` to the picked option's value.
/// Returns whether it changed.
pub fn select_value<T: PartialEq + Clone>(
    ui: &mut Ui,
    id_salt: impl Hash,
    value: &mut T,
    options: &[(T, &str)],
    width: Option<f32>,
) -> bool {
    let list = items(options.iter().map(|(_, l)| *l));
    let cur = options.iter().position(|(v, _)| v == value);
    match select(ui, id_salt, &list, cur, "", width) {
        Some(i) if options[i].0 != *value => {
            *value = options[i].0.clone();
            true
        }
        _ => false,
    }
}

/// An action dropdown ("Add scene"): a button that opens a list and
/// returns the picked option. The button shows `label`, not a value.
pub fn action(
    ui: &mut Ui,
    id_salt: impl Hash,
    label: &str,
    list: &[Item],
    options: Options,
) -> Option<usize> {
    let id = ui.make_persistent_id(id_salt);
    let resp = button(ui, id, None, label, options.width);
    let dd = id.with("dd");
    anchor_toggles(ui, &resp, dd);
    match (Popover {
        id: dd,
        anchor: resp.rect,
        anchor_id: Some(id),
        items: list,
        options,
    })
    .show_single(ui.ctx(), None)
    {
        Outcome::Picked(i) => Some(i),
        _ => None,
    }
}

/// A multi-select dropdown over `checked`; `text` is the button label
/// (for example "All levels"). Returns whether the checks changed.
pub fn multi_select(
    ui: &mut Ui,
    id_salt: impl Hash,
    icon: Option<&'static str>,
    text: &str,
    list: &[Item],
    checked: &mut [bool],
    width: Option<f32>,
) -> bool {
    let id = ui.make_persistent_id(id_salt);
    let resp = button(ui, id, icon, text, width);
    let dd = id.with("dd");
    anchor_toggles(ui, &resp, dd);
    (Popover {
        id: dd,
        anchor: resp.rect,
        anchor_id: Some(id),
        items: list,
        options: Options {
            width,
            ..Options::default()
        },
    })
    .show_multi(ui.ctx(), checked)
        == Outcome::Changed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list() -> Vec<Item> {
        vec![
            Item::new("WAIT WALL").group("BAR-WAIT").meta("2"),
            Item::new("BAR HEX").group("BAR-WAIT").meta("3"),
            Item::new("FOG").group("FOG").meta("1"),
        ]
    }

    #[test]
    fn groups_draw_one_band_each() {
        let r = rows(&list(), "", false);
        assert_eq!(
            r,
            vec![
                Row::Group(0),
                Row::Item(0),
                Row::Item(1),
                Row::Group(2),
                Row::Item(2)
            ]
        );
    }

    #[test]
    fn filter_matches_label_and_meta_and_drops_empty_groups() {
        assert_eq!(
            rows(&list(), "fog", false),
            vec![Row::Group(2), Row::Item(2)]
        );
        assert_eq!(rows(&list(), "3", false), vec![Row::Group(1), Row::Item(1)]);
        assert!(rows(&list(), "zzz", false).is_empty());
    }

    #[test]
    fn multi_gets_an_all_row_only_unfiltered_and_from_three() {
        assert_eq!(rows(&list(), "", true)[0], Row::All);
        assert_ne!(rows(&list(), "f", true)[0], Row::All);
        assert_ne!(rows(&list()[..2], "", true)[0], Row::All);
    }
}
