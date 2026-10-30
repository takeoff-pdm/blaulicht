# 016 — Folder hierarchy data model for groups, scenes and palettes

| | |
|---|---|
| Epic | E4 Hierarchy (folders) |
| Depends on | none |
| ABI / showfile | **ABI bump** (serialized shared types change) and **showfile shape** change (new fields need `#[serde(default)]`) |

**Touches:** `crates/shared/src/fixture/state.rs`, `crates/shared/src/scene.rs`, `crates/shared/src/palette.rs`, `crates/shared/src/state/engine.rs`, `crates/shared/src/abi.rs`, engine event handling, save.rs / showfile

## Problem
Original review notes (2026-09-27):

> fixture groups should be hierarchical
> same for scenes
> like everything, we need hierarchies

## Scope
- Add `folders: BTreeMap<u16, Folder { name, parent: Option<u16>, kind: FolderKind(Groups|Scenes|Palettes) }>` to `EngineState` and `folder: Option<u16>` on `FixtureGroup`, `Scene` and `Palette` (all `#[serde(default)]`).
- Add `ControlEvent`s, appended at the end, for create, rename, delete (children move to the parent) and move-item-to-folder, with cycle prevention.
- Selecting a groups folder selects all groups under it recursively: a helper plus a `SelectFolder` event that expands to group ids.
- Only data and engine changes here. The UI is 017 and 018.

## Out of scope
- Nested groups with value cascading.
- Any UI.

## Key code
- `FixtureGroup`, `EngineGroups` — `crates/shared/src/fixture/state.rs:28`
- `Scene` — `crates/shared/src/scene.rs:96`
- `Palette` — `crates/shared/src/palette.rs`
- `EngineState` — `crates/shared/src/state/engine.rs`

## Acceptance criteria
- [ ] Unit tests cover create/move/delete, cycle rejection and recursive folder selection.
- [ ] An existing showfile (e.g. a scratch copy of `Kuze_Theater.json`) still loads with all groups and scenes. Check with `blctl.py state` that it is **not** a default show (a shape mismatch silently boots a default show).
- [ ] ABI bumped and plugins rebuilt.

## Verification
- `cargo clippy --all-targets --all-features` and `cargo fmt` are clean.
- `cargo test --all-features` passes, and new logic has co-located `#[cfg(test)]` tests.
- If the ABI was bumped: `PLUGIN_ABI_VERSION` in `crates/shared/src/abi.rs` is incremented and `cd crates/plugins && make build_noopt` was run. The Makefile reports success even on failure, so grep its output for `^error`.
- Showfile compatibility: load a **scratch copy** of an existing showfile and confirm with `python3 scripts/blctl.py state` that it is not a silently booted default show.

## Notes
<!-- Implementation notes, decisions, and follow-ups found while working. Keep this updated before renaming to .done.md. -->
