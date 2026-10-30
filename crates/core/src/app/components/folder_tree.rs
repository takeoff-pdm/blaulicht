//! Folder trees for groups, scenes and palettes, built on the generic [`Tree`]
//! widget and the folder model in [`blaulicht_shared::folder`].
//!
//! - **Items**: [`folder_tree_items`] turns the folders of one kind into tree
//!   items in sibling order; [`group_tree_items`] adds the groups as leaves.
//!   Scenes and palettes add their leaves the same way.
//! - **Dialogs**: [`FolderDialog`] is the touch UI for one folder kind: create,
//!   rename and delete a folder, and "Move to…" for any item (a folder picker,
//!   no dragging). [`FolderDialog::show`] returns the event to send once the
//!   user confirms; the caller puts it on the event bus.
//! - **Events**: [`create_folder_event`], [`rename_folder_event`] and
//!   [`move_to_folder_event`] check the input the way the engine will, so the
//!   dialogs can show the engine's error text inline instead of sending an
//!   event that is rejected. [`folder_tap_event`] is the Programmer's
//!   "tap a folder" (select its groups, or deselect them when all are selected).
//!
//! There is no undo stack in the UI yet. When one is added, compute the undo
//! event with `EngineState::folder_rollback` before sending.

use std::{
    collections::{BTreeMap, BTreeSet},
    hash::Hash,
};

use blaulicht_shared::{
    folder::{self, Folder, FolderId, FolderItem, FolderKind},
    ControlEvent, EngineState,
};
use egui::{Color32, Context, RichText};

use crate::app::components::{
    self, ButtonSize, Dialog, TextInput, Tree, TreeAction, TreeItem, TreeKey,
};

/// Collapse-state salt of the Setup group tree (Setup › Groups), shared with
/// the Setup › Fixtures group picker so both show the same folders open.
pub const SETUP_GROUP_TREE: &str = "setup_groups";

/// Width of a narrow picker column (the Programmer's and Setup › Fixtures'
/// group trees). At 800x480 the Programmer has about 30 px more than the old
/// 90 px group tiles before its right column (masters, scene changes) is
/// pushed off screen.
pub const NARROW_TREE_WIDTH: f32 = 120.0;

/// A [`Tree`] for a [`NARROW_TREE_WIDTH`] column: a slimmer toggle (24 px), a
/// smaller minimum row body (80 px), 8 px indentation (clamped at 12 px, so
/// levels 1 and 2+ are visible) and the compact look (no folder icon, less
/// padding). Long names truncate with "…"; the detail line keeps the group id.
pub fn narrow_tree<'a, F, L>(id_salt: impl Hash, items: &'a [TreeItem<F, L>]) -> Tree<'a, F, L>
where
    F: Copy + Ord + Send + Sync + 'static,
    L: Copy + Ord,
{
    Tree::new(id_salt, items)
        .toggle_width(24.0)
        .min_body_width(80.0)
        .indent(8.0)
        .compact(true)
}

//
// Tree items.
//

/// The folders of `kind` as tree items, siblings in `(order, name)` order
/// ([`folder::sorted_folder_ids`]). Callers append their leaves.
pub fn folder_tree_items<L>(
    folders: &BTreeMap<FolderId, Folder>,
    kind: FolderKind,
) -> Vec<TreeItem<FolderId, L>> {
    folder::sorted_folder_ids(folders, kind)
        .into_iter()
        .map(|id| {
            let f = &folders[&id];
            TreeItem::folder(id, f.parent, f.name.clone())
        })
        .collect()
}

/// The group tree: every groups folder, then every group as a leaf (by id),
/// with the same detail line as the group tiles (`"4 FX | #3"`).
pub fn group_tree_items(state: &EngineState) -> Vec<TreeItem<FolderId, u8>> {
    let mut items = folder_tree_items(&state.folders, FolderKind::Groups);
    items.extend(state.groups.iter().map(|(id, group)| {
        TreeItem::leaf(*id, group.folder, group.name.clone())
            .with_detail(format!("{} FX | #{id}", group.fixtures.len()))
    }));
    items
}

