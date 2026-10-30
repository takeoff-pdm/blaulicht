# 018 — Tree UI for scene and palette folders

| | |
|---|---|
| Epic | E4 Hierarchy (folders) |
| Depends on | 016, 006, 010, 017, 030 |
| ABI / showfile | none |

**Touches:** `crates/core/src/app/pages/setup/scenes.rs`, `crates/core/src/app/pages/setup/palettes.rs (setup_palette_list, since 010)`, `crates/core/src/app/components/fixtures_shared.rs (scene_overview)`, view/performance scene pickers

## Problem
Original review notes (2026-09-27):

> same for scenes
> like everything, we need hierarchies

## Scope
- Use the tree widget from 030 (`components/tree.rs`) in Setup › Scenes, Setup › Palettes, the Programmer palette-assignment dialog, and the scene/overlay pickers (views, performance).
- Add folder CRUD and move-to-folder for scenes and palettes.

## Out of scope
- New features beyond folders.

## Key code
- `scene_overview` — `fixtures_shared.rs:1142`
- `render_overlay_picker_dialog` — `crates/core/src/app/pages/view.rs`
- `render_palette_assignment_dialog` — `fixtures_perf.rs:826`

## Acceptance criteria
- [x] Every scene and palette list or picker shows folders consistently. Done for every list and picker the
  Scope names (Setup › Scenes, Setup › Palettes, the Programmer's scene column and palette dialog, the View and
  View Performance overlay pickers). Three more pickers outside the Scope still show flat lists; they are in the
  Inbox (see Notes).

## Verification
- `cargo clippy --all-targets --all-features` and `cargo fmt` are clean.
- `cargo test --all-features` passes, and new logic has co-located `#[cfg(test)]` tests.
- Follow **UI Verification** in `crates/core/AGENTS.md` (check that `swaylock` is not running, launch from the repo root, navigate via blctl, capture with grim). Use the headless Xvfb recipe when clicks are needed or another session is running (see memory notes). Take before and after screenshots, keep them in the scratchpad, and never commit them.
- Screenshot the Scenes and Palettes sub-pages and one picker dialog.

