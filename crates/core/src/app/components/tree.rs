//! Reusable collapsible tree (folders and leaves) for touch and desktop layouts.
//!
//! The widget is data-model agnostic: callers describe their hierarchy as a flat
//! list of [`TreeItem`]s (key, parent folder, label) and get back at most one
//! [`TreeAction`] per frame. Folder keys (`F`) and leaf keys (`L`) are separate
//! types, so a `u16` folder id and a `u8` group id can never be confused.
//!
//! Mapping a folder model such as `folders: BTreeMap<u16, Folder { name, parent, .. }>`
//! plus items with `folder: Option<u16>` is one `map` per collection:
//!
//! ```ignore
//! let items: Vec<TreeItem<u16, u8>> = folders
//!     .iter()
//!     .map(|(id, f)| TreeItem::folder(*id, f.parent, &f.name))
//!     .chain(groups.iter().map(|(id, g)| {
//!         TreeItem::leaf(*id, g.folder, &g.name)
//!             .with_detail(format!("{} FX | #{id}", g.fixtures.len()))
//!     }))
//!     .collect();
//!
//! match Tree::new("programmer_groups", &items)
//!     .selected(selected_group.map(TreeKey::Leaf))
//!     .show(ui)
//! {
//!     Some(TreeAction::SelectLeaf(group_id)) => { /* select one group */ }
//!     Some(TreeAction::TapFolder(folder_id)) => { /* e.g. SelectFolder(folder_id) */ }
//!     Some(TreeAction::MoveRequested(_)) | None => {}
//! }
//! ```
//!
//! Bad input never hangs or hides items: a parent that does not exist (orphan)
//! and folders whose parent chain loops (cycle) are shown as roots, and
//! duplicate keys keep their first occurrence.

use std::{
    collections::{btree_map::Entry, BTreeMap, BTreeSet},
    hash::Hash,
};

use egui::{Color32, Context, Id, Rect, Sense, Ui};

use crate::app::components::{self, ButtonColor, ButtonSize};

/// Default row height, matching the 50 px group rows in the Programmer.
pub const TREE_ROW_HEIGHT: f32 = 50.0;
/// Default indentation per nesting level.
pub const TREE_INDENT: f32 = 16.0;

/// Width of the expand/collapse hit area at the start of every row.
const TOGGLE_WIDTH: f32 = 40.0;
/// Width of the optional "Move…" button at the end of every row.
const MOVE_WIDTH: f32 = 64.0;
/// The row body never gets narrower than this; deeper levels stop indenting instead.
const MIN_BODY_WIDTH: f32 = 96.0;
const ROW_GAP: f32 = 2.0;
const ITEM_GAP: f32 = 4.0;

/// Identifies a row: either a folder or a leaf.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TreeKey<F, L> {
    Folder(F),
    Leaf(L),
}

/// One input node. Leaves and folders both point at their parent folder.
#[derive(Clone, Debug, PartialEq)]
pub struct TreeItem<F, L> {
    pub key: TreeKey<F, L>,
    /// Parent folder; `None`, or a folder that is not in the list, means root.
    pub parent: Option<F>,
    pub label: String,
    /// Optional second line (e.g. `"4 FX | #3"`). Folders default to their child count.
    pub detail: Option<String>,
}

impl<F, L> TreeItem<F, L> {
    pub fn folder(id: F, parent: Option<F>, label: impl Into<String>) -> Self {
        Self {
            key: TreeKey::Folder(id),
            parent,
            label: label.into(),
            detail: None,
        }
    }

    pub fn leaf(id: L, parent: Option<F>, label: impl Into<String>) -> Self {
        Self {
            key: TreeKey::Leaf(id),
            parent,
            label: label.into(),
            detail: None,
        }
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}

/// What the user did with the tree this frame. Toggling a folder is handled
/// internally and is not reported.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TreeAction<F, L> {
    /// A leaf row was tapped.
    SelectLeaf(L),
    /// A folder row body (not its expand/collapse toggle) was tapped.
    TapFolder(F),
    /// The row's "Move…" button was tapped (only with [`Tree::with_move_button`]).
    MoveRequested(TreeKey<F, L>),
}

/// Session-lived collapse state, kept in egui temp memory under [`tree_id`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TreeState<F> {
    collapsed: BTreeSet<F>,
}

