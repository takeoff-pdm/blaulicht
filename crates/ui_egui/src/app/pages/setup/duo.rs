//! The desktop master-detail layout of Setup › Groups, Scenes, Animations
//! and Palettes (`setup.html`, `.duo`): a 240px list pane on the left and
//! the detail pane on the right, both topped by a 40px bar
//! ([`studio::pane_header`]) so the bars line up across the columns. The
//! detail bar names the selection with a muted prefix ("Group:", "Scene:").
//!
//! Desktop only ([`Density::Compact`]); the kiosk keeps each page's touch
//! layout.
//!
//! [`Density::Compact`]: crate::app::ui_tokens::Density::Compact

use crate::app::components::{folder_details, studio, FolderDialog};
use crate::app::ui_tokens::{studio as studio_tokens, Density, Weight, TARGET_MIN, TARGET_SHOW};
use blaulicht_shared::folder::{FolderId, FolderKind};
use blaulicht_shared::EngineState;
use egui::{pos2, vec2, Align2, Id, Rect, RichText, Sense, Ui};

/// Width of the list pane (`setup.css` `.duo`).
const LIST_W: f32 = 240.0;
/// The 1px gutter between panes.
const GAP: f32 = 1.0;
/// Width of the label column in a detail form.
const LABEL_W: f32 = 86.0;

/// Splits the page into the list pane and the detail pane, edge to edge
/// with the gutter colour between them, like Setup › Fixtures.
pub(crate) fn duo(ui: &mut Ui) -> (Ui, Ui) {
    let t = studio_tokens(ui.ctx());
    let full = ui.max_rect().expand(8.0).intersect(ui.clip_rect());
    ui.allocate_rect(ui.max_rect(), Sense::hover());
    ui.painter().rect_filled(full, 0.0, t.gutter);
    let list_rect = Rect::from_min_size(full.min, vec2(LIST_W, full.height()));
    let detail_rect = Rect::from_min_max(pos2(list_rect.right() + GAP, full.top()), full.max);
    let mut pane = |rect: Rect, fill| {
        ui.painter().rect_filled(rect, 0.0, fill);
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(rect)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        child.set_clip_rect(rect);
        child.spacing_mut().item_spacing = vec2(0.0, 0.0);
        child
    };
    let list = pane(list_rect, t.list);
    let detail = pane(detail_rect, t.pane);
    (list, detail)
}

/// The bar buttons of a pane header: 28px, 2px apart.
pub(crate) fn bar_button(ui: &mut Ui, label: &str, pressed: bool) -> bool {
    ui.spacing_mut().item_spacing.x = 2.0;
    studio::mini_button(ui, label, pressed, TARGET_SHOW).clicked()
}

/// A padded area for a form or text below a header, `height` tall
/// (`None`: what is left of the pane).
pub(crate) fn body<R>(ui: &mut Ui, height: Option<f32>, add: impl FnOnce(&mut Ui) -> R) -> R {
    let avail = ui.available_rect_before_wrap();
    let h = height.unwrap_or(avail.height()).min(avail.height());
    let rect = Rect::from_min_size(avail.min, vec2(avail.width(), h));
    ui.allocate_rect(rect, Sense::hover());
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.shrink(8.0))
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    child.spacing_mut().item_spacing = vec2(6.0, ROW_GAP);
    // Text rows are 16px, not the 20px `interact_size` minimum.
    child.spacing_mut().interact_size.y = TEXT_ROW;
    add(&mut child)
}

/// The same area as [`body`] without padding, for an edge-to-edge
/// [`table`].
pub(crate) fn flush<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    let rect = ui.available_rect_before_wrap();
    ui.allocate_rect(rect, Sense::hover());
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    child.spacing_mut().item_spacing = vec2(0.0, 0.0);
    add(&mut child)
}

/// Vertical gap between form rows (`.frm` `gap: 6px`).
const ROW_GAP: f32 = 6.0;
/// Height of a form row with text (not a field).
const TEXT_ROW: f32 = 16.0;

/// Height of a form with a Name field and `rows - 1` text rows, as laid
/// out by [`body`], [`name_row`] and [`form_row`].
pub(crate) fn form_height(rows: usize) -> f32 {
    let rest = rows.saturating_sub(1) as f32;
    16.0 + TARGET_MIN + rest * (TEXT_ROW + ROW_GAP)
}

