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
- [ ] Matches the mockup at the same window size, both themes.
- [ ] Moving a scene by drag changes its folder in the saved showfile.

## Verification
- `cargo clippy --all-targets --all-features`, `cargo fmt`, `cargo test --all-features`.
- UI: screenshots per `crates/core/AGENTS.md` next to the mockup: drag in progress with the outlined target.

## Notes
