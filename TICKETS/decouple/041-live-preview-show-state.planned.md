# 041 — Live mode becomes a live preview in show state

| | |
|---|---|
| Epic | E10 Decouple |
| Depends on | 040 |
| ABI / showfile | none (runtime-only field; plugins see `live_mode` through the same projection as 040) |

**Touches:** `crates/shared/src/state/engine.rs`, `crates/core/src/dmx/mod.rs`, `crates/core/src/app/pages/programmer.rs`, `crates/core/src/app/pages/view_perf.rs`

## Problem
`live_mode: bool` renders `current_scene_focus` as the base layer. Since 040, focus belongs to a session, but what
the rig outputs is global. "Live mode on" therefore needs to say *which* scene is previewed.

## Scope
- Replace `live_mode: bool` with `live_preview: Option<u8>` (scene id) in show state, set by a new command
  (`ControlEvent` variant appended **at the end**). Turning live mode on from a session sets it to that session's
  focus, and changing focus while live follows that session. Only one preview exists at a time; last write wins.
- `render_base_scene` and `rendered_overlays` read `live_preview`.
- The UI shows LIVE as active when the preview is the scene this frontend's session is focused on.
- Plugins keep seeing `live_mode` as `live_preview.is_some()` through the 040 projection.

## Out of scope
- Per-session previews. There is one rig and one output.

## Key code
- `live_mode`, `render_base_scene`, `rendered_overlays` — `crates/shared/src/state/engine.rs:~95–135`
- LIVE toggle — `crates/core/src/app/pages/programmer.rs:47`, `:273`
- Performance page — `crates/core/src/app/pages/view_perf.rs:207`

## Acceptance criteria
- [ ] Live preview behaves exactly as before for a single operator.
- [ ] Unit test: two sessions, one turns live on, and the output renders that session's scene.

## Verification
- `cargo clippy --all-targets --all-features`, `cargo fmt`, `cargo test --all-features`.
- UI: screenshot of Programmer with LIVE on.

## Notes