/// One form row: a muted label in the label column, then `add`.
pub(crate) fn form_row(ui: &mut Ui, label: &str, add: impl FnOnce(&mut Ui)) {
    let t = studio_tokens(ui.ctx());
    let f = Density::of(ui.ctx()).font(Weight::Regular, 12.0);
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(vec2(LABEL_W, TEXT_ROW), Sense::hover());
        ui.painter().text(
            pos2(r.left(), r.center().y),
            Align2::LEFT_CENTER,
            label,
            f,
            t.muted,
        );
        add(ui);
    });
}

/// Width of the term column of a read-only list (`setup.css` `.kv`).
const KV_W: f32 = 128.0;

/// One row of a read-only term list (`.kv`): a muted term in a 128px
/// column, then `add`, at least 16px tall.
pub(crate) fn kv_row(ui: &mut Ui, term: &str, add: impl FnOnce(&mut Ui)) {
    let t = studio_tokens(ui.ctx());
    let f = Density::of(ui.ctx()).font(Weight::Regular, 12.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        let (r, _) = ui.allocate_exact_size(vec2(KV_W, KV_ROW), Sense::hover());
        ui.painter().text(
            pos2(r.left(), r.center().y),
            Align2::LEFT_CENTER,
            term,
            f,
            t.muted,
        );
        add(ui);
    });
}

/// Gap between [`kv_row`]s (`.kv` `gap: 4px`).
pub(crate) const KV_GAP: f32 = 4.0;

/// A 24px button chip in a [`kv_row`] (`dd > button.mini`).
pub(crate) fn chip(ui: &mut Ui, label: &str) -> bool {
    studio::mini_button(ui, label, false, TARGET_MIN).clicked()
}

/// Row height of [`kv_row`].
pub(crate) const KV_ROW: f32 = TEXT_ROW;

/// Plain body text.
pub(crate) fn text(ui: &mut Ui, s: impl Into<String>) {
    let t = studio_tokens(ui.ctx());
    let f = Density::of(ui.ctx()).font(Weight::Regular, 12.0);
    ui.add(egui::Label::new(RichText::new(s).font(f).color(t.text)).wrap());
}

/// Muted help text.
pub(crate) fn note(ui: &mut Ui, s: impl Into<String>) {
    let t = studio_tokens(ui.ctx());
    let f = Density::of(ui.ctx()).font(Weight::Regular, 12.0);
    ui.add(egui::Label::new(RichText::new(s).font(f).color(t.muted)).wrap());
}

/// A form row "Name" with a text field that renames in place: the edit is
/// kept while the field has focus, Enter or leaving the field returns the
/// new name (trimmed, not empty, different), Escape puts the old one back.
/// `key` names the item, so switching items drops an unsaved edit.
pub(crate) fn name_row(ui: &mut Ui, key: Id, current: &str) -> Option<String> {
    let mut out = None;
    form_row(ui, "Name", |ui| {
        let id = key.with("name_field");
        let mut buf: String = ui
            .data(|d| d.get_temp::<String>(id))
            .unwrap_or_else(|| current.to_owned());
        let f = Density::of(ui.ctx()).font(Weight::Regular, 12.0);
        let response = ui.add_sized(
            vec2(ui.available_width(), TARGET_MIN),
            egui::TextEdit::singleline(&mut buf)
                .id(id.with("edit"))
                .font(f)
                .vertical_align(egui::Align::Center),
        );
        if response.lost_focus() {
            let escaped = ui.input(|i| i.key_pressed(egui::Key::Escape));
            let name = buf.trim();
            if !escaped && !name.is_empty() && name != current {
                out = Some(name.to_owned());
            }
            ui.data_mut(|d| d.remove::<String>(id));
        } else if response.has_focus() {
            ui.data_mut(|d| d.insert_temp(id, buf));
        } else {
            ui.data_mut(|d| d.remove::<String>(id));
        }
    });
    out
}

/// A lower pane: a 26px title bar over the rest of the pane.
pub(crate) fn lower_pane(ui: &mut Ui, title: &str) {
    studio::pane_header(ui, None, title, false, |_| {});
}

/// The detail pane of a selected folder: a "Folder:" bar with Rename and
/// Delete over [`folder_details`]. Returns the dialog a button opened.
pub(crate) fn folder_desktop_pane(
    ui: &mut Ui,
    state: &EngineState,
    kind: FolderKind,
    folder: FolderId,
) -> Option<FolderDialog> {
    let name = state
        .folders
        .get(&folder)
        .map_or_else(String::new, |f| f.name.clone());
    let (mut rename, mut delete) = (false, false);
    studio::pane_header(ui, Some("Folder:"), &name, true, |ui| {
        rename = bar_button(ui, "Rename", false);
        delete = bar_button(ui, "Delete", false);
    });
    body(ui, None, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        folder_details(ui, state, kind, folder);
    });
    if rename {
        Some(FolderDialog::rename(state, folder))
    } else if delete {
        Some(FolderDialog::delete(folder))
    } else {
        None
    }
}

