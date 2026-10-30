# 012 — Apply animation presets from the Programmer

| | |
|---|---|
| Epic | E3 Programmer |
| Depends on | 011 |
| ABI / showfile | none expected (reuse existing preset / animation events) |

**Touches:** `crates/core/src/app/pages/fixtures_perf.rs`, `crates/core/src/app/pages/animations.rs (read-only reuse)`

## Problem
Original review notes (2026-09-27):

> i think not really, but would be nice to have presets.

## Scope
- Add an **Animations** panel or dialog in the Programmer. It lists `AnimationPreset`s (and templates) as buttons, and applying one attaches it to the current selection in the focused scene, the same way `render_add_animations_dialog` does today.
- Show the animations active on the current selection with remove buttons.
- Allow saving the animation on the current selection as a new preset.

## Out of scope
- Editing templates (stays in Setup, 010).

## Key code
- `AnimationPreset`, `AnimationTemplate` — `crates/shared/src/state/engine.rs:391+`
- `render_add_animations_dialog`, `render_scene_animations_dialog` — `crates/core/src/app/pages/fixtures_perf.rs`

## Acceptance criteria
- [ ] A preset can be applied to a selection in at most 2 taps.
- [ ] The active animations list matches `blctl.py state` for the scene.

## Verification
- `cargo clippy --all-targets --all-features` and `cargo fmt` are clean.
- `cargo test --all-features` passes, and new logic has co-located `#[cfg(test)]` tests.
- Follow **UI Verification** in `crates/core/AGENTS.md` (check that `swaylock` is not running, launch from the repo root, navigate via blctl, capture with grim). Use the headless Xvfb recipe when clicks are needed or another session is running (see memory notes). Take before and after screenshots, keep them in the scratchpad, and never commit them.
- Screenshot the presets panel and the selection with an animation running.

## Notes
<!-- Implementation notes, decisions, and follow-ups found while working. Keep this updated before renaming to .done.md. -->