impl<F> Default for TreeState<F> {
    fn default() -> Self {
        Self {
            collapsed: BTreeSet::new(),
        }
    }
}

impl<F: Copy + Ord + Send + Sync + 'static> TreeState<F> {
    pub fn load(ctx: &Context, id: Id) -> Self {
        ctx.data(|d| d.get_temp::<Self>(id)).unwrap_or_default()
    }

    pub fn store(self, ctx: &Context, id: Id) {
        ctx.data_mut(|d| d.insert_temp(id, self));
    }

    pub fn collapsed(&self) -> &BTreeSet<F> {
        &self.collapsed
    }

    pub fn is_collapsed(&self, folder: F) -> bool {
        self.collapsed.contains(&folder)
    }

    pub fn set_collapsed(&mut self, folder: F, collapsed: bool) {
        if collapsed {
            self.collapsed.insert(folder);
        } else {
            self.collapsed.remove(&folder);
        }
    }

    pub fn toggle(&mut self, folder: F) {
        let collapsed = self.is_collapsed(folder);
        self.set_collapsed(folder, !collapsed);
    }
}

/// The egui memory id a tree with this salt stores its [`TreeState`] under.
/// It is global (not tied to the parent `Ui`), so the same salt shares
/// collapse state between pages and dialogs.
pub fn tree_id(id_salt: impl Hash) -> Id {
    Id::new(("blaulicht_tree", id_salt))
}

//
// Pure logic.
//

/// One visible row after flattening.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TreeRow {
    /// Index into the input slice.
    pub item: usize,
    /// Nesting depth; roots are 0.
    pub depth: usize,
    /// Number of direct children (always 0 for leaves).
    pub child_count: usize,
    /// Whether this folder is collapsed (always false for leaves).
    pub collapsed: bool,
}

/// Effective parent index for every item, or `None` for roots and skipped items.
/// The second vector marks duplicate keys, which are dropped.
///
/// - A parent that is missing, or names a leaf, makes the item a root (orphan).
/// - Every folder on a parent cycle becomes a root; folders hanging off a cycle
///   keep their parent. The result is a forest, so walking it terminates.
fn resolve_parents<F: Copy + Ord, L: Copy + Ord>(
    items: &[TreeItem<F, L>],
) -> (Vec<Option<usize>>, Vec<bool>) {
    let mut skip = vec![false; items.len()];
    let mut folder_index: BTreeMap<F, usize> = BTreeMap::new();
    let mut seen_leaves: BTreeSet<L> = BTreeSet::new();
    for (idx, item) in items.iter().enumerate() {
        let first = match item.key {
            TreeKey::Folder(f) => match folder_index.entry(f) {
                Entry::Vacant(e) => {
                    e.insert(idx);
                    true
                }
                Entry::Occupied(_) => false,
            },
            TreeKey::Leaf(l) => seen_leaves.insert(l),
        };
        skip[idx] = !first;
    }

    let mut parent: Vec<Option<usize>> = items
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            if skip[idx] {
                return None;
            }
            item.parent.and_then(|p| folder_index.get(&p).copied())
        })
        .collect();

    // Break cycles among folders (leaves cannot be parents, so they never close one).
    #[derive(Clone, Copy, PartialEq)]
    enum Mark {
        New,
        OnPath,
        Done,
    }
    let mut mark = vec![Mark::New; items.len()];
    let mut path: Vec<usize> = Vec::new();
    for &start in folder_index.values() {
        path.clear();
        let mut cur = Some(start);
        while let Some(node) = cur {
            match mark[node] {
                Mark::Done => break,
                Mark::OnPath => {
                    // `node` closes a cycle: everything on the path from `node` on is in it.
                    let pos = path.iter().position(|&n| n == node).unwrap_or(0);
                    for &member in &path[pos..] {
                        parent[member] = None;
                    }
                    break;
                }
                Mark::New => {
                    mark[node] = Mark::OnPath;
                    path.push(node);
                    cur = parent[node];
                }
            }
        }
        for &node in &path {
            mark[node] = Mark::Done;
        }
    }

    (parent, skip)
}

