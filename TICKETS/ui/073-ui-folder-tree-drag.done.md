# 073 — Drag items between folders in Setup lists

| | |
|---|---|
| Epic | E13 UI port |
| Depends on | 069 |
| ABI / showfile | none expected; check while implementing |

**Touches:** `app/components/folder_tree.rs`, `app/components/tree.rs`, `app/pages/setup/*.rs`

## Problem
In the mockup the Folder field is gone from detail panes; items move by dragging in the tree (`treedrag.js`). Mockup: `design/studio`; gaps listed in `design/studio/UI_GAP.md`.

## Scope
- Drag a group, scene or palette onto a folder or onto any item in it; target outlined blue; 5px threshold so clicks still select; Esc cancels.
- Groups may be dropped below the folders to leave all folders; scenes and palettes can't.
- After a drop: target folder opens, notification "Moved X to Y" (no undo), show marked unsaved.
- Remove the Folder field from the Group and Scene detail panes.
- Live lists (Programmer) never move by drag.

## Out of scope
- Fixture rows onto groups (077).
- The kiosk layout (`Density::Compact`) stays unchanged.

## Acceptance criteria
- [x] Matches the mockup at the same window size, both themes.
- [x] Moving a scene by drag changes its folder in the saved showfile.

## Verification
- `cargo clippy --all-targets --all-features`, `cargo fmt`, `cargo test --all-features`.
- UI: screenshots per `crates/core/AGENTS.md` next to the mockup: drag in progress with the outlined target.

## Notes
- `Tree::drag_to_folders(allow_root)` (`components/tree.rs`) adds the drag to the desktop tree path
  (`show_rows_compact`) only; the kiosk path (`show_rows`, `Density::Touch`) ignores it. Leaves only, as in
  `treedrag.js`: folders still move with "Move…". A press on a leaf arms the drag, it starts after 5 px
  (`TREE_DRAG_THRESHOLD`), so a click still selects; a finished or cancelled drag swallows the release click.
  Esc cancels (and is consumed). Targets: a folder band, or any item in a folder (its band is outlined);
  with `allow_root` (Groups) a top-level item or the space below the folders outlines the whole tree and
  drops out of every folder. Scenes and palettes get no root target. Dropping where the item already is sends
  nothing. Looks: target band `blue_tint` with a 1 px blue inside outline, dragged row at 40 % opacity, ghost
  label (12 px, ctl fill, blue outline, grey/muted when there is no target) 12/8 px from the pointer, grabbing
  cursor; no hover fills and no "Move…" while dragging.
- `TreeAction::DropLeaf(leaf, folder)` is handled by `drop_to_folder_event` (`folder_tree.rs`): checks it like
  the Move dialog, opens the target folder, shows "Moved X to Y" ("no folder" for the top level), no undo. The
  engine marks the show unsaved (`MoveToFolder` already counts as a show edit). Used on Setup › Groups,
  Scenes and Palettes. The Programmer and pickers never enable it.
- Scene details lose "In: <folder>" on the desktop only (kiosk keeps it). The group pane had no folder field.
  The palette pane still shows "In:" (not in this ticket's scope).
- Fix on the way: the desktop row's "Move…" button was drawn with `scope_builder`, which pushed every row below
  the hovered or selected row down by 2 px; it now uses a child Ui, so the rows don't jump when a drag starts.
- Verified in blenv at 1500x950 (`--desktop`, `SPARTACUS_SOLI.json` copy) with real xdotool
  mousedown/move/up: scene STATIC onto the FOG band (outlined target next to the mockup shot of the same
  drag), group FOG below the folders (whole tree outlined, "Moved FOG to no folder"), palette STAGE onto an item
  in TOP-LEVEL in the light theme; Esc mid-drag cancels with nothing moved; click after drag still selects.
  After the scene drop the status showed "Unsaved changes", and after saving `/state/showfiles/SPARTACUS_SOLI.json`
  had STATIC (#2) in folder 7 (FOG).
- Kiosk: Setup › Scenes at 800x480 is byte-identical (`cmp`) between decouple and ui/073.
- Checks: `rustfmt` on the touched files (a workspace `cargo fmt` reformats many untouched files on
  decouple, so those hunks were left out), `cargo clippy -p blaulicht-ui-egui --all-targets --all-features`
  (no warnings in touched code) and `cargo test -p blaulicht-ui-egui --all-features` (145 passed, new
  `drop_sends_a_checked_move_and_opens_the_target`), all inside the blenv container. The full workspace
  clippy/test was not run (ui_egui is the only crate changed).
- Not done / follow-ups: the rest of the mockup's Setup pages (title bars, Add/Folder buttons, detail panes)
  is 080; dragging fixture rows onto groups and a drag in the Setup › Fixtures group picker are 077. Not
  verified: drag inside a scrolled tree (auto-scroll while dragging does not exist, as in the mockup).