/// Groups folders whose groups (recursively) are all selected, for
/// highlighting in the Programmer. Folders without groups are never included.
pub fn fully_selected_group_folders(state: &EngineState) -> Vec<FolderId> {
    state
        .folders
        .iter()
        .filter(|(_, f)| f.kind == FolderKind::Groups)
        .map(|(id, _)| *id)
        .filter(|id| {
            let groups = state.folder_group_ids(*id);
            !groups.is_empty() && groups.iter().all(|g| state.selection.group_ids.contains(g))
        })
        .collect()
}

//
// Events.
//

/// The Programmer's folder tap: `SelectFolder` adds every group under the
/// folder to the selection. If all of them are selected already, the tap
/// deselects them instead (one `DeSelectGroup` each, in a `Transaction`),
/// like tapping a selected group. `None` for a folder without groups.
pub fn folder_tap_event(state: &EngineState, id: FolderId) -> Option<ControlEvent> {
    let groups = state.folder_group_ids(id);
    if groups.is_empty() {
        return None;
    }
    if groups.iter().all(|g| state.selection.group_ids.contains(g)) {
        Some(ControlEvent::Transaction(
            groups
                .into_iter()
                .map(ControlEvent::DeSelectGroup)
                .collect(),
        ))
    } else {
        Some(ControlEvent::SelectFolder(id))
    }
}

/// `CreateFolder` for a new folder of `kind` under `parent`, with the id
/// picked by [`folder::next_folder_id`] and the name checked and trimmed by
/// `EngineState::check_folder_name`.
pub fn create_folder_event(
    state: &EngineState,
    kind: FolderKind,
    parent: Option<FolderId>,
    name: &str,
) -> Result<ControlEvent, &'static str> {
    if let Some(parent) = parent {
        match state.folders.get(&parent) {
            None => return Err("Folder not found"),
            Some(f) if f.kind != kind => return Err("Folder holds a different kind of item"),
            Some(_) => {}
        }
    }
    let name = state.check_folder_name(kind, parent, name, None)?;
    let id = folder::next_folder_id(&state.folders).ok_or("No free folder id")?;
    Ok(ControlEvent::CreateFolder {
        id,
        kind,
        parent,
        name,
    })
}

/// `RenameFolder` with the name checked and trimmed.
pub fn rename_folder_event(
    state: &EngineState,
    id: FolderId,
    name: &str,
) -> Result<ControlEvent, &'static str> {
    let f = state.folders.get(&id).ok_or("Folder not found")?;
    let name = state.check_folder_name(f.kind, f.parent, name, Some(id))?;
    Ok(ControlEvent::RenameFolder(id, name))
}

/// `MoveToFolder` for `item` into `target` (`None` = top level), checked the
/// way the engine checks it (kind, cycles, sibling names). Moving an item to
/// where it already is is an error too, so the dialog never sends a no-op.
pub fn move_to_folder_event(
    state: &EngineState,
    item: FolderItem,
    target: Option<FolderId>,
) -> Result<ControlEvent, &'static str> {
    let kind = item.kind(&state.folders).ok_or("Folder not found")?;
    let current = state.item_folder(item).ok_or("Item not found")?;
    if current == target {
        return Err("Already in this folder");
    }
    if let Some(target) = target {
        match state.folders.get(&target) {
            None => return Err("Folder not found"),
            Some(f) if f.kind != kind => return Err("Folder holds a different kind of item"),
            Some(_) => {}
        }
    }
    if let FolderItem::Folder(moved) = item {
        if target.is_some_and(|t| folder::is_ancestor_or_self(&state.folders, moved, t)) {
            return Err("Folder cannot move into itself or a subfolder");
        }
        state.check_folder_name(kind, target, &state.folders[&moved].name, Some(moved))?;
    }
    Ok(ControlEvent::MoveToFolder {
        item,
        folder: target,
    })
}

//
// Labels.
//

/// "group" / "groups" and so on.
pub fn kind_noun(kind: FolderKind, count: usize) -> &'static str {
    match (kind, count == 1) {
        (FolderKind::Groups, true) => "group",
        (FolderKind::Groups, false) => "groups",
        (FolderKind::Scenes, true) => "scene",
        (FolderKind::Scenes, false) => "scenes",
        (FolderKind::Palettes, true) => "palette",
        (FolderKind::Palettes, false) => "palettes",
    }
}