/// Flatten `items` into the rows that are visible with the given collapsed folders,
/// in display order (depth-first). Under each parent, folders come first, then
/// leaves, each in input order. Never loops, whatever the input.
pub fn visible_rows<F: Copy + Ord, L: Copy + Ord>(
    items: &[TreeItem<F, L>],
    collapsed: &BTreeSet<F>,
) -> Vec<TreeRow> {
    let (parent, skip) = resolve_parents(items);

    let mut roots: Vec<usize> = Vec::new();
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); items.len()];
    let is_folder = |idx: usize| matches!(items[idx].key, TreeKey::Folder(_));
    for pass_folders in [true, false] {
        for idx in 0..items.len() {
            if skip[idx] || is_folder(idx) != pass_folders {
                continue;
            }
            match parent[idx] {
                Some(p) => children[p].push(idx),
                None => roots.push(idx),
            }
        }
    }

    let mut rows = Vec::with_capacity(items.len());
    // Explicit stack instead of recursion: deep chains cannot overflow.
    let mut stack: Vec<(usize, usize)> = roots.iter().rev().map(|&idx| (idx, 0)).collect();
    while let Some((idx, depth)) = stack.pop() {
        let is_collapsed = match items[idx].key {
            TreeKey::Folder(f) => collapsed.contains(&f),
            TreeKey::Leaf(_) => false,
        };
        rows.push(TreeRow {
            item: idx,
            depth,
            child_count: children[idx].len(),
            collapsed: is_collapsed,
        });
        if !is_collapsed {
            stack.extend(children[idx].iter().rev().map(|&c| (c, depth + 1)));
        }
    }
    rows
}

/// Indentation in pixels for a row at `depth`, shrunk so the row body keeps at
/// least [`MIN_BODY_WIDTH`] when `reserved` pixels (toggle, move button) are taken.
pub fn indent_width(depth: usize, step: f32, available: f32, reserved: f32) -> f32 {
    let wanted = depth as f32 * step;
    let max = (available - reserved - MIN_BODY_WIDTH).max(0.0);
    wanted.min(max)
}

//
// Widget.
//

/// Collapsible tree. Build with [`Tree::new`], then call [`Tree::show`]. Rows are
/// laid out top to bottom at the available width; wrap it in a `ScrollArea` if the
/// list can be long.
pub struct Tree<'a, F, L> {
    id: Id,
    items: &'a [TreeItem<F, L>],
    selected: Option<TreeKey<F, L>>,
    move_button: bool,
    row_height: f32,
    indent: f32,
}

