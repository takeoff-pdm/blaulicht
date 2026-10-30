# 017 — Tree UI for group folders (Setup › Groups and Programmer)

| | |
|---|---|
| Epic | E4 Hierarchy (folders) |
| Depends on | 016, 006, 030 |
| ABI / showfile | none |

**Touches:** `crates/core/src/app/pages/setup/groups.rs`, `crates/core/src/app/components/fixtures_shared.rs (group_selection)`

## Problem
Original review notes (2026-09-27):

> fixture groups should be hierarchical

## Scope
- Use the reusable tree widget from 030 (`components/tree.rs`) for the group trees below; 018 uses it too.
- In Setup › Groups: create, rename and delete folders, and move groups into folders (drag or a "Move to…" dialog, since touch is primary).
- In the Programmer, group selection is shown as a tree. Tapping a folder selects all its groups (`SelectFolder`), and collapsed state is remembered per session.

## Out of scope
- The generic tree widget itself (030).
- Scenes and palettes (018).

## Key code
- `group_selection` — `crates/core/src/app/components/fixtures_shared.rs:577`

## Acceptance criteria
- [x] Folders can be managed on the touch layout without a keyboard, except for naming.
- [x] Selecting a folder selects the right groups (check with `blctl.py state`).

## Verification
- `cargo clippy --all-targets --all-features` and `cargo fmt` are clean.
- `cargo test --all-features` passes, and new logic has co-located `#[cfg(test)]` tests.
- Follow **UI Verification** in `crates/core/AGENTS.md` (check that `swaylock` is not running, launch from the repo root, navigate via blctl, capture with grim). Use the headless Xvfb recipe when clicks are needed or another session is running (see memory notes). Take before and after screenshots, keep them in the scratchpad, and never commit them.
- Screenshot Setup › Groups with nested folders, and the Programmer tree.

## Notes
<!-- Implementation notes, decisions, and follow-ups found while working. Keep this updated before renaming to .done.md. -->
### What changed
- **Shared helpers for 017/018** (new `crates/core/src/app/components/folder_tree.rs`, re-exported from `components`):
  - Items: `folder_tree_items::<L>(&folders, kind)` gives the folders of one kind in `sorted_folder_ids` order;
    callers append their leaves. `group_tree_items(&state)` is that plus every group as a leaf with the old tile
    detail (`"4 FX | #3"`). 018 builds scenes/palettes the same way: `folder_tree_items` + one `TreeItem::leaf`
    per scene/palette with `s.folder` as parent.
  - Dialogs: `FolderDialog` (`Closed`, `Create`, `Rename`, `Delete`, `Move`), opened with
    `FolderDialog::create(kind, parent)`, `::rename(&state, id)`, `::delete(id)`, `::move_item(&state, item)`, and
    rendered every frame with `dialog.show(ctx, &state) -> Option<ControlEvent>`. The caller sends the returned
    event (and may react to it). The dialog closes itself on confirm, Cancel or Esc, and also when its folder or
    item disappears. Everything is generic over `FolderKind` / `FolderItem`, so 018 needs no new dialog code.
  - Event builders that check input exactly like the engine, so errors show inline in the dialog instead of being
    rejected later: `create_folder_event` (id from `folder::next_folder_id`, name from `check_folder_name`),
    `rename_folder_event`, `move_to_folder_event` (kind, cycle, sibling name; a no-op move is refused).
    A test runs every item/target pair against `EngineState::move_to_folder` and asserts the same outcome.
  - "Move to…" (`FolderDialog::Move`): a "Top level" button plus the folder tree of the item's kind (rows of
    48 px, scrolls after 230 px), without the moved folder's own subtree (`move_target_items`). Tap a target,
    then "Move here"; the button is disabled for the current folder. Target rows show the folder's item count.
  - Delete confirmation names what moves where (`delete_summary`: subfolders and items, destination), and that
    no groups/scenes/palettes are deleted.
  - Programmer helpers: `folder_tap_event(&state, id)` and `fully_selected_group_folders(&state)`.
  - Narrow columns: `narrow_tree(salt, &items)` and `NARROW_TREE_WIDTH` (120 px). Labels: `kind_noun`,
    `item_name`, `folder_path` (`Stage / Front`). Salt constant `SETUP_GROUP_TREE`.
- **Tree widget** (`components/tree.rs`, 030), additive builders: `.selected_keys(iter)` for multi-selection
  (`.selected(Option)` still works), `.toggle_width(f32)`, `.min_body_width(f32)` and `.compact(bool)` (no folder
  icon, 6 px padding). `indent_width` takes the minimum body width as a fifth argument.
- **Setup › Groups** (`pages/setup/groups.rs`): `setup_group_tree` replaces `setup_group_list`. The tree has a
  "Move…" button on every row (groups and folders). The selection is a group (`add_fixture_group`, shared with
  Fixtures) or a folder (`SetupGroupsUi::folder`, new `BlaulichtApp::setup_groups_ui`). Toolbar: Add Group |
  New Folder | Rename | Delete. Rename and Delete act on the selected folder or group and are disabled with no
  selection. New groups and new folders go into the selected folder, otherwise to the top level. The right side
  shows the selected folder (name, place, direct and recursive group count, every group under it with its
  path) or the group details as before. A new folder is selected right away and its parent is expanded; a move
  target is expanded too.
- **Programmer** (`fixture_selection`): the group column is a narrow tree (`"programmer_groups"` salt, collapse
  state kept for the session). Tapping a group toggles it as before. Tapping a folder sends `SelectFolder`, or,
  if every group under it is already selected, one `Transaction` of `DeSelectGroup`s, so a folder toggles like a
  group. Folders whose groups are all selected are highlighted. Empty folders do nothing on tap. The toolbar and
  the rest of `programmer.rs` are untouched.
