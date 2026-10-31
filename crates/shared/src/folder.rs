//! Folder hierarchy for fixture groups, scenes and palettes.
//!
//! Folders are pure organisation: they live in [`EngineState::folders`] and
//! items point at their folder through a `folder: Option<FolderId>` field
//! (`None` is the root). Every folder holds exactly one [`FolderKind`] of item,
//! and its parent (if any) must be a folder of the same kind.
//!
//! Invariants kept by the mutating methods below (and restored for external
//! input by [`EngineState::repair_folders`]):
//! - a folder's parent exists and has the same kind as the folder;
//! - the parent chain never forms a cycle;
//! - an item's folder exists and has the item's kind.
//!
//! Name rules, kept by `CreateFolder`, `RenameFolder` and `MoveToFolder`
//! (folders): a name is trimmed and must not be empty, and no two sibling
//! folders (same kind, same parent) share a name, ignoring case. Deleting a
//! folder can still put two equally named folders side by side, and old
//! showfiles may contain such duplicates; [`EngineState::repair_folders`] only
//! logs them.
//!
//! Siblings sort by `(order, name)` ([`sorted_children`]).
//!
//! Every folder event has a rollback ([`EngineState::folder_rollback`]): applying
//! the event and then its rollback leaves the state unchanged, as long as the
//! state follows the name rules.
//!
//! The read helpers do not rely on these invariants, so a broken state can
//! never make them loop or panic.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};

use crate::{ControlEvent, EngineState, scene::BLANK_SCENE_ID};

pub type FolderId = u16;

/// The kind of item a folder holds.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Encode, Decode,
)]
pub enum FolderKind {
    Groups,
    Scenes,
    Palettes,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Encode, Decode)]
pub struct Folder {
    pub name: String,
    /// `None` places the folder at the root of its kind's tree.
    pub parent: Option<FolderId>,
    pub kind: FolderKind,
    /// Position among its siblings, which sort by `(order, name)`
    /// ([`sorted_children`]). New folders start at 0, so siblings nobody
    /// reordered sort by name. Showfiles without the field load as 0.
    #[serde(default)]
    pub order: u16,
}

/// Anything that can be placed into a folder, including a folder itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Encode, Decode)]
pub enum FolderItem {
    Group(u8),
    Scene(u8),
    Palette(u8),
    Folder(FolderId),
}

impl FolderItem {
    /// The folder kind this item belongs in. For a folder this is its own kind,
    /// so it needs the folder map; `None` if the folder does not exist.
    pub fn kind(&self, folders: &BTreeMap<FolderId, Folder>) -> Option<FolderKind> {
        match self {
            Self::Group(_) => Some(FolderKind::Groups),
            Self::Scene(_) => Some(FolderKind::Scenes),
            Self::Palette(_) => Some(FolderKind::Palettes),
            Self::Folder(id) => folders.get(id).map(|folder| folder.kind),
        }
    }
}

/// Lowest unused folder id, or `None` if all `u16` ids are taken. A sender
/// picks the id of a new folder with this before sending `CreateFolder`.
pub fn next_folder_id(folders: &BTreeMap<FolderId, Folder>) -> Option<FolderId> {
    (0..=FolderId::MAX).find(|id| !folders.contains_key(id))
}

/// `true` if `ancestor` is `folder` itself or one of its ancestors.
/// Cycle-safe: a looping parent chain stops at the first repeated folder.
pub fn is_ancestor_or_self(
    folders: &BTreeMap<FolderId, Folder>,
    ancestor: FolderId,
    folder: FolderId,
) -> bool {
    let mut seen = HashSet::new();
    let mut current = Some(folder);
    while let Some(id) = current {
        if id == ancestor {
            return true;
        }
        if !seen.insert(id) {
            return false;
        }
        current = folders.get(&id).and_then(|f| f.parent);
    }
    false
}

/// `root` and every folder below it, recursively. Cycle-safe. Empty if `root`
/// does not exist.
pub fn folder_subtree(folders: &BTreeMap<FolderId, Folder>, root: FolderId) -> BTreeSet<FolderId> {
    let mut subtree = BTreeSet::new();
    if !folders.contains_key(&root) {
        return subtree;
    }
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        if !subtree.insert(id) {
            continue;
        }
        stack.extend(
            folders
                .iter()
                .filter(|(_, f)| f.parent == Some(id))
                .map(|(child, _)| *child),
        );
    }
    subtree
}

/// Trims a folder name and rejects an empty one.
pub fn normalize_folder_name(name: &str) -> Result<String, &'static str> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Folder name is empty");
    }
    Ok(name.to_string())
}

/// `true` if two folder names collide: equal after trimming, ignoring case.
pub fn folder_names_equal(a: &str, b: &str) -> bool {
    a.trim().to_lowercase() == b.trim().to_lowercase()
}

/// Sibling sort key: `order`, then the name ignoring case, then the id, so
/// the order is total even for duplicate names in old showfiles.
fn sibling_key(id: FolderId, folder: &Folder) -> (u16, String, FolderId) {
    (folder.order, folder.name.trim().to_lowercase(), id)
}

/// The folders of `kind` directly under `parent` (`None` = root), sorted by
/// `(order, name)`.
pub fn sorted_children(
    folders: &BTreeMap<FolderId, Folder>,
    kind: FolderKind,
    parent: Option<FolderId>,
) -> Vec<FolderId> {
    let mut children: Vec<_> = folders
        .iter()
        .filter(|(_, f)| f.kind == kind && f.parent == parent)
        .collect();
    children.sort_by_cached_key(|(id, f)| sibling_key(**id, f));
    children.into_iter().map(|(id, _)| *id).collect()
}

/// Every folder of `kind`, sorted by `(order, name)`. Siblings keep that
/// relative order, so this list can feed a tree widget that keeps the input
/// order under each parent.
pub fn sorted_folder_ids(folders: &BTreeMap<FolderId, Folder>, kind: FolderKind) -> Vec<FolderId> {
    let mut ids: Vec<_> = folders.iter().filter(|(_, f)| f.kind == kind).collect();
    ids.sort_by_cached_key(|(id, f)| sibling_key(**id, f));
    ids.into_iter().map(|(id, _)| *id).collect()
}

impl EngineState {
    /// The folder an item currently sits in (`Some(None)` = root), or `None`
    /// if the item does not exist. The ephemeral BLANK scene does not count.
    pub fn item_folder(&self, item: FolderItem) -> Option<Option<FolderId>> {
        match item {
            FolderItem::Group(id) => self.groups.get(&id).map(|g| g.folder),
            FolderItem::Scene(id) if id == BLANK_SCENE_ID => None,
            FolderItem::Scene(id) => self.scenes.get(&id).map(|s| s.folder),
            FolderItem::Palette(id) => self.palettes.get(&id).map(|p| p.folder),
            FolderItem::Folder(id) => self.folders.get(&id).map(|f| f.parent),
        }
    }

