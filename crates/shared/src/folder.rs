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
//! The read helpers do not rely on these invariants, so a broken state can
//! never make them loop or panic.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};

use crate::{EngineState, scene::BLANK_SCENE_ID};

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

/// Lowest unused folder id, or `None` if all `u16` ids are taken.
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

    /// Creates a folder of `kind` under `parent` (`None` = root) and returns
    /// its id, which is always the lowest free one ([`next_folder_id`]).
    pub fn create_folder(
        &mut self,
        kind: FolderKind,
        parent: Option<FolderId>,
        name: String,
    ) -> Result<FolderId, &'static str> {
        self.check_folder_target(kind, parent)?;
        let id = next_folder_id(&self.folders).ok_or("Max folders reached")?;
        self.folders.insert(id, Folder { name, parent, kind });
        Ok(id)
    }

    pub fn rename_folder(&mut self, id: FolderId, name: String) -> Result<(), &'static str> {
        let folder = self.folders.get_mut(&id).ok_or("Folder not found")?;
        folder.name = name;
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
    /// of its descendants.
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
        if let (FolderItem::Folder(moved), Some(target)) = (item, folder)
            && is_ancestor_or_self(&self.folders, moved, target)
        {
            return Err("Folder cannot move into itself or a subfolder");
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

    /// Restores the folder invariants on external input (a loaded showfile):
    /// folders with a missing or other-kind parent, or caught in a parent
    /// cycle, move to the root; items pointing at a missing or other-kind
    /// folder move to the root. Returns how many references were fixed.
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

        fixed
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

    fn engine() -> EngineState {
        let mut state = EngineState {
            groups: (0..4).map(|id| (id, group(None))).collect(),
            scenes: BTreeMap::from([(0, scene("a")), (1, scene("b"))]),
            palettes: BTreeMap::from([(0, palette())]),
            ..EngineState::default()
        };
        state.ensure_blank_scene();
        state
    }

    #[test]
    fn create_allocates_the_lowest_free_id_and_checks_the_parent() {
        let mut state = engine();
        let root = state
            .create_folder(FolderKind::Groups, None, "Front".into())
            .unwrap();
        assert_eq!(root, 0);
        let child = state
            .create_folder(FolderKind::Groups, Some(root), "Wash".into())
            .unwrap();
        assert_eq!(child, 1);
        assert_eq!(
            state.folders[&child],
            Folder {
                name: "Wash".into(),
                parent: Some(root),
                kind: FolderKind::Groups,
            }
        );

        assert_eq!(
            state.create_folder(FolderKind::Groups, Some(9), "x".into()),
            Err("Folder not found")
        );
        assert_eq!(
            state.create_folder(FolderKind::Scenes, Some(root), "x".into()),
            Err("Folder holds a different kind of item")
        );
        assert_eq!(state.folders.len(), 2);

        // Freed ids are reused.
        state.delete_folder(root).unwrap();
        assert_eq!(
            state.create_folder(FolderKind::Scenes, None, "Songs".into()),
            Ok(0)
        );
    }

    #[test]
    fn rename_folder() {
        let mut state = engine();
        let id = state
            .create_folder(FolderKind::Palettes, None, "Old".into())
            .unwrap();
        state.rename_folder(id, "New".into()).unwrap();
        assert_eq!(state.folders[&id].name, "New");
        assert_eq!(state.rename_folder(7, "x".into()), Err("Folder not found"));
    }

    #[test]
    fn move_items_into_folders_of_their_kind_only() {
        let mut state = engine();
        let groups = state
            .create_folder(FolderKind::Groups, None, "G".into())
            .unwrap();
        let scenes = state
            .create_folder(FolderKind::Scenes, None, "S".into())
            .unwrap();
        let palettes = state
            .create_folder(FolderKind::Palettes, None, "P".into())
            .unwrap();

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
    fn moving_a_folder_into_itself_or_a_descendant_is_rejected() {
        let mut state = engine();
        let a = state
            .create_folder(FolderKind::Groups, None, "a".into())
            .unwrap();
        let b = state
            .create_folder(FolderKind::Groups, Some(a), "b".into())
            .unwrap();
        let c = state
            .create_folder(FolderKind::Groups, Some(b), "c".into())
            .unwrap();
        let other = state
            .create_folder(FolderKind::Groups, None, "other".into())
            .unwrap();

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
        let top = state
            .create_folder(FolderKind::Groups, None, "top".into())
            .unwrap();
        let mid = state
            .create_folder(FolderKind::Groups, Some(top), "mid".into())
            .unwrap();
        let leaf = state
            .create_folder(FolderKind::Groups, Some(mid), "leaf".into())
            .unwrap();
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
        let songs = state
            .create_folder(FolderKind::Scenes, None, "songs".into())
            .unwrap();
        let act = state
            .create_folder(FolderKind::Scenes, Some(songs), "act 1".into())
            .unwrap();
        let colors = state
            .create_folder(FolderKind::Palettes, None, "colors".into())
            .unwrap();
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
        assert_eq!(state.palettes.len(), 1);
    }

    #[test]
    fn folder_group_ids_is_recursive() {
        let mut state = engine();
        let all = state
            .create_folder(FolderKind::Groups, None, "all".into())
            .unwrap();
        let front = state
            .create_folder(FolderKind::Groups, Some(all), "front".into())
            .unwrap();
        let deep = state
            .create_folder(FolderKind::Groups, Some(front), "deep".into())
            .unwrap();
        let empty = state
            .create_folder(FolderKind::Groups, Some(all), "empty".into())
            .unwrap();
        let scenes = state
            .create_folder(FolderKind::Scenes, None, "s".into())
            .unwrap();
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
        let all = state
            .create_folder(FolderKind::Groups, None, "all".into())
            .unwrap();
        let front = state
            .create_folder(FolderKind::Groups, Some(all), "front".into())
            .unwrap();
        let empty = state
            .create_folder(FolderKind::Groups, None, "empty".into())
            .unwrap();
        let scenes = state
            .create_folder(FolderKind::Scenes, None, "s".into())
            .unwrap();
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
        use crate::ControlEvent;

        // Appended variants must not shift existing indices; index 0 is the
        // first variant, `SelectGroup`.
        assert_eq!(ControlEvent::SelectGroup(5).serialize()[0], 0);

        let events = [
            ControlEvent::CreateFolder {
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
        ];
        for event in events {
            let decoded = ControlEvent::deserialize(&event.serialize());
            assert_eq!(format!("{decoded:?}"), format!("{event:?}"));
        }

        // The JSON shape blctl / the inspector accept.
        let json = r#"{"MoveToFolder":{"item":{"Group":2},"folder":5}}"#;
        let event: ControlEvent = serde_json::from_str(json).unwrap();
        assert!(matches!(
            event,
            ControlEvent::MoveToFolder {
                item: FolderItem::Group(2),
                folder: Some(5)
            }
        ));
    }

    #[test]
    fn helpers_terminate_on_a_cyclic_parent_chain() {
        let mut state = engine();
        let kind = FolderKind::Groups;
        state.folders.insert(
            0,
            Folder {
                name: "a".into(),
                parent: Some(1),
                kind,
            },
        );
        state.folders.insert(
            1,
            Folder {
                name: "b".into(),
                parent: Some(0),
                kind,
            },
        );
        state.groups.get_mut(&0).unwrap().folder = Some(1);

        assert_eq!(folder_subtree(&state.folders, 0), BTreeSet::from([0, 1]));
        assert!(!is_ancestor_or_self(&state.folders, 5, 0));
        assert_eq!(state.folder_group_ids(0), BTreeSet::from([0]));
    }

    #[test]
    fn repair_folders_fixes_broken_external_input() {
        let mut state = engine();
        let folder = |parent, kind| Folder {
            name: "f".into(),
            parent,
            kind,
        };
        // 0 <-> 1 is a cycle, 2 points at a missing parent, 3 at a parent of
        // another kind, 4 is fine.
        state.folders = BTreeMap::from([
            (0, folder(Some(1), FolderKind::Groups)),
            (1, folder(Some(0), FolderKind::Groups)),
            (2, folder(Some(50), FolderKind::Groups)),
            (3, folder(Some(5), FolderKind::Groups)),
            (4, folder(Some(0), FolderKind::Groups)),
            (5, folder(None, FolderKind::Scenes)),
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
}