/// Display name of an item, e.g. `Frontlicht`; `#3` if it has no name.
pub fn item_name(state: &EngineState, item: FolderItem) -> String {
    let name = match item {
        FolderItem::Group(id) => state.groups.get(&id).map(|g| g.name.clone()),
        FolderItem::Scene(id) => state.scenes.get(&id).map(|s| s.name.clone()),
        FolderItem::Palette(id) => state.palettes.get(&id).map(|p| p.name.clone()),
        FolderItem::Folder(id) => state.folders.get(&id).map(|f| f.name.clone()),
    };
    match (name, item) {
        (Some(name), _) if !name.trim().is_empty() => name,
        (_, FolderItem::Group(id) | FolderItem::Scene(id) | FolderItem::Palette(id)) => {
            format!("#{id}")
        }
        (_, FolderItem::Folder(id)) => format!("Folder {id}"),
    }
}

/// Where `folder` is, e.g. `Stage / Front`; `Top level` for `None`.
pub fn folder_path(folders: &BTreeMap<FolderId, Folder>, folder: Option<FolderId>) -> String {
    let mut names = Vec::new();
    let mut seen = BTreeSet::new();
    let mut current = folder;
    while let Some(id) = current {
        if !seen.insert(id) {
            break;
        }
        let Some(f) = folders.get(&id) else { break };
        names.push(f.name.clone());
        current = f.parent;
    }
    if names.is_empty() {
        return "Top level".to_string();
    }
    names.reverse();
    names.join(" / ")
}

/// What deleting a folder does: how many subfolders and items move up, and
/// to where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeleteSummary {
    pub name: String,
    pub kind: FolderKind,
    pub subfolders: usize,
    pub items: usize,
    pub destination: Option<FolderId>,
}

pub fn delete_summary(state: &EngineState, id: FolderId) -> Option<DeleteSummary> {
    let f = state.folders.get(&id)?;
    Some(DeleteSummary {
        name: f.name.clone(),
        kind: f.kind,
        subfolders: state
            .folders
            .values()
            .filter(|c| c.parent == Some(id))
            .count(),
        items: items_directly_in(state, f.kind, id),
        destination: f.parent,
    })
}

/// How many items of `kind` (groups, user scenes or palettes; not folders)
/// sit directly in `folder`.
pub fn items_directly_in(state: &EngineState, kind: FolderKind, folder: FolderId) -> usize {
    let here = Some(folder);
    match kind {
        FolderKind::Groups => state.groups.values().filter(|g| g.folder == here).count(),
        FolderKind::Scenes => state
            .user_scenes()
            .filter(|(_, s)| s.folder == here)
            .count(),
        FolderKind::Palettes => state.palettes.values().filter(|p| p.folder == here).count(),
    }
}

/// Targets for moving `item`: every folder of its kind except, for a folder,
/// itself and its descendants (a whole subtree, so the tree stays intact).
/// The detail line counts the items in each folder (`"3 groups"`); the
/// widget's default would only count the folders shown in the picker.
pub fn move_target_items(state: &EngineState, item: FolderItem) -> Vec<TreeItem<FolderId, u8>> {
    let Some(kind) = item.kind(&state.folders) else {
        return Vec::new();
    };
    let excluded = match item {
        FolderItem::Folder(id) => folder::folder_subtree(&state.folders, id),
        _ => BTreeSet::new(),
    };
    folder_tree_items(&state.folders, kind)
        .into_iter()
        .filter_map(|it| match it.key {
            TreeKey::Folder(f) if !excluded.contains(&f) => {
                let n = items_directly_in(state, kind, f);
                Some(it.with_detail(format!("{n} {}", kind_noun(kind, n))))
            }
            _ => None,
        })
        .collect()
}

//
// Dialogs.
//