impl<'a, F, L> Tree<'a, F, L>
where
    F: Copy + Ord + Send + Sync + 'static,
    L: Copy + Ord,
{
    /// `id_salt` names the tree's collapse state (see [`tree_id`]).
    pub fn new(id_salt: impl Hash, items: &'a [TreeItem<F, L>]) -> Self {
        Self {
            id: tree_id(id_salt),
            items,
            selected: None,
            move_button: false,
            row_height: TREE_ROW_HEIGHT,
            indent: TREE_INDENT,
        }
    }

    /// Highlight one row (usually the selected leaf).
    pub fn selected(mut self, key: Option<TreeKey<F, L>>) -> Self {
        self.selected = key;
        self
    }

    /// Show a "Move…" button on every row, reported as [`TreeAction::MoveRequested`].
    pub fn with_move_button(mut self, enabled: bool) -> Self {
        self.move_button = enabled;
        self
    }

    pub fn row_height(mut self, height: f32) -> Self {
        self.row_height = height;
        self
    }

    pub fn indent(mut self, step: f32) -> Self {
        self.indent = step;
        self
    }

    /// Draws the rows top to bottom (also inside a horizontal parent layout) and
    /// returns what the user did this frame, if anything.
    pub fn show(self, ui: &mut Ui) -> Option<TreeAction<F, L>> {
        ui.vertical(|ui| self.show_rows(ui)).inner
    }

    fn show_rows(self, ui: &mut Ui) -> Option<TreeAction<F, L>> {
        let mut state = TreeState::<F>::load(ui.ctx(), self.id);
        let rows = visible_rows(self.items, &state.collapsed);

        let full_width = ui.available_width();
        let reserved = TOGGLE_WIDTH
            + ITEM_GAP
            + if self.move_button {
                MOVE_WIDTH + ITEM_GAP
            } else {
                0.0
            };

        let mut action = None;
        let mut toggled = None;

        for row in &rows {
            let item = &self.items[row.item];
            let indent = indent_width(row.depth, self.indent, full_width, reserved);
            let body_width = (full_width - indent - reserved).max(MIN_BODY_WIDTH.min(full_width));
            let is_selected = self.selected == Some(item.key);

            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let h = self.row_height;

                if indent > 0.0 {
                    let (rect, _) = ui.allocate_exact_size(egui::vec2(indent, h), Sense::hover());
                    paint_guides(ui, rect, row.depth);
                }

                // Expand/collapse toggle (empty space for leaves, so labels line up).
                let (toggle_rect, toggle) = ui.allocate_exact_size(
                    egui::vec2(TOGGLE_WIDTH, h),
                    match item.key {
                        TreeKey::Folder(_) => Sense::click(),
                        TreeKey::Leaf(_) => Sense::hover(),
                    },
                );
                if let TreeKey::Folder(f) = item.key {
                    paint_chevron(ui, toggle_rect, row.collapsed, row.child_count > 0, &toggle);
                    if toggle.clicked() {
                        toggled = Some(f);
                    }
                }
                ui.add_space(ITEM_GAP);

                // Row body.
                let detail = match (&item.detail, item.key) {
                    (Some(d), _) => Some(d.clone()),
                    (None, TreeKey::Folder(_)) => Some(match row.child_count {
                        1 => "1 item".to_string(),
                        n => format!("{n} items"),
                    }),
                    (None, TreeKey::Leaf(_)) => None,
                };
                let is_folder = matches!(item.key, TreeKey::Folder(_));
                let body_clicked = components::clickable(
                    ui,
                    is_selected,
                    ButtonColor::Blue.into(),
                    ButtonSize::Custom(body_width, h, 16.0),
                    |ui, rect, fg| paint_body(ui, rect, fg, is_folder, &item.label, detail),
                );
                if body_clicked {
                    action = Some(match item.key {
                        TreeKey::Folder(f) => TreeAction::TapFolder(f),
                        TreeKey::Leaf(l) => TreeAction::SelectLeaf(l),
                    });
                }

                if self.move_button {
                    ui.add_space(ITEM_GAP);
                    if components::button(
                        ui,
                        false,
                        "Move…",
                        ButtonSize::Custom(MOVE_WIDTH, h, 13.0),
                    ) {
                        action = Some(TreeAction::MoveRequested(item.key));
                    }
                }
            });
            ui.add_space(ROW_GAP);
        }

        if let Some(f) = toggled {
            state.toggle(f);
            ui.ctx().request_repaint();
        }
        state.store(ui.ctx(), self.id);

        action
    }
}

fn paint_guides(ui: &Ui, rect: Rect, depth: usize) {
    if depth == 0 {
        return;
    }
    let step = rect.width() / depth as f32;
    let stroke = ui.visuals().widgets.noninteractive.bg_stroke;
    for level in 0..depth {
        let x = rect.left() + step * (level as f32 + 0.5);
        ui.painter().vline(x, rect.y_range(), stroke);
    }
}

fn paint_chevron(
    ui: &Ui,
    rect: Rect,
    collapsed: bool,
    has_children: bool,
    response: &egui::Response,
) {
    let visuals = ui.visuals();
    if response.is_pointer_button_down_on() || response.hovered() {
        ui.painter()
            .rect_filled(rect.shrink(2.0), 1.0, visuals.widgets.hovered.bg_fill);
    }
    let color = if has_children {
        visuals.strong_text_color()
    } else {
        visuals.weak_text_color()
    };
    let c = rect.center();
    let s = 7.0;
    let points = if collapsed {
        // Pointing right.
        vec![
            c + egui::vec2(-s * 0.6, -s),
            c + egui::vec2(s * 0.8, 0.0),
            c + egui::vec2(-s * 0.6, s),
        ]
    } else {
        // Pointing down.
        vec![
            c + egui::vec2(-s, -s * 0.6),
            c + egui::vec2(s, -s * 0.6),
            c + egui::vec2(0.0, s * 0.8),
        ]
    };
    if has_children {
        ui.painter().add(egui::Shape::convex_polygon(
            points,
            color,
            egui::Stroke::NONE,
        ));
    } else {
        ui.painter().add(egui::Shape::closed_line(
            points,
            egui::Stroke::new(1.0_f32, color),
        ));
    }
}

