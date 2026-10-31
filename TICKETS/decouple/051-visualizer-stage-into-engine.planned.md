# 051 — Visualizer onto EngineClient; stage geometry becomes show state

| | |
|---|---|
| Epic | E10 Decouple |
| Depends on | 042 |
| ABI / showfile | showfile *shape* unchanged (the `stage` section stays where it is); the owner changes from UI to engine |

**Touches:** `crates/core/src/app/pages/visualizer/{ui,state,data}.rs`, `crates/core/src/stage.rs`, `crates/core/src/state.rs`, `crates/core/src/dmx/mod.rs`, `crates/core/src/config.rs`

## Problem
- The stage (room, trusses, models, fixture attachments) is show data, but it lives in
  `visualizer_ui_state.stage` (`visualizer/state.rs:147`).
- The UI saves it (`ui.rs:607`) and derives engine fixture positions from it every frame
  (`reconcile_truss_attachments`, `visualizer/ui.rs:756–768`).
- GLB import writes files next to the showfile on the frontend's machine (`visualizer/ui.rs:1105–1120`).
- `visualizer/ui.rs` writes engine state directly at 6 sites.

## Scope
- `StageScene` moves into engine show state (in `AppState` next to `dmx_engine`, or inside the core wrapper).
  Stage edits become `HostCommand`s, and attachment reconciliation runs in the engine when the stage or the patch
  changes.
- Model import: the frontend sends the file bytes in a command, and the engine stores them next to the showfile.
- The camera, selection highlight and editor state stay view state in the frontend.
- The showfile `stage` section is written by whoever builds the showfile. Until 052, the UI builds it by reading
  the stage through `with_show`.
- All other visualizer reads go through `with_show`. The GL render target and three-d resources stay
  frontend-local.

## Out of scope
- 024–027 (visualizer features).

## Acceptance criteria
- [ ] No production code under `app/pages/visualizer/` writes engine state.
- [ ] Editing trusses and attachments, and importing a model, works, and the result survives save and reload.

## Verification
- `cargo clippy --workspace --all-targets --all-features`, `cargo fmt`, `cargo test --workspace --all-features`.
- blenv: Visualizer screenshots before and after a truss edit; save, reload, screenshot.

## Notes
