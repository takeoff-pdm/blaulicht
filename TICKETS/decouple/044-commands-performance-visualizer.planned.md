# 044 — Performance, View, Scene graph and Visualizer pages onto EngineClient

| | |
|---|---|
| Epic | E10 Decouple |
| Depends on | 042 |
| ABI / showfile | none, unless a new `ControlEvent` variant is needed (append at the end) |

**Touches:** `crates/core/src/app/pages/view.rs`, `view_perf.rs`, `scene_graph.rs`, `visualizer/ui.rs`, `crates/core/src/dmx/mod.rs`

## Problem
Direct engine writes remain in `visualizer/ui.rs` (6), `view.rs` (5) and `scene_graph.rs` (3), as well as
reads under lock in `view_perf.rs`.

## Scope
- Move these pages onto `EngineClient`; no direct `AppState` locks remain in them.
- Scene graph node edits (dragging, wiring) send a command when the drag ends or the connection is made,
  not on every frame of the drag.
- Visualizer camera and selection highlight stay view state in the frontend.

## Out of scope
- 020–024 (Performance and Visualizer feature work) and 038. Don't run them at the same time on the same files.

## Key code
- `scene_graph.rs:285`, `:310`, `:344`
- `view.rs:62`, `:144`, `:185`, `:312`, `:469`

## Acceptance criteria
- [ ] `grep -nE 'state\.[a-z_]+\.(write|lock)\(\)'` over these files finds nothing.
- [ ] Views, overlays, the scene graph and the visualizer work as before.

## Verification
- `cargo clippy --all-targets --all-features`, `cargo fmt`, `cargo test --all-features`.
- UI: screenshots of each page, plus an interaction test that wires a scene graph node.

## Notes