fn paint_folder_icon(ui: &Ui, origin: egui::Pos2, color: Color32) {
    // A 16x12 folder: tab on top-left, body below.
    let painter = ui.painter();
    let tab = Rect::from_min_size(origin, egui::vec2(7.0, 3.0));
    let body = Rect::from_min_size(origin + egui::vec2(0.0, 2.0), egui::vec2(16.0, 11.0));
    painter.rect_filled(tab, 1.0, color);
    painter.rect_stroke(
        body,
        1.0,
        egui::Stroke::new(1.5_f32, color),
        egui::StrokeKind::Inside,
    );
}

fn paint_body(
    ui: &Ui,
    rect: Rect,
    fg: Color32,
    is_folder: bool,
    label: &str,
    detail: Option<String>,
) {
    const PAD: f32 = 10.0;
    let mut x = rect.left() + PAD;
    if is_folder {
        paint_folder_icon(ui, egui::pos2(x, rect.center().y - 6.5), fg);
        x += 16.0 + 8.0;
    }
    let text_width = (rect.right() - PAD - x).max(0.0);

    let (label_y, label_align) = if detail.is_some() {
        (rect.top() + 8.0, egui::Align2::LEFT_TOP)
    } else {
        (rect.center().y, egui::Align2::LEFT_CENTER)
    };
    paint_truncated(
        ui,
        egui::pos2(x, label_y),
        label_align,
        label,
        16.0,
        text_width,
        fg,
    );
    if let Some(detail) = detail {
        paint_truncated(
            ui,
            egui::pos2(x, rect.bottom() - 8.0),
            egui::Align2::LEFT_BOTTOM,
            &detail,
            12.0,
            text_width,
            fg,
        );
    }
}

fn paint_truncated(
    ui: &Ui,
    pos: egui::Pos2,
    align: egui::Align2,
    text: &str,
    size: f32,
    max_width: f32,
    color: Color32,
) {
    let mut job = egui::text::LayoutJob::single_section(
        text.to_owned(),
        egui::text::TextFormat {
            font_id: egui::FontId::proportional(size),
            color,
            ..Default::default()
        },
    );
    job.wrap = egui::text::TextWrapping::truncate_at_width(max_width);
    let galley = ui.painter().layout_job(job);
    let min = align.anchor_size(pos, galley.size()).min;
    ui.painter().galley(min, galley, color);
}

#[cfg(test)]
mod tests {
    use super::*;

    type Item = TreeItem<u16, u8>;

    fn folder(id: u16, parent: Option<u16>) -> Item {
        TreeItem::folder(id, parent, format!("F{id}"))
    }

    fn leaf(id: u8, parent: Option<u16>) -> Item {
        TreeItem::leaf(id, parent, format!("L{id}"))
    }

    /// Rows as (key, depth) for compact assertions.
    fn layout(items: &[Item], collapsed: &[u16]) -> Vec<(TreeKey<u16, u8>, usize)> {
        let collapsed: BTreeSet<u16> = collapsed.iter().copied().collect();
        visible_rows(items, &collapsed)
            .into_iter()
            .map(|r| (items[r.item].key, r.depth))
            .collect()
    }

    use TreeKey::{Folder as F, Leaf as L};

    #[test]
    fn empty_input() {
        assert!(layout(&[], &[]).is_empty());
    }

    #[test]
    fn nesting_and_order_folders_first() {
        let items = vec![
            leaf(1, None),
            leaf(2, Some(10)),
            folder(10, None),
            folder(11, Some(10)),
            leaf(3, Some(11)),
            folder(12, None),
        ];
        assert_eq!(
            layout(&items, &[]),
            vec![
                (F(10), 0),
                (F(11), 1),
                (L(3), 2),
                (L(2), 1),
                (F(12), 0),
                (L(1), 0),
            ]
        );
    }