    fn item_folder_mut(&mut self, item: FolderItem) -> Option<&mut Option<FolderId>> {
        match item {
            FolderItem::Group(id) => self.groups.get_mut(&id).map(|g| &mut g.folder),
            FolderItem::Scene(id) if id == BLANK_SCENE_ID => None,
            FolderItem::Scene(id) => self.scenes.get_mut(&id).map(|s| &mut s.folder),
            FolderItem::Palette(id) => self.palettes.get_mut(&id).map(|p| &mut p.folder),
            FolderItem::Folder(id) => self.folders.get_mut(&id).map(|f| &mut f.parent),
        }
    }

    /// The items of `kind` (not folders) that sit directly in `folder`.
    fn items_in_folder(&self, kind: FolderKind, folder: FolderId) -> Vec<FolderItem> {
        let here = Some(folder);
        match kind {
            FolderKind::Groups => self
                .groups
                .iter()
                .filter(|(_, g)| g.folder == here)
                .map(|(id, _)| FolderItem::Group(*id))
                .collect(),
            FolderKind::Scenes => self
                .user_scenes()
                .filter(|(_, s)| s.folder == here)
                .map(|(id, _)| FolderItem::Scene(*id))
                .collect(),
            FolderKind::Palettes => self
                .palettes
                .iter()
                .filter(|(_, p)| p.folder == here)
                .map(|(id, _)| FolderItem::Palette(*id))
                .collect(),
        }
    }

