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
- [x] `components::tree` renders nested folders and leaves with indentation and touch-sized rows.
- [x] Tapping a folder's toggle collapses/expands it, and the state survives page switches within the session.
- [x] Leaf selection, folder tap and "Move to…" are distinct responses.
- [x] Orphaned parents and cycles render as roots without hanging (unit tests).
- [x] No folder types and no ABI or showfile change.

## Verification
- `cargo clippy --all-targets --all-features` and `cargo fmt` are clean.
- `cargo test --all-features` passes, and the tree logic has co-located `#[cfg(test)]` tests.
- UI: mount the widget temporarily with sample data and take blenv screenshots (800x480 and `--desktop`),
  expanded and collapsed, after real clicks on a folder toggle and a leaf. Remove the temporary mount before
  committing; keep the screenshots in `.blenv/shots/` and never commit them.

## Notes
<!-- Implementation notes, decisions, and follow-ups found while working. Keep this updated before renaming to .done.md. -->
**API** (`crates/core/src/app/components/tree.rs`, re-exported from `components`):
- `TreeKey<F, L> { Folder(F), Leaf(L) }`. Folder and leaf ids are separate types, so a `u16` folder id and a `u8`
  group id can't be mixed up.
- `TreeItem { key, parent: Option<F>, label, detail }`, built with `TreeItem::folder(id, parent, label)`,
  `TreeItem::leaf(id, parent, label)` and `.with_detail(..)`. Without a detail, folders show their child count.
- `Tree::new(id_salt, &items)`, with the builders `.selected(Option<TreeKey>)`, `.with_move_button(bool)`,
  `.row_height(f32)` (default 50) and `.indent(f32)` (default 16). `.show(ui) -> Option<TreeAction>`.
- `TreeAction { SelectLeaf(L), TapFolder(F), MoveRequested(TreeKey) }`. A chevron toggle is handled internally and is
  not reported.
- `TreeState<F>` (load/store, toggle, set_collapsed) lives in egui temp memory under `tree_id(salt)`. It is global
  and not tied to the parent `Ui`, so the same salt shares collapse state between pages and dialogs for the session.
  Callers that want to own the state can use `tree_id` with `TreeState::load/store`.
- Pure logic: `visible_rows(items, &collapsed) -> Vec<TreeRow>` and `indent_width(..)`.

**Decisions**
- Order under each parent: folders first, then leaves, each in input order. Callers sort before passing items.
- Bad input: a missing parent, or a parent that is a leaf, makes the item a root. Every folder on a parent cycle
  becomes a root, and branches hanging off a cycle keep their parent. Duplicate keys keep their first occurrence.
  Flattening uses an explicit stack (tested with a 20 000-deep chain).
- Row layout is a 40 px chevron hit area (leaves keep the gap so labels line up), then the body (a `clickable`,
  highlighted when selected), then an optional 64 px "Move…" button. Indentation shrinks so the body stays at least
  96 px wide. Labels truncate with "…".
- `show()` wraps the rows in `ui.vertical`, so the tree stacks top-down even inside a horizontal parent layout.
  The first demo mount in `horizontal_top`/`allocate_ui` laid the rows out left to right without this.
- 016's model maps on with one `map` per collection (see the module docs). No folder types, events, ABI or
  showfile changes.

**Verification** (in the blenv container, rustfmt and clippy added to its toolchain with `rustup component add`):
- `cargo fmt`: clean for these files. `cargo fmt --check` still reports an existing diff in
  `app/pages/animations.rs`, which is not touched here.
- `cargo clippy --all-targets --no-default-features --features "x11 audio wasmtime" -p blaulicht-core`:
  no warnings in `tree.rs` or `mod.rs`. The crate's existing warnings remain.
- `cargo test -p blaulicht-core --no-default-features --features "x11 audio wasmtime"`: 193 passed, including
  the 13 `tree` tests.
- `--all-features` was not run: it would add `wayland`, `audio-mock`, `dhat` and others that the container build
  doesn't use. The features above are the ones `tools/blenv/container/build.sh` uses.
- UI: a temporary demo on the Logs page (removed before commit) showed nested folders, a 4-deep chain, an
  orphan, a 2-cycle, an empty folder and long names. Real clicks at 800x480 and in `--desktop`: chevron collapse
  (subtree hidden, nothing reported), leaf -> `SelectLeaf`, folder body -> `TapFolder`, "Move…" ->
  `MoveRequested(Leaf(2))` (800x480 only). Collapse and selection survived Logs -> System -> Logs in both modes.

**Follow-ups**
- There is no "start collapsed" option: new folders always start expanded. Add a `.default_collapsed(bool)` or
  an initial set if 017 or 018 need it.
- An optional helper for "all leaves under folder X (recursively)" would let 017 resolve `TapFolder` into the
  groups to select. It is left to the caller for now.
