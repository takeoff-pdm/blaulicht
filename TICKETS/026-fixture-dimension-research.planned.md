# 026 — Research exact fixture and truss dimensions

| | |
|---|---|
| Epic | E7 Visualizer |
| Depends on | none |
| ABI / showfile | Showfile shape only if dimensions are stored per fixture there (prefer an asset table in code/data) |

**Touches:** `crates/core/src/stage_assets.rs (or a data file next to it)`, visualizer model/asset definitions

## Problem
Original review notes (2026-09-27):

> also research exact dimensions from official docs

## Scope
- List the fixture types and truss types actually used in the showfiles in the repo (e.g. `Kuze_Theater.json`, `SPARTACUS_DRAFT.json`, `crates/core/OUTDOOR.json`).
- For each, find the manufacturer's official spec sheet and record the physical dimensions (W×D×H mm, weight, beam origin/pivot offset if listed) with the source URL.
- Store them as data used by the visualizer (`stage_assets.rs` or a TOML/JSON asset table), replacing guessed sizes.

## Out of scope
- Placement UI (027).

## Key code
- `crates/core/src/stage_assets.rs`, `crates/core/src/app/pages/visualizer/truss.rs`

## Acceptance criteria
- [ ] Every used type has dimensions with a cited source, and the 3D view renders them to scale.

## Verification
- `cargo clippy --all-targets --all-features` and `cargo fmt` are clean.
- `cargo test --all-features` passes, and new logic has co-located `#[cfg(test)]` tests.
- Showfile compatibility: load a **scratch copy** of an existing showfile and confirm with `python3 scripts/blctl.py state` that it is not a silently booted default show.
- Follow **UI Verification** in `crates/core/AGENTS.md` (check that `swaylock` is not running, launch from the repo root, navigate via blctl, capture with grim). Use the headless Xvfb recipe when clicks are needed or another session is running (see memory notes). Take before and after screenshots, keep them in the scratchpad, and never commit them.
- Screenshot the 3D view before and after with a known reference, e.g. a 2 m truss next to a fixture.

## Notes
<!-- Implementation notes, decisions, and follow-ups found while working. Keep this updated before renaming to .done.md. -->