/// The folder dialog of one tree (at most one is open at a time). Open it with
/// the constructors, render it every frame with [`FolderDialog::show`].
#[derive(Debug, Clone, Default, PartialEq)]
pub enum FolderDialog {
    #[default]
    Closed,
    /// New folder of `kind` under `parent`.
    Create {
        kind: FolderKind,
        parent: Option<FolderId>,
        name: String,
        error: Option<&'static str>,
    },
    Rename {
        id: FolderId,
        name: String,
        error: Option<&'static str>,
    },
    /// Confirmation; children move to the folder's parent.
    Delete { id: FolderId },
    /// Folder picker for `item`; `target` is the picked folder (`None` = top level).
    Move {
        item: FolderItem,
        target: Option<FolderId>,
        error: Option<&'static str>,
    },
}

const DIALOG_BUTTON: ButtonSize = ButtonSize::Large;
const NAME_WIDTH: f32 = 240.0;
const MOVE_DIALOG_WIDTH: f32 = 340.0;
const MOVE_TREE_HEIGHT: f32 = 230.0;
const MOVE_ROW_HEIGHT: f32 = 48.0;

impl FolderDialog {
    pub fn create(kind: FolderKind, parent: Option<FolderId>) -> Self {
        Self::Create {
            kind,
            parent,
            name: String::new(),
            error: None,
        }
    }

    pub fn rename(state: &EngineState, id: FolderId) -> Self {
        Self::Rename {
            id,
            name: state
                .folders
                .get(&id)
                .map(|f| f.name.clone())
                .unwrap_or_default(),
            error: None,
        }
    }

    pub fn delete(id: FolderId) -> Self {
        Self::Delete { id }
    }

    /// Opens the picker with the item's current folder preselected.
    pub fn move_item(state: &EngineState, item: FolderItem) -> Self {
        Self::Move {
            item,
            target: state.item_folder(item).flatten(),
            error: None,
        }
    }

    pub fn is_open(&self) -> bool {
        *self != Self::Closed
    }

    /// Renders the open dialog. Returns the event to send when the user
    /// confirms; the dialog closes itself then, and on Cancel or Esc. A dialog
    /// whose folder or item disappeared closes without an event.
    pub fn show(&mut self, ctx: &Context, state: &EngineState) -> Option<ControlEvent> {
        let mut event = None;

        let close = match self {
            Self::Closed => return None,
            Self::Create {
                kind,
                parent,
                name,
                error,
            } => {
                let title = format!("New {} folder", kind_noun(*kind, 1));
                let place = format!("In: {}", folder_path(&state.folders, *parent));
                let (confirm, cancel) = name_dialog(ctx, &title, &place, "Create", name, error);
                if confirm {
                    match create_folder_event(state, *kind, *parent, name) {
                        Ok(ev) => event = Some(ev),
                        Err(err) => *error = Some(err),
                    }
                }
                cancel
            }
            Self::Rename { id, name, error } => {
                let Some(f) = state.folders.get(id) else {
                    *self = Self::Closed;
                    return None;
                };
                let place = format!("In: {}", folder_path(&state.folders, f.parent));
                let (confirm, cancel) =
                    name_dialog(ctx, "Rename folder", &place, "Rename", name, error);
                if confirm {
                    match rename_folder_event(state, *id, name) {
                        Ok(ev) => event = Some(ev),
                        Err(err) => *error = Some(err),
                    }
                }
                cancel
            }
            Self::Delete { id } => {
                let Some(summary) = delete_summary(state, *id) else {
                    *self = Self::Closed;
                    return None;
                };
                let (confirm, cancel) = delete_dialog(ctx, state, &summary);
                if confirm {
                    event = Some(ControlEvent::DeleteFolder(*id));
                }
                cancel
            }
            Self::Move {
                item,
                target,
                error,
            } => {
                if state.item_folder(*item).is_none() {
                    *self = Self::Closed;
                    return None;
                }
                let (confirm, cancel) = move_dialog(ctx, state, *item, target, error);
                if confirm {
                    match move_to_folder_event(state, *item, *target) {
                        Ok(ev) => event = Some(ev),
                        Err(err) => *error = Some(err),
                    }
                }
                cancel
            }
        };

        if close || event.is_some() {
            *self = Self::Closed;
        }
        event
    }
}

