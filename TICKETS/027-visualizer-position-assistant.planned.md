# 027 — Visualizer position assistant (grid placement with layers)

| | |
|---|---|
| Epic | E7 Visualizer |
| Depends on | 026 |
| ABI / showfile | Showfile shape if new placement metadata (layer id) is stored; use `#[serde(default)]` |

**Touches:** `crates/core/src/app/pages/visualizer/ (new assistant module)`, visualizer state

## Problem
Original review notes (2026-09-27):

> maybe do a 'position assistant?'
> maybe allow it to be grid-pinned and pixel-art like?
> vertical will be 'layers'

## Scope
- A top-down 2D placement mode (or sub-page) for quick stage layout: a grid with a configurable cell size (e.g. 25 cm), fixtures and trusses drawn as their real footprint (from 026), drag/tap to place, and snapping to cells ("pixel-art" placement).
- Vertical position is chosen from discrete **layers** (floor, truss 1 height, truss 2 height, …), each with a name and height. Changing a layer's height moves everything on it.
- Positions are written into the existing visualizer state so the 3D view reflects them immediately.

## Out of scope
- Automatic placement.

## Key code
- `VisualizerUiState`, `EditorState` — `crates/core/src/app/pages/visualizer/state.rs`
- snapping helpers in `visualizer/ui.rs` / `math.rs`

## Acceptance criteria
- [ ] A small rig (e.g. 8 fixtures on 2 truss layers) can be placed via the assistant only, and the 3D view matches.

## Verification
- `cargo clippy --all-targets --all-features` and `cargo fmt` are clean.
- `cargo test --all-features` passes, and new logic has co-located `#[cfg(test)]` tests.
- Showfile compatibility: load a **scratch copy** of an existing showfile and confirm with `python3 scripts/blctl.py state` that it is not a silently booted default show.
- Follow **UI Verification** in `crates/core/AGENTS.md` (check that `swaylock` is not running, launch from the repo root, navigate via blctl, capture with grim). Use the headless Xvfb recipe when clicks are needed or another session is running (see memory notes). Take before and after screenshots, keep them in the scratchpad, and never commit them.
- Headless X recipe with real clicks. Screenshot the assistant grid and the resulting 3D view.

## Notes
<!-- Implementation notes, decisions, and follow-ups found while working. Keep this updated before renaming to .done.md. -->
