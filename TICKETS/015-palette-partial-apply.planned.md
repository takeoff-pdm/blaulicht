# 015 — Apply only selected attributes of a compound palette

| | |
|---|---|
| Epic | E3 Programmer |
| Depends on | 011 |
| ABI / showfile | **ABI bump** (serialized shared types change) and **showfile shape** change (new fields need `#[serde(default)]`) |

**Touches:** `crates/shared/src/scene.rs (EngineSink palette assignment)`, `crates/shared/src/palette.rs`, `crates/shared/src/abi.rs`, `crates/core/src/app/pages/fixtures_perf.rs (render_palette_assignment_dialog)`

## Problem
Original review notes (2026-09-27):

> no possibility of using only one attribute of a compound palette
> would be nice to do this during the apply step in the programmer

## Scope
- Extend `AssignPaletteToSelection` (or add a new event appended at the end) with an optional attribute mask (`Option<Vec<FixtureProperty>>`, where `None` = all).
- Store the mask with the per-fixture assignment in `EngineSink`, and make `sync_palette_bindings` only bind the masked properties.
- In `render_palette_assignment_dialog` (`fixtures_perf.rs:826`), after picking a palette, show its properties (`PaletteKind::properties`) as toggle chips (e.g. Hue/Sat/Val, Pan/Tilt, Focus/Strobe) with all selected by default.

## Out of scope
- Palette hierarchy (016/018).

## Key code
- `render_palette_assignment_dialog`, `refresh_active_palettes_cache` — `crates/core/src/app/pages/fixtures_perf.rs:793-826`
- `EngineSink`, `sync_palette_bindings` — `crates/shared/src/scene.rs:120+`
- `PaletteKind::properties` — `crates/shared/src/palette.rs`

## Acceptance criteria
- [ ] Applying a Position palette with only Pan leaves Tilt untouched, and the same for a Color palette with only Hue.
- [ ] Unit tests cover the mask.
- [ ] Old showfiles load with full-palette assignments.

## Verification
- `cargo clippy --all-targets --all-features` and `cargo fmt` are clean.
- `cargo test --all-features` passes, and new logic has co-located `#[cfg(test)]` tests.
- If the ABI was bumped: `PLUGIN_ABI_VERSION` in `crates/shared/src/abi.rs` is incremented and `cd crates/plugins && make build_noopt` was run. The Makefile reports success even on failure, so grep its output for `^error`.
- Showfile compatibility: load a **scratch copy** of an existing showfile and confirm with `python3 scripts/blctl.py state` that it is not a silently booted default show.
- Follow **UI Verification** in `crates/core/AGENTS.md` (check that `swaylock` is not running, launch from the repo root, navigate via blctl, capture with grim). Use the headless Xvfb recipe when clicks are needed or another session is running (see memory notes). Take before and after screenshots, keep them in the scratchpad, and never commit them.
- Screenshot the assignment dialog with chips, then DMX/state before and after a pan-only apply.

## Notes
<!-- Implementation notes, decisions, and follow-ups found while working. Keep this updated before renaming to .done.md. -->