/// A name input with a context line, an inline error and Cancel / confirm.
/// Enter confirms. Returns `(confirm, cancel)`.
fn name_dialog(
    ctx: &Context,
    title: &str,
    place: &str,
    confirm_label: &str,
    name: &mut String,
    error: &mut Option<&'static str>,
) -> (bool, bool) {
    let mut confirm = false;
    let mut cancel = false;
    // Dialogs are centred on this size; the height is an estimate of the content.
    let response = Dialog::new(
        format!("folder-name::{title}"),
        egui::vec2(NAME_WIDTH + 20.0, 200.0),
    )
    .with_backdrop()
    .show(ctx, |ui| {
        ui.spacing_mut().interact_size = egui::vec2(44.0, 36.0);
        ui.label(RichText::new(title).strong().size(16.0));
        ui.label(RichText::new(place).size(12.0).color(Color32::GRAY));
        ui.add_space(8.0);
        let input = TextInput::new(NAME_WIDTH)
            .with_hint_text("Folder name")
            .ui(ui, name);
        if input.changed() {
            // A stale error would describe the old text.
            *error = None;
        }
        ui.add_space(4.0);
        ui.label(
            RichText::new(error.unwrap_or(" "))
                .size(12.0)
                .color(Color32::from_rgb(255, 120, 120)),
        );
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.add_space(
                ((ui.available_width() - 2.0 * DIALOG_BUTTON.dim().0.x - 8.0) / 2.0).max(0.0),
            );
            if components::button(ui, false, "Cancel", DIALOG_BUTTON) {
                cancel = true;
            }
            ui.add_space(8.0);
            if components::button(ui, true, confirm_label, DIALOG_BUTTON) {
                confirm = true;
            }
        });
        if ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
            confirm = true;
        }
    });
    (confirm, cancel || response.cancel_requested)
}

fn delete_dialog(ctx: &Context, state: &EngineState, summary: &DeleteSummary) -> (bool, bool) {
    let mut confirm = false;
    let mut cancel = false;
    let destination = folder_path(&state.folders, summary.destination);
    let mut moved = Vec::new();
    if summary.subfolders > 0 {
        moved.push(format!(
            "{} {}",
            summary.subfolders,
            if summary.subfolders == 1 {
                "subfolder"
            } else {
                "subfolders"
            }
        ));
    }
    if summary.items > 0 {
        moved.push(format!(
            "{} {}",
            summary.items,
            kind_noun(summary.kind, summary.items)
        ));
    }
    let consequence = if moved.is_empty() {
        "The folder is empty.".to_string()
    } else {
        let destination = match summary.destination {
            Some(_) => destination,
            None => "the top level".to_string(),
        };
        format!("Its {} move to {destination}.", moved.join(" and "))
    };
    let response = Dialog::new(
        "folder-delete".to_string(),
        egui::vec2(NAME_WIDTH + 20.0, 180.0),
    )
    .with_backdrop()
    .show(ctx, |ui| {
        ui.label(RichText::new("Delete folder?").strong().size(16.0));
        ui.add_space(4.0);
        ui.label(RichText::new(&summary.name).size(16.0));
        ui.add_space(6.0);
        ui.label(RichText::new(consequence).size(12.0));
        ui.label(
            RichText::new(format!("No {} are deleted.", kind_noun(summary.kind, 2)))
                .size(12.0)
                .color(Color32::GRAY),
        );
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            ui.add_space(
                ((ui.available_width() - 2.0 * DIALOG_BUTTON.dim().0.x - 8.0) / 2.0).max(0.0),
            );
            if components::button(ui, false, "Cancel", DIALOG_BUTTON) {
                cancel = true;
            }
            ui.add_space(8.0);
            if components::button(ui, true, "Delete", DIALOG_BUTTON) {
                confirm = true;
            }
        });
    });
    (confirm, cancel || response.cancel_requested)
}

