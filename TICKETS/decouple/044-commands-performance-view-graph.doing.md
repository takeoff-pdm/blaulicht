# 044 — Performance, View and Scene graph pages onto EngineClient

| | |
|---|---|
| Epic | E10 Decouple |
| Depends on | 042 |
| ABI / showfile | none (new operations go into `HostCommand`) |

**Touches:** `crates/core/src/app/pages/view.rs`, `view_perf.rs`, `scene_graph.rs`, `palettes.rs`, `crates/core/src/dmx/mod.rs`, `crates/core/src/state.rs`

## Problem
- `view.rs` writes the engine directly at `:62`, `:144`, `:185`, `:312` and `:469`, and deep-clones the state
  per frame at `:119` and `:335`.
- `scene_graph.rs` writes at `:285`, `:310` and `:344`. It saves by diffing and replacing the *whole* graph
  (`:336–344`), but the engine also mutates `scene_graphs` every tick, so replacing the whole graph loses
  updates. It also writes `focused_graph` directly (`:287–398`, `:867`).
- The grand master is written through `set_grand_master_percent` (`view_perf.rs:243`, `:252`, `:257`).

## Scope
- All reads go through `with_show`, and all writes are commands.
- Scene graph edits become granular node and edge commands (add or remove a node, move it, connect or
  disconnect an edge, set a parameter), sent when a drag ends or a connection is made, not every frame.
  `focused_graph` is session state (040): set it with the existing `SetFocusedSceneGraph`.
- The grand master becomes a `HostCommand::SetGrandMaster(u8)`. The quit-time reset at `ui.rs:530` moves in 052.
- Replace per-frame deep clones with scoped `with_show` blocks.

## Out of scope
- 020–023 (Performance features) and 038.
- The visualizer (051).

## Acceptance criteria
- [ ] No production code in these files touches `self.data.state` or `AppState`.
- [ ] Views, overlays, the grand master and the scene graph (drag, wire, delete, focus) work as before.
- [ ] Unit test: a graph edit command while the engine ticks the graph loses no runtime state.

## Verification
- `cargo clippy --workspace --all-targets --all-features`, `cargo fmt`, `cargo test --workspace --all-features`.
- blenv: screenshots of each page, plus an interaction test that wires a scene graph node.

## Notes