- **Setup › Fixtures** (`group_selection`): decision: it gets the tree too, so a show with folders looks the
  same everywhere. It shares the collapse state with Setup › Groups (`SETUP_GROUP_TREE`). A folder tap
  opens/closes the folder, since that picker selects exactly one group.
- `group_tile` was removed; nothing used it after this change.
- **Dirty flag**: `control_event_marks_showfile_dirty` treats `SelectFolder` like `SelectGroup` (not show data).
  Folder edits still mark the show dirty (test `selecting_a_folder_is_not_show_data_but_editing_folders_is`).
- No ABI bump, no showfile change, nothing in `crates/shared` touched.

### Decisions
- **Width at 800x480**: the Programmer row has about 10 px of slack. A 140 px tree pushed the masters and
  Scene Changes column off screen, so the tree column is 120 px (old tiles 90 px). There, `narrow_tree` uses a
  24 px toggle, an 80 px minimum body and 8 px indentation, so indentation clamps at 12 px (levels 1 and 2+
  differ, deeper levels line up) and long names truncate with "…". Guides and chevrons carry the nesting.
- **Folder deselect in the Programmer**: tapping a fully selected folder deselects its groups. It's not in the
  ticket text, but without it the second tap fails with "Already selected" and tiles and folders behave
  differently.
- **Folder events go through the event bus** (originator `Web`), not by writing the engine lock, so the engine
  validates them and broadcasts the rollbacks. Group create/rename/delete stay as before (direct writes);
  placing a new group in the selected folder uses `move_to_folder` under the same write lock.
- **No undo**: the app UI has no undo stack (the Visualizer's undo is local to it), so the rollbacks from
  `EngineState::folder_rollback` are not used yet. The module doc of `folder_tree.rs` says where to hook them in.
- **Names**: typed with the existing `TextInput` (the kiosk's system keyboard), as in the group dialogs. There is
  no on-screen keyboard component in the app. Enter confirms, Esc cancels.
- **No drag and drop**; "Move…" per row only, as the ticket allows.

### Verification
All cargo ran in the blenv container (`blenv-agent-a432671a60cebf66c-3427082f`), never on the host.
- `cargo clippy --workspace --all-targets --all-features`: exit 0; no warning points at `folder_tree.rs`,
  `tree.rs`, `groups.rs`, `event.rs` or the changed hunks of `fixtures_shared.rs` (its remaining warnings are on
  old lines).
- `cargo fmt --all -- --check`: only the known diff in `pages/animations.rs`.
- `cargo test --workspace --all-features`: exit 0 (core 217, shared 71, others 25 + 6). New tests: 8 in
  `app::components::folder_tree::tests`, the event dirty test, and narrow-column cases in
  `tree::tests::indent_shrinks_to_keep_body_width`.
- Plugins were not rebuilt: no shared type changed, and the v3 bundles from `blenv build all` accepted every
  folder event sent through `blctl`.
- App (blenv, a copy of `Kuze_Theater.json`, 7 groups), checked by screenshot and by `blenv blctl state` (Debug
  dump, parsed for `folders`, each group's `folder` and `selection`):
  - Before: Setup › Groups flat list, Programmer with 90 px tiles.
  - Real clicks at 800x480: New Folder with an empty name (inline "Folder name is empty"), then Stage, Front
    inside Stage and Inner inside Front (each selected after creation). Three groups moved through "Move…"
    (into Front, Inner and Stage); state matched. Rename with a clashing name (inline "A sibling folder already
    has this name"), then a valid rename. Front moved to the top level (its subtree was not offered as a target).
    Front deleted: Inner and Frontlicht moved to the top level, as the dialog said. Add Group with Inner selected
    put the new group into Inner; Delete Group still works. Esc closes the folder dialogs.
  - After a restart the tree was rebuilt with `blctl event` (CreateFolder/MoveToFolder), since the app autosaves
    into the container's copy.
  - Programmer: tapping Stage selected `{0, 1, 2}` (Saal in Stage, Frontlicht in Front, Spitzlicht in Inner);
    again: `{}`; Pars: `{4, 6, 7}`; then Stage: `{0, 1, 2, 4, 6, 7}`. A collapsed folder stayed collapsed after
    Setup -> Programmer.
  - Dirty flag: with the autosaved file (folders inside) loaded, the dirty dot was off; tapping a folder in the
    Programmer selected `{4, 6, 7}` and the dot stayed off. Folder edits turn it on.
  - Setup › Fixtures: tree at 120 px; a folder tap collapses; selecting a group loads its fixtures.
  - `--desktop`: Setup › Groups and the Programmer in Dynamic panes: tree, details, Move dialog (centred on the
    window), a move and a folder tap (`{4, 6, 7}`) all work.
- Screenshots are in `.blenv/shots/` (not committed).

### Not covered / follow-ups
- In `--desktop`, a Programmer pane in "Default (723 x 480)" mode is narrower than the kiosk page and cuts the
  right column off; the 120 px tree makes that ~30 px worse. Dynamic mode fits. Added to the Inbox.
- egui logs `Widget rect … changed id between passes` (debug builds) for tree rows right after a move: the engine
  applies the event between egui's two passes, so other rows occupy those rects. Keying rows with `push_id`
  didn't remove it, so it was left out. Harmless.
- Undo for folder edits needs an app-wide undo stack (see Decisions).
- Folder `order` (`ReorderFolder`) has no UI; siblings sort by name. Add "Move up/down" if manual order is wanted.
- The Programmer at 800x480 has little room; a later layout pass could give the group tree more width.
