# 013 — Per-scene selection lock (freeze) in the Programmer

| | |
|---|---|
| Epic | E3 Programmer |
| Depends on | 011 |
| ABI / showfile | **ABI bump** (serialized shared types change) and **showfile shape** change (new fields need `#[serde(default)]`) |

**Touches:** `crates/shared/src/state/engine.rs`, `crates/shared/src/abi.rs`, engine selection event handling, `crates/core/src/app/pages/fixtures_perf.rs`, `crates/core/src/app/components/fixtures_shared.rs (fixture_selection)`

## Problem
Original review notes (2026-09-27):

> also, it would be nice to 'freeze' a selection in the programmer ui
> User clarification: lock the current selection, but only for that scene. When the lock is active, the selection UI only shows what is in the selection, hiding visual clutter.

## Scope
- Add engine state `selection_locks: BTreeSet<u8>` (scene ids), or equivalent, plus `ControlEvent::SetSelectionLock { scene, locked }` appended at the end.
- While the focused scene is locked, selection-changing events (SelectGroup, DeSelectGroup, Limit…, RemoveAllSelection, Push/Pop) are ignored for that scene and logged at debug.
- Show a lock toggle in the Programmer toolbar. When locked, the group and fixture selection widgets only show selected groups/fixtures, with a visual lock badge.
- Switching focus to another scene shows that scene's own lock state.

## Out of scope
- Named or saved selections.

## Key code
- `EngineSelection`, `selection_stack` — `crates/shared/src/state/engine.rs:26-60`
- `fixture_selection`, `group_selection` — `crates/core/src/app/components/fixtures_shared.rs:577,726`

## Acceptance criteria
- [ ] With the lock on, clicking groups does not change the selection, and the list is filtered.
- [ ] Another scene is unaffected.
- [ ] The lock survives save/reload, and old showfiles load with no locks.

## Verification
- `cargo clippy --all-targets --all-features` and `cargo fmt` are clean.
- `cargo test --all-features` passes, and new logic has co-located `#[cfg(test)]` tests.
- If the ABI was bumped: `PLUGIN_ABI_VERSION` in `crates/shared/src/abi.rs` is incremented and `cd crates/plugins && make build_noopt` was run. The Makefile reports success even on failure, so grep its output for `^error`.
- Showfile compatibility: load a **scratch copy** of an existing showfile and confirm with `python3 scripts/blctl.py state` that it is not a silently booted default show.
- Follow **UI Verification** in `crates/core/AGENTS.md` (check that `swaylock` is not running, launch from the repo root, navigate via blctl, capture with grim). Use the headless Xvfb recipe when clicks are needed or another session is running (see memory notes). Take before and after screenshots, keep them in the scratchpad, and never commit them.
- Screenshot the Programmer unlocked and locked (filtered).

## Notes
<!-- Implementation notes, decisions, and follow-ups found while working. Keep this updated before renaming to .done.md. -->