    #[test]
    fn child_counts_and_collapsed_flag() {
        let items = vec![
            folder(10, None),
            leaf(1, Some(10)),
            leaf(2, Some(10)),
            folder(11, None),
        ];
        let collapsed: BTreeSet<u16> = [10].into();
        let rows = visible_rows(&items, &collapsed);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].child_count, 2);
        assert!(rows[0].collapsed);
        assert_eq!(rows[1].child_count, 0);
        assert!(!rows[1].collapsed);
    }

    #[test]
    fn collapse_hides_all_descendants_only() {
        let items = vec![
            folder(10, None),
            folder(11, Some(10)),
            leaf(1, Some(11)),
            leaf(2, Some(10)),
            leaf(3, None),
        ];
        assert_eq!(layout(&items, &[10]), vec![(F(10), 0), (L(3), 0)]);
        assert_eq!(
            layout(&items, &[11]),
            vec![(F(10), 0), (F(11), 1), (L(2), 1), (L(3), 0)]
        );
        // Collapsing an unknown folder id is harmless.
        assert_eq!(layout(&items, &[99]).len(), 5);
    }

    #[test]
    fn orphans_become_roots() {
        let items = vec![folder(10, Some(77)), leaf(1, Some(42)), leaf(2, Some(10))];
        assert_eq!(layout(&items, &[]), vec![(F(10), 0), (L(2), 1), (L(1), 0)]);
    }

    #[test]
    fn self_parent_is_root() {
        let items = vec![folder(10, Some(10)), leaf(1, Some(10))];
        assert_eq!(layout(&items, &[]), vec![(F(10), 0), (L(1), 1)]);
    }

    #[test]
    fn cycle_members_become_roots_and_branches_stay() {
        // 10 -> 11 -> 12 -> 10 is a cycle; 13 hangs off 11; 14 is a normal root.
        let items = vec![
            folder(10, Some(12)),
            folder(11, Some(10)),
            folder(12, Some(11)),
            folder(13, Some(11)),
            folder(14, None),
            leaf(1, Some(13)),
        ];
        let rows = layout(&items, &[]);
        assert_eq!(
            rows,
            vec![
                (F(10), 0),
                (F(11), 0),
                (F(13), 1),
                (L(1), 2),
                (F(12), 0),
                (F(14), 0),
            ]
        );
    }

    #[test]
    fn two_cycles_and_chain_into_cycle() {
        // 20 <-> 21 cycle, 22 -> 21 (into the cycle), 30 <-> 31 cycle.
        let items = vec![
            folder(22, Some(21)),
            folder(20, Some(21)),
            folder(21, Some(20)),
            folder(30, Some(31)),
            folder(31, Some(30)),
        ];
        let rows = layout(&items, &[]);
        assert_eq!(rows.len(), 5, "every folder is shown exactly once");
        assert!(rows.contains(&(F(20), 0)));
        assert!(rows.contains(&(F(21), 0)));
        assert!(rows.contains(&(F(22), 1)));
        assert!(rows.contains(&(F(30), 0)));
        assert!(rows.contains(&(F(31), 0)));
    }

    #[test]
    fn duplicate_keys_keep_first() {
        let items = vec![
            folder(10, None),
            folder(10, Some(10)),
            leaf(1, Some(10)),
            leaf(1, None),
        ];
        let rows = visible_rows(&items, &BTreeSet::new());
        let indices: Vec<usize> = rows.iter().map(|r| r.item).collect();
        assert_eq!(indices, vec![0, 2]);
    }

    #[test]
    fn folder_and_leaf_with_same_number_are_distinct() {
        // Leaf 5 must not become the parent of anything; Folder 5 is.
        let items = vec![leaf(5, None), folder(5, None), leaf(6, Some(5))];
        assert_eq!(layout(&items, &[]), vec![(F(5), 0), (L(6), 1), (L(5), 0)]);
    }

    #[test]
    fn deep_chain_does_not_overflow() {
        let n: u16 = 20_000;
        let mut items: Vec<Item> = (0..n)
            .map(|i| folder(i, if i == 0 { None } else { Some(i - 1) }))
            .collect();
        items.push(leaf(1, Some(n - 1)));
        let rows = visible_rows(&items, &BTreeSet::new());
        assert_eq!(rows.len(), n as usize + 1);
        assert_eq!(rows.last().unwrap().depth, n as usize);
    }

    #[test]
    fn indent_shrinks_to_keep_body_width() {
        assert_eq!(indent_width(0, 16.0, 300.0, 44.0), 0.0);
        assert_eq!(indent_width(2, 16.0, 300.0, 44.0), 32.0);
        // 300 - 44 - 96 = 160 px max indent.
        assert_eq!(indent_width(50, 16.0, 300.0, 44.0), 160.0);
        // Too narrow for any indent.
        assert_eq!(indent_width(3, 16.0, 100.0, 44.0), 0.0);
    }

    #[test]
    fn state_toggle() {
        let mut state = TreeState::<u16>::default();
        assert!(!state.is_collapsed(3));
        state.toggle(3);
        assert!(state.is_collapsed(3));
        state.set_collapsed(3, false);
        assert!(state.collapsed().is_empty());
    }
}