/// "Move to…": a top-level entry and the folder tree (without the moved
/// folder's own subtree). Tapping picks the target; the confirm button moves.
fn move_dialog(
    ctx: &Context,
    state: &EngineState,
    item: FolderItem,
    target: &mut Option<FolderId>,
    error: &mut Option<&'static str>,
) -> (bool, bool) {
    let mut confirm = false;
    let mut cancel = false;
    let kind = item.kind(&state.folders).unwrap_or(FolderKind::Groups);
    let what = match item {
        FolderItem::Folder(_) => "folder",
        _ => kind_noun(kind, 1),
    };
    let items = move_target_items(state, item);
    let current = state.item_folder(item).flatten();

    // Top level plus every target row, until the list scrolls.
    let list_height =
        ((items.len() + 1) as f32 * (MOVE_ROW_HEIGHT + 2.0) + 6.0).min(MOVE_TREE_HEIGHT);
    let response = Dialog::new(
        "folder-move".to_string(),
        egui::vec2(MOVE_DIALOG_WIDTH, 160.0 + list_height),
    )
    .with_backdrop()
    .show(ctx, |ui| {
        ui.label(RichText::new(format!("Move {what}")).strong().size(16.0));
        ui.label(RichText::new(item_name(state, item)).size(14.0));
        ui.label(
            RichText::new(format!("Now in: {}", folder_path(&state.folders, current)))
                .size(12.0)
                .color(Color32::GRAY),
        );
        ui.add_space(8.0);

        let width = ui.available_width();
        egui::ScrollArea::vertical()
            .id_salt("folder-move-targets")
            .max_height(MOVE_TREE_HEIGHT)
            // The dialog window has no fixed height, so the list would shrink
            // to egui's 64 px minimum; show every row up to the maximum.
            .min_scrolled_height(list_height)
            .show(ui, |ui| {
                ui.set_width(width);
                let top = components::button(
                    ui,
                    target.is_none(),
                    "Top level",
                    ButtonSize::Custom(width, MOVE_ROW_HEIGHT, 14.0),
                );
                if top {
                    *target = None;
                    *error = None;
                }
                ui.add_space(2.0);
                if let Some(TreeAction::TapFolder(f)) =
                    Tree::new(("folder_move_targets", kind), &items)
                        .selected(target.map(TreeKey::Folder))
                        .row_height(MOVE_ROW_HEIGHT)
                        .show(ui)
                {
                    *target = Some(f);
                    *error = None;
                }
            });

        ui.add_space(4.0);
        ui.label(
            RichText::new(error.unwrap_or(" "))
                .size(12.0)
                .color(Color32::from_rgb(255, 120, 120)),
        );
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.add_space(
                ((ui.available_width() - 2.0 * DIALOG_BUTTON.dim().0.x - 8.0) / 2.0).max(0.0),
            );
            if components::button(ui, false, "Cancel", DIALOG_BUTTON) {
                cancel = true;
            }
            ui.add_space(8.0);
            let enabled = *target != current;
            if components::action_button(
                ui,
                enabled,
                "Move here",
                DIALOG_BUTTON,
                Some("Pick another folder"),
            ) {
                confirm = true;
            }
        });
    });
    (confirm, cancel || response.cancel_requested)
}

#[cfg(test)]
mod tests {
    use super::*;
    use blaulicht_shared::fixture::state::FixtureGroup;

    /// Groups tree: Stage(0) > Front(1) > Inner(2); Side(3) at the top level;
    /// a scenes folder Songs(4). Groups 0, 1 in Front, 2 in Inner, 3 in
    /// Stage, 4 at the top level.
    fn state() -> EngineState {
        let mut s = EngineState::default();
        for gid in 0..5u8 {
            s.groups.insert(
                gid,
                FixtureGroup {
                    name: format!("G{gid}"),
                    ..FixtureGroup::default()
                },
            );
        }
        let mut create = |id, kind, parent, name: &str| {
            s.create_folder(id, kind, parent, name.to_string()).unwrap();
        };
        create(0, FolderKind::Groups, None, "Stage");
        create(1, FolderKind::Groups, Some(0), "Front");
        create(2, FolderKind::Groups, Some(1), "Inner");
        create(3, FolderKind::Groups, None, "Side");
        create(4, FolderKind::Scenes, None, "Songs");
        for (gid, folder) in [(0, 1), (1, 1), (2, 2), (3, 0)] {
            s.move_to_folder(FolderItem::Group(gid), Some(folder))
                .unwrap();
        }
        s
    }

    fn keys(items: &[TreeItem<FolderId, u8>]) -> Vec<TreeKey<FolderId, u8>> {
        items.iter().map(|it| it.key).collect()
    }

    /// `ControlEvent` has no `PartialEq`; compare its Debug form.
    fn ev(event: ControlEvent) -> String {
        format!("{event:?}")
    }