    /// Checks that `parent` can hold an item of `kind`.
    fn check_folder_target(
        &self,
        kind: FolderKind,
        parent: Option<FolderId>,
    ) -> Result<(), &'static str> {
        let Some(parent) = parent else {
            return Ok(());
        };
        match self.folders.get(&parent) {
            None => Err("Folder not found"),
            Some(folder) if folder.kind != kind => Err("Folder holds a different kind of item"),
            Some(_) => Ok(()),
        }
    }

    /// `true` if a folder of `kind` under `parent`, other than `except`,
    /// already uses `name` ([`folder_names_equal`]).
    fn sibling_name_taken(
        &self,
        kind: FolderKind,
        parent: Option<FolderId>,
        name: &str,
        except: Option<FolderId>,
    ) -> bool {
        self.folders.iter().any(|(id, folder)| {
            Some(*id) != except
                && folder.kind == kind
                && folder.parent == parent
                && folder_names_equal(&folder.name, name)
        })
    }

    /// Checks `name` for a folder of `kind` under `parent` and returns it
    /// trimmed. `folder` is the folder being renamed, if any, so that it does
    /// not collide with itself. The UI can call this to validate input before
    /// sending `CreateFolder` / `RenameFolder`.
    pub fn check_folder_name(
        &self,
        kind: FolderKind,
        parent: Option<FolderId>,
        name: &str,
        folder: Option<FolderId>,
    ) -> Result<String, &'static str> {
        let name = normalize_folder_name(name)?;
        if self.sibling_name_taken(kind, parent, &name, folder) {
            return Err("A sibling folder already has this name");
        }
        Ok(name)
    }

    /// Creates folder `id` of `kind` under `parent` (`None` = root). The id
    /// must be free; senders pick it with [`next_folder_id`]. The name is
    /// trimmed and checked with [`Self::check_folder_name`]. New folders get
    /// `order` 0.
    pub fn create_folder(
        &mut self,
        id: FolderId,
        kind: FolderKind,
        parent: Option<FolderId>,
        name: String,
    ) -> Result<(), &'static str> {
        if self.folders.contains_key(&id) {
            return Err("Folder id already taken");
        }
        self.check_folder_target(kind, parent)?;
        let name = self.check_folder_name(kind, parent, &name, None)?;
        self.folders.insert(
            id,
            Folder {
                name,
                parent,
                kind,
                order: 0,
            },
        );
        Ok(())
    }

    /// Renames a folder; the name is checked with [`Self::check_folder_name`].
    pub fn rename_folder(&mut self, id: FolderId, name: String) -> Result<(), &'static str> {
        let folder = self.folders.get(&id).ok_or("Folder not found")?;
        let name = self.check_folder_name(folder.kind, folder.parent, &name, Some(id))?;
        if let Some(folder) = self.folders.get_mut(&id) {
            folder.name = name;
        }
        Ok(())
    }

    /// Sets a folder's position among its siblings (see [`sorted_children`]).
    pub fn reorder_folder(&mut self, id: FolderId, order: u16) -> Result<(), &'static str> {
        let folder = self.folders.get_mut(&id).ok_or("Folder not found")?;
        folder.order = order;
        Ok(())
    }

    /// Deletes a folder. Its child folders and items move to the deleted
    /// folder's parent (the root if it had none); nothing else is deleted.
    pub fn delete_folder(&mut self, id: FolderId) -> Result<(), &'static str> {
        let removed = self.folders.remove(&id).ok_or("Folder not found")?;
        let new_parent = removed.parent;

        let reparent = |folder: &mut Option<FolderId>| {
            if *folder == Some(id) {
                *folder = new_parent;
            }
        };
        self.folders
            .values_mut()
            .for_each(|folder| reparent(&mut folder.parent));
        match removed.kind {
            FolderKind::Groups => self
                .groups
                .values_mut()
                .for_each(|group| reparent(&mut group.folder)),
            FolderKind::Scenes => self
                .scenes
                .values_mut()
                .for_each(|scene| reparent(&mut scene.folder)),
            FolderKind::Palettes => self
                .palettes
                .values_mut()
                .for_each(|palette| reparent(&mut palette.folder)),
        }
        Ok(())
    }

    /// Moves an item (or a folder) into `folder` (`None` = root). The target
    /// must hold the item's kind, and a folder cannot move into itself or any
    /// of its descendants, nor next to a sibling folder with the same name.
    pub fn move_to_folder(
        &mut self,
        item: FolderItem,
        folder: Option<FolderId>,
    ) -> Result<(), &'static str> {
        let kind = item.kind(&self.folders).ok_or("Folder not found")?;
        if self.item_folder(item).is_none() {
            return Err("Item not found");
        }
        self.check_folder_target(kind, folder)?;
        if let FolderItem::Folder(moved) = item {
            if let Some(target) = folder
                && is_ancestor_or_self(&self.folders, moved, target)
            {
                return Err("Folder cannot move into itself or a subfolder");
            }
            let name = &self.folders[&moved].name;
            if self.sibling_name_taken(kind, folder, name, Some(moved)) {
                return Err("A sibling folder already has this name");
            }
        }
        if let Some(slot) = self.item_folder_mut(item) {
            *slot = folder;
        }
        Ok(())
    }

    /// Ids of every group in the groups folder `id` and, recursively, in all of
    /// its subfolders. Empty if `id` does not exist or is not a groups folder.
    pub fn folder_group_ids(&self, id: FolderId) -> BTreeSet<u8> {
        if self.folders.get(&id).map(|f| f.kind) != Some(FolderKind::Groups) {
            return BTreeSet::new();
        }
        let subtree = folder_subtree(&self.folders, id);
        self.groups
            .iter()
            .filter(|(_, group)| group.folder.is_some_and(|f| subtree.contains(&f)))
            .map(|(gid, _)| *gid)
            .collect()
    }

    /// Adds every group under the groups folder `id` (recursively, see
    /// [`Self::folder_group_ids`]) to the current selection, like a
    /// `SelectGroup` per group. The fixture filter is cleared, as a multi-group
    /// selection cannot carry one.
    pub fn select_folder(&mut self, id: FolderId) -> Result<(), &'static str> {
        match self.folders.get(&id) {
            None => return Err("Folder not found"),
            Some(folder) if folder.kind != FolderKind::Groups => {
                return Err("Not a groups folder");
            }
            Some(_) => {}
        }
        let group_ids = self.folder_group_ids(id);
        if group_ids.is_empty() {
            return Err("Folder contains no groups");
        }
        let mut added = false;
        for gid in group_ids {
            added |= self.selection.group_ids.insert(gid);
        }
        if !added {
            return Err("Already selected");
        }
        self.selection.fixtures_in_group.clear();
        Ok(())
    }

    /// Applies one folder event (`CreateFolder` .. `SelectFolder`,
    /// `ReorderFolder`) and returns its rollback (see
    /// [`Self::folder_rollback`]). On error the state is unchanged.
    pub fn apply_folder_event(
        &mut self,
        event: ControlEvent,
    ) -> Result<ControlEvent, &'static str> {
        let rollback = self.folder_rollback(&event)?;
        match event {
            ControlEvent::CreateFolder {
                id,
                kind,
                parent,
                name,
            } => self.create_folder(id, kind, parent, name),
            ControlEvent::RenameFolder(id, name) => self.rename_folder(id, name),
            ControlEvent::DeleteFolder(id) => self.delete_folder(id),
            ControlEvent::MoveToFolder { item, folder } => self.move_to_folder(item, folder),
            ControlEvent::SelectFolder(id) => self.select_folder(id),
            ControlEvent::ReorderFolder(id, order) => self.reorder_folder(id, order),
            _ => Err("Not a folder event"),
        }?;
        Ok(rollback)
    }

    /// The event that undoes the folder event `event` if it is applied to the
    /// current state. Read-only, so a UI can compute it before sending
    /// `event`. It errors only if the event's folder or item does not exist
    /// (or `event` is no folder event); everything else is checked when the
    /// event is applied.
    ///
    /// - `CreateFolder { id, .. }` -> `DeleteFolder(id)`.
    /// - `RenameFolder(id, _)` -> `RenameFolder(id, <current name>)`.
    /// - `ReorderFolder(id, _)` -> `ReorderFolder(id, <current order>)`.
    /// - `MoveToFolder { item, .. }` -> `MoveToFolder` back to the item's
    ///   current folder.
    /// - `DeleteFolder(id)` -> a `Transaction` that recreates the folder with
    ///   the same id, parent and name (`CreateFolder`), restores its order
    ///   (`ReorderFolder`, only if not 0) and moves its child folders and
    ///   items back in (one `MoveToFolder` each). If a child folder shares
    ///   the folder's name, the folder is recreated under a free temporary
    ///   name and renamed back at the end (`RenameFolder`), since that child
    ///   sits next to it until it has been moved back.
    /// - `SelectFolder(id)` -> a `Transaction` of one `DeSelectGroup` per
    ///   group the event adds to the selection. If it adds any, the fixture
    ///   filter it clears is restored after those
    ///   (`LimitSelectionToFixtureInCurrentGroup` per fixture). An empty
    ///   `Transaction` if it would add nothing (the event then fails anyway).
    pub fn folder_rollback(&self, event: &ControlEvent) -> Result<ControlEvent, &'static str> {
        let folder = |id: &FolderId| self.folders.get(id).ok_or("Folder not found");
        Ok(match event {
            ControlEvent::CreateFolder { id, .. } => ControlEvent::DeleteFolder(*id),
            ControlEvent::RenameFolder(id, _) => {
                ControlEvent::RenameFolder(*id, folder(id)?.name.clone())
            }
            ControlEvent::ReorderFolder(id, _) => {
                ControlEvent::ReorderFolder(*id, folder(id)?.order)
            }
            ControlEvent::MoveToFolder { item, .. } => {
                item.kind(&self.folders).ok_or("Folder not found")?;
                let previous = self.item_folder(*item).ok_or("Item not found")?;
                ControlEvent::MoveToFolder {
                    item: *item,
                    folder: previous,
                }
            }
            ControlEvent::DeleteFolder(id) => self.delete_folder_rollback(*id)?,
            ControlEvent::SelectFolder(id) => {
                folder(id)?;
                let added: Vec<u8> = self
                    .folder_group_ids(*id)
                    .into_iter()
                    .filter(|gid| !self.selection.group_ids.contains(gid))
                    .collect();
                let mut events: Vec<_> = added
                    .iter()
                    .map(|gid| ControlEvent::DeSelectGroup(*gid))
                    .collect();
                if !added.is_empty() {
                    let mut filter: Vec<u8> =
                        self.selection.fixtures_in_group.iter().copied().collect();
                    filter.sort_unstable();
                    events.extend(
                        filter
                            .into_iter()
                            .map(ControlEvent::LimitSelectionToFixtureInCurrentGroup),
                    );
                }
                ControlEvent::Transaction(events)
            }
            _ => return Err("Not a folder event"),
        })
    }

    fn delete_folder_rollback(&self, id: FolderId) -> Result<ControlEvent, &'static str> {
        let folder = self.folders.get(&id).ok_or("Folder not found")?;

        // After the delete, the folder's siblings and its child folders share
        // its parent. If one of them has the folder's name, recreating it
        // under that name would be rejected.
        let names_after_delete: Vec<&str> = self
            .folders
            .iter()
            .filter(|(other, f)| {
                **other != id
                    && f.kind == folder.kind
                    && (f.parent == folder.parent || f.parent == Some(id))
            })
            .map(|(_, f)| f.name.as_str())
            .collect();
        let taken = |name: &str| {
            names_after_delete
                .iter()
                .any(|other| folder_names_equal(other, name))
        };
        let clash = taken(&folder.name);
        let create_name = if clash {
            (1..=names_after_delete.len() + 1)
                .map(|n| format!("{} ({n})", folder.name.trim()))
                .find(|candidate| !taken(candidate))
                .unwrap_or_else(|| folder.name.clone())
        } else {
            folder.name.clone()
        };

        let mut events = vec![ControlEvent::CreateFolder {
            id,
            kind: folder.kind,
            parent: folder.parent,
            name: create_name,
        }];
        if folder.order != 0 {
            events.push(ControlEvent::ReorderFolder(id, folder.order));
        }
        let child_folders = self
            .folders
            .iter()
            .filter(|(_, f)| f.parent == Some(id))
            .map(|(child, _)| FolderItem::Folder(*child));
        events.extend(
            child_folders
                .chain(self.items_in_folder(folder.kind, id))
                .map(|item| ControlEvent::MoveToFolder {
                    item,
                    folder: Some(id),
                }),
        );
        if clash {
            events.push(ControlEvent::RenameFolder(id, folder.name.clone()));
        }
        Ok(ControlEvent::Transaction(events))
    }

    /// Restores the folder invariants on external input (a loaded showfile):
    /// folders with a missing or other-kind parent, or caught in a parent
    /// cycle, move to the root; items pointing at a missing or other-kind
    /// folder move to the root. Returns how many references were fixed.
    ///
    /// Empty and duplicate sibling names are only logged
    /// ([`Self::folder_name_problems`]), not renamed.
    pub fn repair_folders(&mut self) -> usize {
        let mut fixed = 0;

        let ids: Vec<FolderId> = self.folders.keys().copied().collect();
        for id in ids {
            let folder = &self.folders[&id];
            let Some(parent) = folder.parent else {
                continue;
            };
            let parent_ok = self.folders.get(&parent).map(|p| p.kind) == Some(folder.kind)
                && !is_ancestor_or_self(&self.folders, id, parent);
            if !parent_ok {
                self.folders.get_mut(&id).unwrap().parent = None;
                fixed += 1;
            }
        }

        let folders = &self.folders;
        let mut check = |folder: &mut Option<FolderId>, kind: FolderKind| {
            if let Some(id) = *folder
                && folders.get(&id).map(|f| f.kind) != Some(kind)
            {
                *folder = None;
                fixed += 1;
            }
        };
        for group in self.groups.values_mut() {
            check(&mut group.folder, FolderKind::Groups);
        }
        for scene in self.scenes.values_mut() {
            check(&mut scene.folder, FolderKind::Scenes);
        }
        for palette in self.palettes.values_mut() {
            check(&mut palette.folder, FolderKind::Palettes);
        }

        for problem in self.folder_name_problems() {
            tracing::warn!("Showfile: {problem}; left as is");
        }

        fixed
    }

    /// Folder names that break the name rules: empty names, and names that
    /// sibling folders share. One message per empty folder and per set of
    /// equally named siblings.
    pub fn folder_name_problems(&self) -> Vec<String> {
        let mut problems = Vec::new();
        let mut siblings: HashMap<(FolderKind, Option<FolderId>, String), Vec<FolderId>> =
            HashMap::new();
        for (id, folder) in &self.folders {
            if folder.name.trim().is_empty() {
                problems.push(format!("folder {id} has an empty name"));
                continue;
            }
            siblings
                .entry((
                    folder.kind,
                    folder.parent,
                    folder.name.trim().to_lowercase(),
                ))
                .or_default()
                .push(*id);
        }
        let mut duplicates: Vec<_> = siblings
            .into_iter()
            .filter(|(_, ids)| ids.len() > 1)
            .collect();
        duplicates.sort();
        for ((kind, _, name), ids) in duplicates {
            problems.push(format!(
                "sibling {kind:?} folders {ids:?} share the name {name:?}"
            ));
        }
        problems
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        HSVColor,
        fixture::state::FixtureGroup,
        palette::{Palette, PaletteKind},
        scene::{EngineSink, Scene},
    };

    fn group(folder: Option<FolderId>) -> FixtureGroup {
        FixtureGroup {
            folder,
            ..FixtureGroup::default()
        }
    }

    fn scene(name: &str) -> Scene {
        Scene {
            sink: EngineSink::from_groups(&BTreeMap::new()),
            name: name.to_string(),
            folder: None,
        }
    }

    fn palette() -> Palette {
        Palette {
            name: "p".to_string(),
            kind: PaletteKind::Color(HSVColor::default()),
            folder: None,
        }
    }

    fn folder(name: &str, parent: Option<FolderId>, kind: FolderKind) -> Folder {
        Folder {
            name: name.into(),
            parent,
            kind,
            order: 0,
        }
    }

    fn engine() -> EngineState {
        let mut state = EngineState {
            groups: (0..4).map(|id| (id, group(None))).collect(),
            scenes: BTreeMap::from([(0, scene("a")), (1, scene("b"))]),
            palettes: BTreeMap::from([(0, palette()), (1, palette())]),
            ..EngineState::default()
        };
        state.ensure_blank_scene();
        state
    }

    /// Creates a folder with the id a sender would pick.
    fn create(
        state: &mut EngineState,
        kind: FolderKind,
        parent: Option<FolderId>,
        name: &str,
    ) -> FolderId {
        let id = next_folder_id(&state.folders).unwrap();
        state.create_folder(id, kind, parent, name.into()).unwrap();
        id
    }

    /// Applies a rollback the way the DMX engine does: a `Transaction` is
    /// atomic. `DeSelectGroup` and `LimitSelectionToFixtureInCurrentGroup`
    /// are handled in `crates/engine/src/dmx/mod.rs`; their effect on the
    /// selection is mirrored here (the core test runs the real engine).
    fn apply(state: &mut EngineState, event: ControlEvent) -> Result<(), &'static str> {
        match event {
            ControlEvent::Transaction(events) => {
                let snapshot = state.clone();
                for event in events {
                    if let Err(err) = apply(state, event) {
                        *state = snapshot;
                        return Err(err);
                    }
                }
                Ok(())
            }
            ControlEvent::DeSelectGroup(gid) => {
                if !state.selection.group_ids.remove(&gid) {
                    return Err("Not selected");
                }
                state.selection.fixtures_in_group.clear();
                Ok(())
            }
            ControlEvent::LimitSelectionToFixtureInCurrentGroup(fixture) => {
                assert_eq!(state.selection.group_ids.len(), 1);
                state.selection.fixtures_in_group.insert(fixture);
                Ok(())
            }
            event => state.apply_folder_event(event).map(|_| ()),
        }
    }

    /// Asserts that two states are equal: the selection as sets (its hash
    /// sets have no stable order), everything else through its bincode
    /// encoding.
    fn assert_same_state(expected: &EngineState, actual: &EngineState) {
        assert_eq!(expected.folders, actual.folders);
        assert_eq!(expected.selection.group_ids, actual.selection.group_ids);
        assert_eq!(
            expected.selection.fixtures_in_group,
            actual.selection.fixtures_in_group
        );
        let encode = |state: &EngineState| {
            let mut state = state.clone();
            state.selection = Default::default();
            bincode::encode_to_vec(&state, bincode::config::standard()).unwrap()
        };
        assert!(encode(expected) == encode(actual), "states differ");
    }

    /// Applies `event`, checks that its rollback matches the predicted one,
    /// applies the rollback and checks that the state is back where it was.
    fn round_trip(state: &mut EngineState, event: ControlEvent) -> ControlEvent {
        let before = state.clone();
        let predicted = state.folder_rollback(&event).unwrap();
        let rollback = state.apply_folder_event(event.clone()).unwrap();
        assert_eq!(format!("{predicted:?}"), format!("{rollback:?}"));
        assert!(
            format!("{before:?}") != format!("{state:?}"),
            "{event:?} changed nothing"
        );
        apply(state, rollback.clone()).unwrap();
        assert_same_state(&before, state);
        rollback
    }

    /// Asserts that `event` fails with `err` and leaves the state unchanged.
    fn rejected(state: &mut EngineState, event: ControlEvent, err: &str) {
        let before = state.clone();
        assert_eq!(state.apply_folder_event(event).err(), Some(err));
        assert_same_state(&before, state);
    }

    #[test]
    fn create_uses_the_given_id_and_checks_id_parent_and_name() {
        let mut state = engine();
        state
            .create_folder(5, FolderKind::Groups, None, "Front".into())
            .unwrap();
        state
            .create_folder(2, FolderKind::Groups, Some(5), "  Wash  ".into())
            .unwrap();
        assert_eq!(
            state.folders[&2],
            folder("Wash", Some(5), FolderKind::Groups)
        );

        let create = |id, kind, parent, name: &str| ControlEvent::CreateFolder {
            id,
            kind,
            parent,
            name: name.into(),
        };
        let groups = FolderKind::Groups;
        rejected(
            &mut state,
            create(5, groups, None, "Other"),
            "Folder id already taken",
        );
        rejected(
            &mut state,
            create(7, groups, Some(9), "x"),
            "Folder not found",
        );
        rejected(
            &mut state,
            create(7, FolderKind::Scenes, Some(5), "x"),
            "Folder holds a different kind of item",
        );
        rejected(
            &mut state,
            create(7, groups, None, ""),
            "Folder name is empty",
        );
        rejected(
            &mut state,
            create(7, groups, None, " \t "),
            "Folder name is empty",
        );
        rejected(
            &mut state,
            create(7, groups, None, " FRONT "),
            "A sibling folder already has this name",
        );
        rejected(
            &mut state,
            create(7, groups, Some(5), "wash"),
            "A sibling folder already has this name",
        );
        assert_eq!(state.folders.len(), 2);

        // The same name is fine under another parent or for another kind.
        state
            .create_folder(7, groups, Some(2), "Front".into())
            .unwrap();
        state
            .create_folder(8, FolderKind::Scenes, None, "Front".into())
            .unwrap();
        assert_eq!(next_folder_id(&state.folders), Some(0));
    }

    #[test]
    fn rename_trims_and_checks_the_name() {
        let mut state = engine();
        let id = create(&mut state, FolderKind::Palettes, None, "Old");
        create(&mut state, FolderKind::Palettes, None, "Taken");
        let child = create(&mut state, FolderKind::Palettes, Some(id), "Child");

        state.rename_folder(id, "  New ".into()).unwrap();
        assert_eq!(state.folders[&id].name, "New");
        // A folder does not collide with itself: changing the case is fine.
        state.rename_folder(id, "NEW".into()).unwrap();
        assert_eq!(state.folders[&id].name, "NEW");
        // A child may share its parent's name.
        state.rename_folder(child, "new".into()).unwrap();

        rejected(
            &mut state,
            ControlEvent::RenameFolder(id, "taken".into()),
            "A sibling folder already has this name",
        );
        rejected(
            &mut state,
            ControlEvent::RenameFolder(id, "  ".into()),
            "Folder name is empty",
        );
        rejected(
            &mut state,
            ControlEvent::RenameFolder(77, "x".into()),
            "Folder not found",
        );
    }

    #[test]
    fn siblings_sort_by_order_then_name() {
        let mut state = engine();
        let groups = FolderKind::Groups;
        let b = create(&mut state, groups, None, "beta");
        let a = create(&mut state, groups, None, "Alpha");
        let c = create(&mut state, groups, None, "charlie");
        let inner = create(&mut state, groups, Some(b), "inner");
        let scenes = create(&mut state, FolderKind::Scenes, None, "Aaa");

        // All at order 0: by name, ignoring case.
        assert_eq!(sorted_children(&state.folders, groups, None), [a, b, c]);
        state.reorder_folder(c, 0).unwrap();
        state.reorder_folder(a, 2).unwrap();
        state.reorder_folder(b, 1).unwrap();
        assert_eq!(sorted_children(&state.folders, groups, None), [c, b, a]);
        assert_eq!(sorted_children(&state.folders, groups, Some(b)), [inner]);
        assert_eq!(
            sorted_children(&state.folders, FolderKind::Scenes, None),
            [scenes]
        );
        assert!(sorted_children(&state.folders, groups, Some(inner)).is_empty());

        // Equal order and name (an old showfile): the id breaks the tie.
        state.folders.insert(9, folder("Charlie", None, groups));
        assert_eq!(sorted_children(&state.folders, groups, None), [c, 9, b, a]);

        // All folders of one kind, siblings in the same relative order.
        assert_eq!(
            sorted_folder_ids(&state.folders, groups),
            [c, 9, inner, b, a]
        );

        rejected(
            &mut state,
            ControlEvent::ReorderFolder(99, 1),
            "Folder not found",
        );
    }

    #[test]
    fn showfile_folders_without_order_load() {
        let folder: Folder =
            serde_json::from_str(r#"{"name":"Front","parent":null,"kind":"Groups"}"#).unwrap();
        assert_eq!(folder.order, 0);
        assert_eq!(folder.name, "Front");
    }

    #[test]
    fn move_items_into_folders_of_their_kind_only() {
        let mut state = engine();
        let groups = create(&mut state, FolderKind::Groups, None, "G");
        let scenes = create(&mut state, FolderKind::Scenes, None, "S");
        let palettes = create(&mut state, FolderKind::Palettes, None, "P");

        state
            .move_to_folder(FolderItem::Group(2), Some(groups))
            .unwrap();
        state
            .move_to_folder(FolderItem::Scene(1), Some(scenes))
            .unwrap();
        state
            .move_to_folder(FolderItem::Palette(0), Some(palettes))
            .unwrap();
        assert_eq!(state.groups[&2].folder, Some(groups));
        assert_eq!(state.scenes[&1].folder, Some(scenes));
        assert_eq!(state.palettes[&0].folder, Some(palettes));

        // Back to the root.
        state.move_to_folder(FolderItem::Group(2), None).unwrap();
        assert_eq!(state.groups[&2].folder, None);

        assert_eq!(
            state.move_to_folder(FolderItem::Group(2), Some(scenes)),
            Err("Folder holds a different kind of item")
        );
        assert_eq!(
            state.move_to_folder(FolderItem::Folder(scenes), Some(groups)),
            Err("Folder holds a different kind of item")
        );
        assert_eq!(
            state.move_to_folder(FolderItem::Group(2), Some(99)),
            Err("Folder not found")
        );
        assert_eq!(
            state.move_to_folder(FolderItem::Group(42), Some(groups)),
            Err("Item not found")
        );
        assert_eq!(
            state.move_to_folder(FolderItem::Folder(99), None),
            Err("Folder not found")
        );
        assert_eq!(
            state.move_to_folder(FolderItem::Scene(BLANK_SCENE_ID), Some(scenes)),
            Err("Item not found")
        );
        assert_eq!(state.scenes[&BLANK_SCENE_ID].folder, None);
    }

    #[test]
    fn moving_a_folder_next_to_a_namesake_is_rejected() {
        let mut state = engine();
        let a = create(&mut state, FolderKind::Groups, None, "A");
        let inner = create(&mut state, FolderKind::Groups, Some(a), "a");
        rejected(
            &mut state,
            ControlEvent::MoveToFolder {
                item: FolderItem::Folder(inner),
                folder: None,
            },
            "A sibling folder already has this name",
        );
        // Staying where it is does not clash with itself.
        state
            .move_to_folder(FolderItem::Folder(inner), Some(a))
            .unwrap();
    }

    #[test]
    fn moving_a_folder_into_itself_or_a_descendant_is_rejected() {
        let mut state = engine();
        let a = create(&mut state, FolderKind::Groups, None, "a");
        let b = create(&mut state, FolderKind::Groups, Some(a), "b");
        let c = create(&mut state, FolderKind::Groups, Some(b), "c");
        let other = create(&mut state, FolderKind::Groups, None, "other");

        for target in [a, b, c] {
            assert_eq!(
                state.move_to_folder(FolderItem::Folder(a), Some(target)),
                Err("Folder cannot move into itself or a subfolder")
            );
        }
        assert_eq!(state.folders[&a].parent, None);

        // Moving down a sibling branch or up the tree is fine.
        state
            .move_to_folder(FolderItem::Folder(a), Some(other))
            .unwrap();
        state
            .move_to_folder(FolderItem::Folder(c), Some(a))
            .unwrap();
        state.move_to_folder(FolderItem::Folder(c), None).unwrap();
        assert_eq!(state.folders[&a].parent, Some(other));
        assert_eq!(state.folders[&c].parent, None);
    }

    #[test]
    fn delete_moves_children_and_items_to_the_parent() {
        let mut state = engine();
        let top = create(&mut state, FolderKind::Groups, None, "top");
        let mid = create(&mut state, FolderKind::Groups, Some(top), "mid");
        let leaf = create(&mut state, FolderKind::Groups, Some(mid), "leaf");
        state
            .move_to_folder(FolderItem::Group(0), Some(mid))
            .unwrap();
        state
            .move_to_folder(FolderItem::Group(1), Some(leaf))
            .unwrap();

        state.delete_folder(mid).unwrap();
        assert!(!state.folders.contains_key(&mid));
        assert_eq!(state.folders[&leaf].parent, Some(top));
        assert_eq!(state.groups[&0].folder, Some(top));
        assert_eq!(state.groups[&1].folder, Some(leaf));
        // No group was deleted.
        assert_eq!(state.groups.len(), 4);

        // A root folder's children land at the root.
        state.delete_folder(top).unwrap();
        assert_eq!(state.folders[&leaf].parent, None);
        assert_eq!(state.groups[&0].folder, None);

        assert_eq!(state.delete_folder(top), Err("Folder not found"));
    }

    #[test]
    fn delete_reparents_scenes_and_palettes() {
        let mut state = engine();
        let songs = create(&mut state, FolderKind::Scenes, None, "songs");
        let act = create(&mut state, FolderKind::Scenes, Some(songs), "act 1");
        let colors = create(&mut state, FolderKind::Palettes, None, "colors");
        state
            .move_to_folder(FolderItem::Scene(0), Some(act))
            .unwrap();
        state
            .move_to_folder(FolderItem::Palette(0), Some(colors))
            .unwrap();

        state.delete_folder(act).unwrap();
        assert_eq!(state.scenes[&0].folder, Some(songs));
        state.delete_folder(colors).unwrap();
        assert_eq!(state.palettes[&0].folder, None);
        assert_eq!(state.palettes.len(), 2);
    }

    /// A groups tree `Top/{Mid/{Leaf}, Side}` plus a scenes and a palettes
    /// folder, with items in most of them. `Mid` has order 3.
    fn tree() -> (EngineState, [FolderId; 6]) {
        let mut state = engine();
        let top = create(&mut state, FolderKind::Groups, None, "Top");
        let mid = create(&mut state, FolderKind::Groups, Some(top), "Mid");
        let leaf = create(&mut state, FolderKind::Groups, Some(mid), "Leaf");
        let side = create(&mut state, FolderKind::Groups, Some(top), "Side");
        let songs = create(&mut state, FolderKind::Scenes, None, "Songs");
        let colors = create(&mut state, FolderKind::Palettes, None, "Colors");
        for (item, target) in [
            (FolderItem::Group(0), mid),
            (FolderItem::Group(1), mid),
            (FolderItem::Group(2), leaf),
            (FolderItem::Group(3), top),
            (FolderItem::Scene(1), songs),
            (FolderItem::Palette(1), colors),
        ] {
            state.move_to_folder(item, Some(target)).unwrap();
        }
        state.reorder_folder(mid, 3).unwrap();
        (state, [top, mid, leaf, side, songs, colors])
    }

    #[test]
    fn every_folder_event_round_trips_through_its_rollback() {
        let (mut state, [top, mid, leaf, side, songs, colors]) = tree();

        let rollback = round_trip(
            &mut state,
            ControlEvent::CreateFolder {
                id: 40,
                kind: FolderKind::Groups,
                parent: Some(mid),
                name: "New".into(),
            },
        );
        assert!(matches!(rollback, ControlEvent::DeleteFolder(40)));

        round_trip(&mut state, ControlEvent::RenameFolder(mid, "Middle".into()));
        round_trip(&mut state, ControlEvent::ReorderFolder(mid, 0));
        round_trip(&mut state, ControlEvent::ReorderFolder(side, 9));

        for (item, target) in [
            (FolderItem::Group(0), Some(leaf)),
            (FolderItem::Group(0), None),
            (FolderItem::Group(3), Some(side)),
            (FolderItem::Scene(0), Some(songs)),
            (FolderItem::Scene(1), None),
            (FolderItem::Palette(0), Some(colors)),
            (FolderItem::Folder(leaf), None),
            (FolderItem::Folder(leaf), Some(side)),
            (FolderItem::Folder(mid), Some(side)),
            (FolderItem::Folder(side), None),
        ] {
            round_trip(
                &mut state,
                ControlEvent::MoveToFolder {
                    item,
                    folder: target,
                },
            );
        }

        // A folder with nested child folders and items of its kind and a
        // non-zero order, inside another folder.
        round_trip(&mut state, ControlEvent::DeleteFolder(mid));
        // A root folder, folders of the other kinds, and a leaf folder.
        round_trip(&mut state, ControlEvent::DeleteFolder(top));
        round_trip(&mut state, ControlEvent::DeleteFolder(songs));
        round_trip(&mut state, ControlEvent::DeleteFolder(colors));
        round_trip(&mut state, ControlEvent::DeleteFolder(leaf));

        // Selecting a folder, with another group and a fixture filter
        // already selected.
        state.selection.group_ids.insert(3);
        state.selection.fixtures_in_group.extend([4, 1]);
        let rollback = round_trip(&mut state, ControlEvent::SelectFolder(mid));
        assert_eq!(
            format!("{rollback:?}"),
            format!(
                "{:?}",
                ControlEvent::Transaction(vec![
                    ControlEvent::DeSelectGroup(0),
                    ControlEvent::DeSelectGroup(1),
                    ControlEvent::DeSelectGroup(2),
                    ControlEvent::LimitSelectionToFixtureInCurrentGroup(1),
                    ControlEvent::LimitSelectionToFixtureInCurrentGroup(4),
                ])
            )
        );
        // Only the groups it actually adds are deselected again.
        state.selection.fixtures_in_group.clear();
        state.selection.group_ids.insert(1);
        let rollback = round_trip(&mut state, ControlEvent::SelectFolder(top));
        assert_eq!(
            format!("{rollback:?}"),
            format!(
                "{:?}",
                ControlEvent::Transaction(vec![
                    ControlEvent::DeSelectGroup(0),
                    ControlEvent::DeSelectGroup(2),
                ])
            )
        );
    }

    #[test]
    fn delete_rollback_recreates_the_folder_and_moves_its_children_back() {
        let (state, [top, mid, leaf, ..]) = tree();
        let rollback = state.folder_rollback(&ControlEvent::DeleteFolder(mid));
        assert_eq!(
            format!("{rollback:?}"),
            format!(
                "{:?}",
                Ok::<_, &str>(ControlEvent::Transaction(vec![
                    ControlEvent::CreateFolder {
                        id: mid,
                        kind: FolderKind::Groups,
                        parent: Some(top),
                        name: "Mid".into(),
                    },
                    ControlEvent::ReorderFolder(mid, 3),
                    ControlEvent::MoveToFolder {
                        item: FolderItem::Folder(leaf),
                        folder: Some(mid),
                    },
                    ControlEvent::MoveToFolder {
                        item: FolderItem::Group(0),
                        folder: Some(mid),
                    },
                    ControlEvent::MoveToFolder {
                        item: FolderItem::Group(1),
                        folder: Some(mid),
                    },
                ]))
            )
        );
    }

    #[test]
    fn delete_rollback_survives_a_child_with_the_folder_name() {
        let mut state = engine();
        let outer = create(&mut state, FolderKind::Groups, None, "Front");
        let inner = create(&mut state, FolderKind::Groups, Some(outer), "front");
        // A sibling already uses the first temporary name.
        create(&mut state, FolderKind::Groups, None, "Front (1)");
        state
            .move_to_folder(FolderItem::Group(0), Some(inner))
            .unwrap();

        let rollback = round_trip(&mut state, ControlEvent::DeleteFolder(outer));
        let ControlEvent::Transaction(events) = rollback else {
            panic!("expected a transaction");
        };
        assert!(matches!(
            &events[0],
            ControlEvent::CreateFolder { name, .. } if name == "Front (2)"
        ));
        assert!(matches!(
            events.last(),
            Some(ControlEvent::RenameFolder(id, name)) if *id == outer && name == "Front"
        ));
    }

    #[test]
    fn failed_events_have_no_rollback_and_change_nothing() {
        let (mut state, [top, mid, _, _, songs, _]) = tree();
        assert_eq!(
            state
                .folder_rollback(&ControlEvent::RenameFolder(99, "x".into()))
                .err(),
            Some("Folder not found")
        );
        assert_eq!(
            state
                .folder_rollback(&ControlEvent::MoveToFolder {
                    item: FolderItem::Group(42),
                    folder: None,
                })
                .err(),
            Some("Item not found")
        );
        assert_eq!(
            state.folder_rollback(&ControlEvent::SelectGroup(0)).err(),
            Some("Not a folder event")
        );
        rejected(
            &mut state,
            ControlEvent::SelectGroup(0),
            "Not a folder event",
        );
        rejected(
            &mut state,
            ControlEvent::DeleteFolder(99),
            "Folder not found",
        );
        rejected(
            &mut state,
            ControlEvent::MoveToFolder {
                item: FolderItem::Folder(top),
                folder: Some(mid),
            },
            "Folder cannot move into itself or a subfolder",
        );
        rejected(
            &mut state,
            ControlEvent::SelectFolder(songs),
            "Not a groups folder",
        );
        state.select_folder(top).unwrap();
        rejected(
            &mut state,
            ControlEvent::SelectFolder(mid),
            "Already selected",
        );
    }

    #[test]
    fn folder_group_ids_is_recursive() {
        let mut state = engine();
        let all = create(&mut state, FolderKind::Groups, None, "all");
        let front = create(&mut state, FolderKind::Groups, Some(all), "front");
        let deep = create(&mut state, FolderKind::Groups, Some(front), "deep");
        let empty = create(&mut state, FolderKind::Groups, Some(all), "empty");
        let scenes = create(&mut state, FolderKind::Scenes, None, "s");
        state
            .move_to_folder(FolderItem::Group(0), Some(all))
            .unwrap();
        state
            .move_to_folder(FolderItem::Group(1), Some(front))
            .unwrap();
        state
            .move_to_folder(FolderItem::Group(2), Some(deep))
            .unwrap();
        // Group 3 stays at the root.

        assert_eq!(state.folder_group_ids(all), BTreeSet::from([0, 1, 2]));
        assert_eq!(state.folder_group_ids(front), BTreeSet::from([1, 2]));
        assert_eq!(state.folder_group_ids(deep), BTreeSet::from([2]));
        assert!(state.folder_group_ids(empty).is_empty());
        assert!(state.folder_group_ids(scenes).is_empty());
        assert!(state.folder_group_ids(99).is_empty());
    }

    #[test]
    fn select_folder_adds_its_groups_to_the_selection() {
        let mut state = engine();
        let all = create(&mut state, FolderKind::Groups, None, "all");
        let front = create(&mut state, FolderKind::Groups, Some(all), "front");
        let empty = create(&mut state, FolderKind::Groups, None, "empty");
        let scenes = create(&mut state, FolderKind::Scenes, None, "s");
        state
            .move_to_folder(FolderItem::Group(1), Some(all))
            .unwrap();
        state
            .move_to_folder(FolderItem::Group(2), Some(front))
            .unwrap();

        // An existing single-group selection with a fixture filter.
        state.selection.group_ids.insert(3);
        state.selection.fixtures_in_group.insert(0);

        state.select_folder(all).unwrap();
        assert_eq!(
            state.selection.group_ids,
            HashSet::from([1, 2, 3]),
            "folder groups are added to the selection"
        );
        assert!(state.selection.fixtures_in_group.is_empty());

        assert_eq!(state.select_folder(front), Err("Already selected"));
        assert_eq!(state.select_folder(empty), Err("Folder contains no groups"));
        assert_eq!(state.select_folder(scenes), Err("Not a groups folder"));
        assert_eq!(state.select_folder(99), Err("Folder not found"));
    }

    #[test]
    fn folder_events_keep_existing_bincode_indices_and_round_trip() {
        // Appended variants must not shift existing indices; index 0 is the
        // first variant, `SelectGroup`.
        assert_eq!(ControlEvent::SelectGroup(5).serialize()[0], 0);

        let events = [
            ControlEvent::CreateFolder {
                id: 9,
                kind: FolderKind::Scenes,
                parent: Some(3),
                name: "Act 1".into(),
            },
            ControlEvent::RenameFolder(3, "Act 2".into()),
            ControlEvent::DeleteFolder(3),
            ControlEvent::MoveToFolder {
                item: FolderItem::Folder(4),
                folder: None,
            },
            ControlEvent::SelectFolder(7),
            ControlEvent::ReorderFolder(7, 2),
        ];
        for event in events {
            let decoded = ControlEvent::deserialize(&event.serialize());
            assert_eq!(format!("{decoded:?}"), format!("{event:?}"));
        }

        // The JSON shapes blctl / the inspector accept.
        let json = r#"{"MoveToFolder":{"item":{"Group":2},"folder":5}}"#;
        let event: ControlEvent = serde_json::from_str(json).unwrap();
        assert!(matches!(
            event,
            ControlEvent::MoveToFolder {
                item: FolderItem::Group(2),
                folder: Some(5)
            }
        ));
        let json = r#"{"CreateFolder":{"id":4,"kind":"Groups","parent":null,"name":"Front"}}"#;
        let event: ControlEvent = serde_json::from_str(json).unwrap();
        assert!(matches!(
            event,
            ControlEvent::CreateFolder {
                id: 4,
                kind: FolderKind::Groups,
                parent: None,
                ..
            }
        ));
        let event: ControlEvent = serde_json::from_str(r#"{"ReorderFolder":[4,2]}"#).unwrap();
        assert!(matches!(event, ControlEvent::ReorderFolder(4, 2)));
    }

    #[test]
    fn helpers_terminate_on_a_cyclic_parent_chain() {
        let mut state = engine();
        let kind = FolderKind::Groups;
        state.folders.insert(0, folder("a", Some(1), kind));
        state.folders.insert(1, folder("b", Some(0), kind));
        state.groups.get_mut(&0).unwrap().folder = Some(1);

        assert_eq!(folder_subtree(&state.folders, 0), BTreeSet::from([0, 1]));
        assert!(!is_ancestor_or_self(&state.folders, 5, 0));
        assert_eq!(state.folder_group_ids(0), BTreeSet::from([0]));
        assert!(sorted_children(&state.folders, kind, None).is_empty());
        assert_eq!(sorted_folder_ids(&state.folders, kind), [0, 1]);
    }

    #[test]
    fn repair_folders_fixes_broken_external_input() {
        let mut state = engine();
        let f = |parent, kind| folder("f", parent, kind);
        // 0 <-> 1 is a cycle, 2 points at a missing parent, 3 at a parent of
        // another kind, 4 is fine.
        state.folders = BTreeMap::from([
            (0, f(Some(1), FolderKind::Groups)),
            (1, f(Some(0), FolderKind::Groups)),
            (2, f(Some(50), FolderKind::Groups)),
            (3, f(Some(5), FolderKind::Groups)),
            (4, f(Some(0), FolderKind::Groups)),
            (5, f(None, FolderKind::Scenes)),
        ]);
        state.groups.get_mut(&0).unwrap().folder = Some(4); // fine
        state.groups.get_mut(&1).unwrap().folder = Some(5); // wrong kind
        state.scenes.get_mut(&0).unwrap().folder = Some(77); // missing
        state.palettes.get_mut(&0).unwrap().folder = Some(0); // wrong kind

        assert_eq!(state.repair_folders(), 6);

        // Exactly one edge of the cycle is cut.
        assert_eq!(state.folders[&0].parent, None);
        assert_eq!(state.folders[&1].parent, Some(0));
        assert_eq!(state.folders[&2].parent, None);
        assert_eq!(state.folders[&3].parent, None);
        assert_eq!(state.folders[&4].parent, Some(0));
        assert_eq!(state.groups[&0].folder, Some(4));
        assert_eq!(state.groups[&1].folder, None);
        assert_eq!(state.scenes[&0].folder, None);
        assert_eq!(state.palettes[&0].folder, None);

        // Idempotent.
        assert_eq!(state.repair_folders(), 0);
    }

    #[test]
    fn repair_folders_only_reports_bad_names() {
        let mut state = engine();
        let groups = FolderKind::Groups;
        state.folders = BTreeMap::from([
            (0, folder("Front", None, groups)),
            (1, folder(" front", None, groups)),
            (2, folder("FRONT", None, groups)),
            (3, folder("Front", Some(0), groups)),
            (4, folder("Front", None, FolderKind::Scenes)),
            (5, folder("  ", None, groups)),
        ]);
        let before = state.folders.clone();

        assert_eq!(
            state.folder_name_problems(),
            [
                "folder 5 has an empty name",
                "sibling Groups folders [0, 1, 2] share the name \"front\"",
            ]
        );
        assert_eq!(state.repair_folders(), 0);
        assert_eq!(state.folders, before, "names are not changed");
    }
}
