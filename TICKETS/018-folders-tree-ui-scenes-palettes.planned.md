# 018 — Tree UI for scene and palette folders

| | |
|---|---|
| Epic | E4 Hierarchy (folders) |
| Depends on | 016, 006, 010, 017 |
| ABI / showfile | none |

**Touches:** `crates/core/src/app/pages/setup/scenes.rs`, `crates/core/src/app/pages/palettes.rs`, `crates/core/src/app/components/fixtures_shared.rs (scene_overview)`, view/performance scene pickers

## Problem
Original review notes (2026-09-27):

> same for scenes
> like everything, we need hierarchies

## Scope
- Use the tree widget from 017 in Setup › Scenes, Setup › Palettes, the Programmer palette-assignment dialog, and the scene/overlay pickers (views, performance).
- Add folder CRUD and move-to-folder for scenes and palettes.

## Out of scope
- New features beyond folders.

## Key code
- `scene_overview` — `fixtures_shared.rs:1142`
- `render_overlay_picker_dialog` — `crates/core/src/app/pages/view.rs`
- `render_palette_assignment_dialog` — `fixtures_perf.rs:826`

## Acceptance criteria
- [ ] Every scene and palette list or picker shows folders consistently.

## Verification
- `cargo clippy --all-targets --all-features` and `cargo fmt` are clean.
- `cargo test --all-features` passes, and new logic has co-located `#[cfg(test)]` tests.
- Follow **UI Verification** in `crates/core/AGENTS.md` (check that `swaylock` is not running, launch from the repo root, navigate via blctl, capture with grim). Use the headless Xvfb recipe when clicks are needed or another session is running (see memory notes). Take before and after screenshots, keep them in the scratchpad, and never commit them.
- Screenshot the Scenes and Palettes sub-pages and one picker dialog.

## Notes
<!-- Implementation notes, decisions, and follow-ups found while working. Keep this updated before renaming to .done.md. -->