    fn ok(result: Result<ControlEvent, &'static str>) -> Result<String, &'static str> {
        result.map(ev)
    }

    use TreeKey::{Folder as F, Leaf as L};

    #[test]
    fn group_items_are_sorted_folders_then_groups() {
        let mut s = state();
        // Order beats name: Side before Stage.
        s.reorder_folder(3, 0).unwrap();
        s.reorder_folder(0, 1).unwrap();
        let items = group_tree_items(&s);
        // As the tree shows them: siblings by (order, name), folders before
        // groups under each parent.
        let shown: Vec<_> = components::visible_rows(&items, &BTreeSet::new())
            .into_iter()
            .map(|row| (items[row.item].key, row.depth))
            .collect();
        assert_eq!(
            shown,
            vec![
                (F(3), 0),
                (F(0), 0),
                (F(1), 1),
                (F(2), 2),
                (L(2), 3),
                (L(0), 2),
                (L(1), 2),
                (L(3), 1),
                (L(4), 0),
            ]
        );
        let g3 = items.iter().find(|it| it.key == L(3)).unwrap();
        assert_eq!(g3.parent, Some(0));
        assert_eq!(g3.label, "G3");
        assert_eq!(g3.detail.as_deref(), Some("0 FX | #3"));
        // The scenes folder is not part of the groups tree.
        assert!(!items.iter().any(|it| it.key == F(4)));
    }

    #[test]
    fn folder_tap_selects_then_deselects() {
        let mut s = state();
        let tap = |s: &EngineState, id| folder_tap_event(s, id).map(ev);
        assert_eq!(tap(&s, 0), Some(ev(ControlEvent::SelectFolder(0))));
        // Partly selected: still selects.
        s.selection.group_ids.insert(0);
        assert_eq!(tap(&s, 1), Some(ev(ControlEvent::SelectFolder(1))));
        s.select_folder(1).unwrap();
        assert_eq!(
            tap(&s, 1),
            Some(ev(ControlEvent::Transaction(vec![
                ControlEvent::DeSelectGroup(0),
                ControlEvent::DeSelectGroup(1),
                ControlEvent::DeSelectGroup(2),
            ])))
        );
        assert_eq!(fully_selected_group_folders(&s), vec![1, 2]);
        // Empty folder: nothing to do.
        assert_eq!(tap(&s, 3), None);
        // Missing folder and scenes folder.
        assert_eq!(tap(&s, 99), None);
        assert_eq!(tap(&s, 4), None);
    }

    #[test]
    fn create_picks_the_next_id_and_checks_the_name() {
        let s = state();
        let create = |parent, name| ok(create_folder_event(&s, FolderKind::Groups, parent, name));
        assert_eq!(
            create(Some(0), "  Back "),
            Ok(ev(ControlEvent::CreateFolder {
                id: 5,
                kind: FolderKind::Groups,
                parent: Some(0),
                name: "Back".to_string(),
            }))
        );
        assert_eq!(
            create(Some(0), "FRONT"),
            Err("A sibling folder already has this name")
        );
        assert_eq!(create(None, "  "), Err("Folder name is empty"));
        // Same name in another parent is fine.
        assert!(create(None, "Front").is_ok());
        assert_eq!(
            create(Some(4), "X"),
            Err("Folder holds a different kind of item")
        );
        assert_eq!(create(Some(42), "X"), Err("Folder not found"));
    }

    #[test]
    fn rename_allows_case_changes_and_rejects_siblings() {
        let s = state();
        assert_eq!(
            ok(rename_folder_event(&s, 0, "STAGE")),
            Ok(ev(ControlEvent::RenameFolder(0, "STAGE".to_string())))
        );
        assert_eq!(
            ok(rename_folder_event(&s, 0, "side")),
            Err("A sibling folder already has this name")
        );
        assert_eq!(
            ok(rename_folder_event(&s, 42, "X")),
            Err("Folder not found")
        );
    }

    /// Whatever the dialog accepts, the engine accepts, and the other way
    /// round (except no-op moves, which the dialog refuses).
    #[test]
    fn move_checks_match_the_engine() {
        let mut s = state();
        // A top-level folder named like a subfolder of Stage.
        s.create_folder(5, FolderKind::Groups, None, "Front".to_string())
            .unwrap();
        let items = (0..5u8)
            .map(FolderItem::Group)
            .chain((0..7).map(FolderItem::Folder))
            .chain([FolderItem::Scene(0)]);
        let targets: Vec<Option<FolderId>> =
            std::iter::once(None).chain((0..7).map(Some)).collect();
        for item in items {
            for &target in &targets {
                let ours = ok(move_to_folder_event(&s, item, target));
                let mut probe = s.clone();
                let engine = probe.move_to_folder(item, target);
                if s.item_folder(item).is_some() && s.item_folder(item) == Some(target) {
                    assert_eq!(
                        ours,
                        Err("Already in this folder"),
                        "{item:?} -> {target:?}"
                    );
                    continue;
                }
                assert_eq!(
                    ours.is_ok(),
                    engine.is_ok(),
                    "{item:?} -> {target:?}: {ours:?} vs {engine:?}"
                );
                if let Err(err) = engine {
                    assert_eq!(ours, Err(err), "{item:?} -> {target:?}");
                }
            }
        }
        let to = |item, target| ok(move_to_folder_event(&s, item, target));
        assert_eq!(
            to(FolderItem::Folder(0), Some(2)),
            Err("Folder cannot move into itself or a subfolder")
        );
        assert_eq!(
            to(FolderItem::Folder(1), None),
            Err("A sibling folder already has this name")
        );
        assert_eq!(
            to(FolderItem::Group(4), Some(2)),
            Ok(ev(ControlEvent::MoveToFolder {
                item: FolderItem::Group(4),
                folder: Some(2),
            }))
        );
    }

    #[test]
    fn move_targets_skip_the_moved_subtree() {
        let s = state();
        assert_eq!(
            keys(&move_target_items(&s, FolderItem::Folder(1))),
            vec![F(3), F(0)]
        );
        assert_eq!(
            keys(&move_target_items(&s, FolderItem::Group(4))),
            // `sorted_folder_ids` order; the tree nests them.
            vec![F(1), F(2), F(3), F(0)]
        );
        assert_eq!(
            keys(&move_target_items(&s, FolderItem::Scene(0))),
            vec![F(4)]
        );
        assert!(move_target_items(&s, FolderItem::Folder(42)).is_empty());
        // Details count the items in each target, not the folders shown.
        let details: Vec<_> = move_target_items(&s, FolderItem::Group(4))
            .into_iter()
            .map(|it| it.detail.unwrap_or_default())
            .collect();
        assert_eq!(details, vec!["2 groups", "1 group", "0 groups", "1 group"]);
    }

    #[test]
    fn labels_paths_and_delete_summary() {
        let s = state();
        assert_eq!(folder_path(&s.folders, Some(2)), "Stage / Front / Inner");
        assert_eq!(folder_path(&s.folders, None), "Top level");
        assert_eq!(item_name(&s, FolderItem::Group(3)), "G3");
        assert_eq!(item_name(&s, FolderItem::Folder(1)), "Front");
        assert_eq!(item_name(&s, FolderItem::Group(42)), "#42");
        assert_eq!(
            delete_summary(&s, 1),
            Some(DeleteSummary {
                name: "Front".to_string(),
                kind: FolderKind::Groups,
                subfolders: 1,
                items: 2,
                destination: Some(0),
            })
        );
        assert_eq!(delete_summary(&s, 42), None);
        assert_eq!(kind_noun(FolderKind::Scenes, 1), "scene");
        assert_eq!(kind_noun(FolderKind::Palettes, 0), "palettes");
    }

    #[test]
    fn dialog_constructors_prefill() {
        let s = state();
        assert_eq!(
            FolderDialog::rename(&s, 1),
            FolderDialog::Rename {
                id: 1,
                name: "Front".to_string(),
                error: None,
            }
        );
        assert_eq!(
            FolderDialog::move_item(&s, FolderItem::Group(2)),
            FolderDialog::Move {
                item: FolderItem::Group(2),
                target: Some(2),
                error: None,
            }
        );
        assert!(!FolderDialog::default().is_open());
        assert!(FolderDialog::delete(0).is_open());
    }
}
