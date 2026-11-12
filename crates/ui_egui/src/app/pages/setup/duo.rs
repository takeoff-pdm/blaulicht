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
pub(super) fn duo(ui: &mut Ui) -> (Ui, Ui) {
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
pub(super) fn bar_button(ui: &mut Ui, label: &str, pressed: bool) -> bool {
    ui.spacing_mut().item_spacing.x = 2.0;
    studio::mini_button(ui, label, pressed, TARGET_SHOW).clicked()
}

/// A padded area for a form or text below a header, `height` tall
/// (`None`: what is left of the pane).
pub(super) fn body<R>(ui: &mut Ui, height: Option<f32>, add: impl FnOnce(&mut Ui) -> R) -> R {
    let avail = ui.available_rect_before_wrap();
    let h = height.unwrap_or(avail.height()).min(avail.height());
    let rect = Rect::from_min_size(avail.min, vec2(avail.width(), h));
    ui.allocate_rect(rect, Sense::hover());
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.shrink(8.0))
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    child.spacing_mut().item_spacing = vec2(6.0, 2.0);
    add(&mut child)
}

/// Height of a form with `rows` rows, as laid out by [`body`] and
/// [`form_row`].
pub(super) fn form_height(rows: usize) -> f32 {
    16.0 + rows as f32 * TARGET_MIN + rows.saturating_sub(1) as f32 * 2.0
}

/// One form row: a muted label in the label column, then `add`.
pub(super) fn form_row(ui: &mut Ui, label: &str, add: impl FnOnce(&mut Ui)) {
    let t = studio_tokens(ui.ctx());
    let f = Density::of(ui.ctx()).font(Weight::Regular, 12.0);
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(vec2(LABEL_W, TARGET_MIN), Sense::hover());
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

/// Plain body text.
pub(super) fn text(ui: &mut Ui, s: impl Into<String>) {
    let t = studio_tokens(ui.ctx());
    let f = Density::of(ui.ctx()).font(Weight::Regular, 12.0);
    ui.add(egui::Label::new(RichText::new(s).font(f).color(t.text)).wrap());
}

/// Muted help text.
pub(super) fn note(ui: &mut Ui, s: impl Into<String>) {
    let t = studio_tokens(ui.ctx());
    let f = Density::of(ui.ctx()).font(Weight::Regular, 12.0);
    ui.add(egui::Label::new(RichText::new(s).font(f).color(t.muted)).wrap());
}

/// A form row "Name" with a text field that renames in place: the edit is
/// kept while the field has focus, Enter or leaving the field returns the
/// new name (trimmed, not empty, different), Escape puts the old one back.
/// `key` names the item, so switching items drops an unsaved edit.
pub(super) fn name_row(ui: &mut Ui, key: Id, current: &str) -> Option<String> {
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
pub(super) fn lower_pane(ui: &mut Ui, title: &str) {
    studio::pane_header(ui, None, title, false, |_| {});
}

/// The detail pane of a selected folder: a "Folder:" bar with Rename and
/// Delete over [`folder_details`]. Returns the dialog a button opened.
pub(super) fn folder_desktop_pane(
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

/// A cell of [`table`]: its text and whether it is muted.
pub(super) type Cell = (String, bool);

/// A read-only table (`.rows`): a muted header row, then 24px rows with a
/// hairline under each. `cols` are the column starts as fractions of the
/// width; text is cut to its column.
pub(super) fn table(ui: &mut Ui, heads: &[&str], cols: &[f32], rows: &[Vec<Cell>]) {
    let t = studio_tokens(ui.ctx());
    let f = Density::of(ui.ctx()).font(Weight::Regular, 12.0);
    let width = ui.available_width();
    let line = |ui: &mut Ui, cells: &mut dyn Iterator<Item = (&str, bool)>, rule: bool| {
        let (rect, _) = ui.allocate_exact_size(vec2(width, TARGET_MIN), Sense::hover());
        for (i, (s, muted)) in cells.enumerate() {
            let x = rect.left() + cols[i] * width;
            let end = cols
                .get(i + 1)
                .map_or(rect.right(), |c| rect.left() + c * width)
                - 8.0;
            studio::paint_truncated(
                ui,
                pos2(x, rect.center().y),
                s,
                f.clone(),
                if muted { t.muted } else { t.text },
                (end - x).max(0.0),
            );
        }
        if rule {
            ui.painter().hline(
                rect.x_range(),
                rect.bottom() - 0.5,
                egui::Stroke::new(1.0_f32, t.line),
            );
        }
    };
    line(ui, &mut heads.iter().map(|h| (*h, true)), false);
    for row in rows {
        line(ui, &mut row.iter().map(|(s, m)| (s.as_str(), *m)), true);
    }
}
