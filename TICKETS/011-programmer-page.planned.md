# 011 — Programmer page: rename, clear selection, scene changes with undo-all

| | |
|---|---|
| Epic | E3 Programmer |
| Depends on | 006 |
| ABI / showfile | Possibly **ABI bump** if a new `ControlEvent` (e.g. `ClearSceneChanges`) is added (append at the end) |

**Touches:** `crates/core/src/app/pages/fixtures_perf.rs`, `crates/core/src/app/components/fixtures_shared.rs`, `crates/shared/src/page/mod.rs (label only)`, engine event handling for undo-all

## Problem
Original review notes (2026-09-27):

> also fixtures performance should rather be 'programmer'
> also selection in the programmer should have a 'clear' shortcut
> also easy view of the scene changes, and one button to undo all
> Removing changes from a scene (root backlog: "most important")

## Scope
- Rename the FixturesPerformance label to **Programmer**. Keep the `AppPage` variant index stable, and rename the Rust identifier only if it doesn't break bincode.
- Add a **Clear selection** button and a keyboard shortcut (e.g. `Esc` or `C`, when no text field has focus) that sends `RemoveAllSelection`.
- Add a **Scene changes** panel for the focused scene. It lists the changed fixtures and properties (reusing `render_scene_changeset_dialog` / `scene_changes`) with per-row remove and one **Undo all** button (with confirmation) that clears the scene's sink changes. Also bring over the scene animations dialog that 006 removed from setup.

## Out of scope
- Selection lock (013).
- Animation presets (012).

## Key code
- `fixtures_ui` — `crates/core/src/app/pages/fixtures_perf.rs:142`
- `render_scene_changeset_dialog`, `scene_changes` — `fixtures_shared.rs` / `fixtures_perf.rs`
- `EngineSelection::clear`, `RemoveAllSelection` — `crates/shared/src/state/engine.rs:26-60`

## Acceptance criteria
- [ ] The navbar says Programmer.
- [ ] The shortcut clears the selection.
- [ ] Undo all empties the focused scene's changes and nothing else (check other scenes with `blctl.py state`).

## Verification
- `cargo clippy --all-targets --all-features` and `cargo fmt` are clean.
- `cargo test --all-features` passes, and new logic has co-located `#[cfg(test)]` tests.
- If the ABI was bumped: `PLUGIN_ABI_VERSION` in `crates/shared/src/abi.rs` is incremented and `cd crates/plugins && make build_noopt` was run. The Makefile reports success even on failure, so grep its output for `^error`.
- Follow **UI Verification** in `crates/core/AGENTS.md` (check that `swaylock` is not running, launch from the repo root, navigate via blctl, capture with grim). Use the headless Xvfb recipe when clicks are needed or another session is running (see memory notes). Take before and after screenshots, keep them in the scratchpad, and never commit them.
- Screenshot the Programmer with the scene-changes panel populated, then after Undo all.

## Notes
<!-- Implementation notes, decisions, and follow-ups found while working. Keep this updated before renaming to .done.md. -->
