# 030 — Reusable collapsible tree widget

| | |
|---|---|
| Epic | E4 Hierarchy (folders) |
| Depends on | none |
| ABI / showfile | none |

**Touches:** new: `crates/core/src/app/components/tree.rs`, `crates/core/src/app/components/mod.rs`

## Problem
Split out of 017 so the folder-hierarchy work can run in parallel: the generic tree widget has no dependencies,
while 017 and 018 wait for 016 (data model), 006 (Setup shell) and 010.

Original review notes (2026-09-27):

> fixture groups should be hierarchical
> same for scenes
> like everything, we need hierarchies

## Scope
- Build a reusable collapsible tree widget (`components/tree.rs`) that fits both the 800x480 integrated layout and
  desktop mode. 017 (groups) and 018 (scenes, palettes) use it.
- Data-model agnostic: it takes generic nodes (id, parent, label, folder or leaf) and does not depend on the
  folder types from 016. 016's `folders: BTreeMap<u16, Folder { parent, .. }>` plus `folder: Option<u16>` on
  items must map onto it trivially.
- Collapsible folders; collapsed state is kept for the session (egui memory under an id, or caller-owned).
- Selecting a leaf and tapping a folder are reported as distinct responses (017 uses a folder tap to select all
  its groups).
- An optional per-row "Move to…" affordance that reports the row back to the caller (touch is primary, no
  drag-only interaction).
- Touch-sized rows and an indentation that works at 800x480 and in `--desktop-mode`.
- Robust against bad input: orphaned parents and parent cycles are treated as roots, no infinite loops.
- Pure logic (building and flattening the visible rows, orphan and cycle handling, collapse) in functions with
  co-located `#[cfg(test)]` tests.

## Out of scope
- Folder types, events and showfile changes (016).
- Wiring the tree into Setup › Groups and the Programmer (017), and into scenes, palettes and pickers (018).
- The "Move to…" dialog itself and folder CRUD (017, 018); the widget only reports the request.

## Key code
- `clickable`, `ButtonSize` — `crates/core/src/app/components/clickable.rs`, `button.rs`
- `group_selection` (row style to match) — `crates/core/src/app/components/fixtures_shared.rs:577`

## Acceptance criteria
- [ ] `components::tree` renders nested folders and leaves with indentation and touch-sized rows.
- [ ] Tapping a folder's toggle collapses/expands it, and the state survives page switches within the session.
- [ ] Leaf selection, folder tap and "Move to…" are distinct responses.
- [ ] Orphaned parents and cycles render as roots without hanging (unit tests).
- [ ] No folder types and no ABI or showfile change.

## Verification
- `cargo clippy --all-targets --all-features` and `cargo fmt` are clean.
- `cargo test --all-features` passes, and the tree logic has co-located `#[cfg(test)]` tests.
- UI: mount the widget temporarily with sample data and take blenv screenshots (800x480 and `--desktop`),
  expanded and collapsed, after real clicks on a folder toggle and a leaf. Remove the temporary mount before
  committing; keep the screenshots in `.blenv/shots/` and never commit them.

## Notes
<!-- Implementation notes, decisions, and follow-ups found while working. Keep this updated before renaming to .done.md. -->