## Notes
<!-- Implementation notes, decisions, and follow-ups found while working. Keep this updated before renaming to .done.md. -->
### What changed
- **Shared helpers** (`components/folder_tree.rs`, on top of 017's):
  - `SetupFolderUi` (017's `SetupGroupsUi`, now used by Groups, Scenes and Palettes): `folder`, `dialog`,
    `drop_missing_folder(&state)` (the 2 s grace for a just-created folder) and `follow(ctx, salt, &event)` (select a
    new folder and open its parent, open a move target, deselect a deleted folder). The caller sends the event.
    `apply_group_folder_event` now calls `follow`.
  - `folder_details(ui, &state, kind, folder)` and `items_in_subtree(&state, kind, folder)`: the right-hand folder
    details of all three Setup trees. Groups shows the same text as before (`items_in_subtree` for groups is the
    same set as `folder_group_ids`; a test checks that).
  - `scene_tree_items(&state, include)`: scenes folders plus each included scene as a leaf (`#id` detail). BLANK,
    if included, is always at the top level. `palette_tree_items(&state, include, detail)`: palettes folders plus
    palettes (`#id | Color`, from `palette_kind_label`, now `pub(crate)`).
  - `prune_empty_folders(items)`: drops folders with no leaf below them (cycle-safe), for pickers.
  - `toggle_folder(ctx, salt, id)` / `expand_folder(ctx, salt, id)`. Setup › Fixtures' group picker uses
    `toggle_folder` instead of its inline copy.
  - `tree_picker_dialog(ctx, title, salt, &items, selected, empty_text, &mut open) -> Option<u8>`: pick one leaf;
    a folder tap opens or closes the folder; Close, Esc and the backdrop close it. 48 px rows, scrolls after 290 px.
  - Salts: `SETUP_SCENE_TREE`, `SETUP_PALETTE_TREE`, `SCENE_PICKER_TREE` (both overlay pickers),
    `programmer_scenes` and `palette_assign` (private).
- **Setup › Scenes** (`pages/setup/scenes.rs`): the layout of Setup › Groups. Toolbar Scene + | Clone Sc. |
  New Folder | Rename | Delete; tree with "Move…" on the left; focused scene (now with "In: <path>") or folder on
  the right. Tapping a scene sends `SetSceneFocus` (the Setup selection *is* the focused scene, as before); tapping a
  folder selects it. Rename/Delete act on the folder or the focused scene; Clone is disabled while a folder is
  selected. New scenes go into the selected folder (`move_to_folder` under the same write lock, like new groups);
  a clone lands next to its source (engine rule from 016).
- **Programmer scene column** (`scene_overview` in `components/fixtures_shared.rs`): a narrow scene tree instead of
  the 5-per-page pager. `scene_page_index` is gone.
- **Setup › Palettes** (`pages/setup/palettes.rs`): `setup_palette_list` is replaced by `setup_palette_tree` and
  `setup_palette_details`, in the Groups/Scenes layout. Toolbar Create Palette | New Folder | Edit (Rename when a
  folder is selected) | Delete. The right side shows the selected palette (swatch, still editable for Color
  palettes; type; folder; Unassign) or folder. `PaletteUI::selected_palette` is new. With a folder selected, Create
  sends `Transaction[CreatePalette, MoveToFolder { Palette(lowest free id) }]` (`create_palette_event`, tested).
- **Programmer palette dialog** (`render_palette_assignment_dialog`): see Decisions.
- **Overlay pickers** (`render_overlay_picker_dialog` in `view.rs`, `render_view_perf_overlay_picker_dialog` in
  `view_perf.rs`): `tree_picker_dialog` with the scenes that can still be added, pruned. `ViewPerfUI` lost
  `overlay_selection` (a tap adds the scene right away, as a tap in the paged list did).
- No ABI bump, no showfile change, nothing in `crates/shared` touched.

### Decisions
- **Programmer scene column**: a tree too, so a show with folders looks the same everywhere. It is a picker for one
  scene: a scene tap focuses it, a folder tap opens or closes the folder (like Setup › Fixtures' group picker in
  017). No folder management there. Width stays the pager's **100 px**: the Programmer row at 800x480 has no slack
  left after 017; 120 px cut ~15 px off the right column, 108 px still clipped "M. Speed". So the tree uses an 18 px
  toggle and a 70 px minimum body, which leaves 8 px of indentation (every nested level indents 8 px; the guide
  lines and chevrons show the depth). Names longer than ~6 characters truncate with "…"; the `#id` line stays.
  Narrow dynamic panes get the tree in a full-width 150 px strip, as the pager had.
- **Pickers vs Setup**: pickers (Programmer scene column, palette dialog, overlay pickers) only browse and pick;
  a folder tap opens or closes the folder. Folder create/rename/delete/move are only in Setup › Scenes and
  Setup › Palettes.
- **Pickers hide empty folders** (`prune_empty_folders`) where the list is filtered (overlay pickers: scenes not yet
  added), so a folder never leads nowhere. The Setup trees and the Programmer scene column show every folder.
- **BLANK**: not shown anywhere, as before (it is never user-selectable and every list already used
  `user_scenes()` or skipped `BLANK_SCENE_ID`). `scene_tree_items` still forces BLANK to the top level if a caller
  includes it (tested), since the engine never moves it into a folder.
- **Palette dialog: one tree instead of Active + Available.** Two stacked trees did not fit at 800x480 (the Active
  tree clipped its third row). Now one tree of all palettes: active ones highlighted, a tap assigns or unassigns
  (`palette_toggle_event`, tested), like groups in the Programmer's group tree. "Active: Red, Center" above the tree
  keeps the active list readable in apply order even with folders closed (`active_palettes_summary`, tested). The
  dialog also no longer shrinks its list to two rows (`min_scrolled_height`).
- **Shared code**: the Setup wiring is factored into `SetupFolderUi` and `folder_details`; each page still owns its
  toolbar and tree, because their item actions differ (group dialogs, `SetSceneFocus`, palette editor).

### Verification
All cargo ran in the blenv container (`blenv-agent-a6f57ec982c03c776-3e452742`), never on the host.
- `cargo clippy --workspace --all-targets --all-features`: exit 0. No warning points at a line this ticket added;
  the remaining warnings in touched files are in old code (`programmer.rs:230/245`, the palette dialogs' `HSVColor`
  clones and unit `let`, the delete-scene dialog's collapsible `if`, `view_perf.rs`'s unused constants).
- `cargo fmt --all --check`: clean.
- `cargo test --workspace --all-features`: exit 0 (core 239, shared 73, others 25 + 6). New tests:
  `folder_tree::tests::{scene_items_nest_scenes_and_keep_blank_at_the_top_level, palette_items_carry_id_and_kind,
  pruning_keeps_only_folders_with_leaves_below, subtree_items_per_kind, setup_folder_ui_follows_folder_events}`,
  `palettes::tests::new_palettes_go_into_the_selected_folder`,
  `programmer::tests::palette_dialog_toggles_and_summarises_in_apply_order`.
- Each of the six commits was `cargo check`ed on its own (`--workspace --all-targets --all-features`).
- App: `scripts/blenv build all`, then `blenv run [--desktop] --showfile` with copies of `Kuze_Theater.json`
  (7 palettes created with `blctl event CreatePalette`, 6 scenes with Scene +, renamed in a scratch copy). Engine
  state checked with `blenv blctl state` (Debug dump, parsed). Screenshots in `.blenv/shots/` (not committed).
  - Before (v3 build): Setup › Scenes (pager), Setup › Palettes (flat list), Programmer, palette dialog (2 of 7
    palettes visible), View Performance overlay picker (paged).
  - Setup › Scenes, real clicks at 800x480: New Folder Show, Act 1 inside it, Slow inside that (each selected after
    creation, parent opened); Scene + with Slow selected created Ballad in Slow; "Move…" Intro -> Act 1, Verse ->
    Show, Encore -> a new top-level Specials; Show's details list 3 scenes with paths; Intro tap focused scene 1;
    Clone made "Intro B" in Act 1; Specials renamed to Extras. Engine: `1:Intro@1, 2:Verse@0, 6:Encore@3,
    7:Ballad@2, 8:Intro B@1`, folders `Show`, `Act 1<0>`, `Slow<1>`, `Extras`.
  - Setup › Palettes: Colors > Warm; Red and Amber -> Warm, Blue -> Colors (Move…); Create Palette with Colors
    selected made Magenta (#7) in Colors (the Transaction); palette details and Edit (prefilled, cancelled); a
    top-level Positions (Center, Floor moved with `blctl`); a scratch folder Tmp created and deleted; Magenta deleted
    through the toolbar (engine: palettes 0–6, folders as expected).
  - Programmer at 800x480: the scene tree fits in 100 px and the right column ("M. Speed", "Undo all") is as before;
    Ballad tap -> `current_scene_focus: 7`; a Show tap collapsed Show without changing focus. Palette dialog: Red
    unassigned, Amber and Center assigned -> scene 1 `(0,0): [2, 3]`, "Active: Amber, Center"; a Colors tap
    collapsed Colors.
  - View Performance picker: a Show tap collapsed it; Encore added (`current_overlay_scenes: [0, 6]`); reopened,
    the now-empty Extras was gone. View page: created a view, picker added Ballad (`overlays: [0, 6, 7]`).
  - Groups re-check: Setup › Groups New Folder Stage > Front, Frontlicht -> Front, Saal -> Stage, Stage details as
    before; Programmer group tree: Stage tap selected `{0, 2}`, again `{}`; Setup › Fixtures group picker: a folder
    tap collapses.
  - `--desktop`: Setup pane switched to Dynamic: Scenes and Palettes tabs with nested folders; Blue moved into Warm
    through the (window-centred) Move dialog. Programmer pane in Dynamic (narrow): scene tree strip, palette dialog,
    Blue assigned to group 1 in Ballad (`(1,0): [1]`).
- Key shots: after `20261030-192230-457` (Setup › Scenes), `20261030-193331-741` (Setup › Palettes),
  `20261030-193150-505` (Programmer, both trees), `20261030-192927-708` (palette dialog), `20261030-192640-028` (View
  Performance picker), `20261030-193308-618` (View picker), `20261030-193455-339` / `20261030-193504-268` /
  `20261030-193556-843` (`--desktop`). Before: `20261030-190359-509`, `20261030-190406-809`,
  `20261030-190413-453`, `20261030-190422-370`, `20261030-190436-159`.

### Not covered / follow-ups (in the Inbox)
- Three pickers outside the Scope still show flat lists: the palette binding picker (`palette_binding_button`,
  animation spec editor), the pointer-palette target picker (Setup › Palettes editor) and the Scene Graph's
  "+ Add Scene" combo box (which also lists BLANK). `tree_picker_dialog` fits all three.
- The palette dialog gives no feedback when the engine rejects an assignment because another active palette already
  covers those properties (same as before).
- The Programmer scene column shows nothing when the focused scene is inside a collapsed folder.
- The `Default (723 x 480)` desktop pane issue from 017 is unchanged (the scene column is as wide as before).