/// How a cell of [`table`] is drawn.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Look {
    Text,
    Muted,
    /// Numbers: 12px mono in the strong colour (`.mono`).
    Mono,
}

/// A cell of [`table`]: its text and look.
pub(crate) type Cell = (String, Look);

/// Height of a [`table`] header row (`.rows th`).
const TH_H: f32 = 22.0;

/// The 12px mono font of numbers (`.mono`).
pub(crate) fn mono_font() -> egui::FontId {
    egui::FontId::new(12.0, egui::FontFamily::Monospace)
}

/// A read-only table (`.rows`), edge to edge in the pane: a 22px muted
/// header row, then 24px rows with no rules between them (13px text, 12px
/// muted and mono cells); cells have 8px
/// side padding. `cols` are the column starts as fractions of the width;
/// text is cut to its column.
pub(crate) fn table(ui: &mut Ui, heads: &[&str], cols: &[f32], rows: &[Vec<Cell>]) {
    let t = studio_tokens(ui.ctx());
    let f = Density::of(ui.ctx()).font(Weight::Regular, 12.0);
    let body = Density::of(ui.ctx()).font(Weight::Regular, 13.0);
    let width = ui.available_width();
    let line = |ui: &mut Ui, cells: &mut dyn Iterator<Item = (&str, Look)>, h: f32| {
        let (rect, _) = ui.allocate_exact_size(vec2(width, h), Sense::hover());
        let inner = rect.shrink2(vec2(8.0, 0.0));
        let w = inner.width();
        for (i, (s, look)) in cells.enumerate() {
            let x = inner.left() + cols[i] * w;
            let end = cols
                .get(i + 1)
                .map_or(inner.right(), |c| inner.left() + c * w)
                - 8.0;
            let (font, color) = match look {
                Look::Text => (body.clone(), t.text),
                Look::Muted => (f.clone(), t.muted),
                Look::Mono => (mono_font(), t.strong),
            };
            studio::paint_truncated(
                ui,
                pos2(x, rect.center().y),
                s,
                font,
                color,
                (end - x).max(0.0),
            );
        }
    };
    line(ui, &mut heads.iter().map(|h| (*h, Look::Muted)), TH_H);
    for row in rows {
        line(
            ui,
            &mut row.iter().map(|(s, l)| (s.as_str(), *l)),
            TARGET_MIN,
        );
    }
}

/// A header-less two-column `.rows` table, edge to edge in the pane: 24px
/// rows of 13px text with the value right-aligned in mono (`td.r.mono`), no
/// rules. Starts right under the pane header, like Setup › Scenes' Contents.
pub(crate) fn value_rows(ui: &mut Ui, rows: &[(&str, String)]) {
    let t = studio_tokens(ui.ctx());
    let body = Density::of(ui.ctx()).font(Weight::Regular, 13.0);
    let width = ui.available_width();
    for (label, value) in rows {
        let (rect, _) = ui.allocate_exact_size(vec2(width, TARGET_MIN), Sense::hover());
        let inner = rect.shrink2(vec2(8.0, 0.0));
        let painter = ui.painter();
        painter.text(
            pos2(inner.right(), rect.center().y),
            Align2::RIGHT_CENTER,
            value,
            mono_font(),
            t.strong,
        );
        studio::paint_truncated(
            ui,
            pos2(inner.left(), rect.center().y),
            label,
            body.clone(),
            t.text,
            (inner.width() - 48.0).max(0.0),
        );
    }
}

/// The fixture type as the studio shows it (`pretty` in `setup.html`),
/// falling back to the model name for types the mockup does not list.
pub(crate) fn type_label(model: &str) -> String {
    match model {
        "AdjMegaHexPar" => "ADJ Mega Hex Par",
        "DimmerSingle" => "Dimmer",
        "LiteCraftMiniParAT10" => "LiteCraft AT10",
        "MartinMac250E" => "Martin MAC 250E",
        "VaryTechVP1" => "Varytec VP1",
        "FogMachineSingle" => "Fog machine",
        other => other,
    }
    .to_owned()
}
