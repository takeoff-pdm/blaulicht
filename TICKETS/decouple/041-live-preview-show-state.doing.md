# 041 — Live preview owned by a session

| | |
|---|---|
| Epic | E10 Decouple |
| Depends on | 040 |
| ABI / showfile | none: `live_mode` stays in `EngineState`, and `SetLiveMode` / `ToggleLiveMode` already exist |

**Touches:** `crates/core/src/dmx/mod.rs`, `crates/core/src/dmx/state.rs`, `crates/shared/src/state/engine.rs`, `crates/core/src/app/pages/programmer.rs`, `crates/core/src/app/pages/view_perf.rs`

## Problem
`live_mode` renders `current_scene_focus` as the base layer (`render_base_scene`, `engine.rs:~110`). Since
040, focus belongs to a session, but the rig output is global, so "live" has to say whose focus it follows.

## Scope
- Add `live_owner: Option<SessionId>` to the core wrapper. `SetLiveMode(true)` / `ToggleLiveMode` make the
  sending session the owner; `SetLiveMode(false)` clears it. `live_mode` stays in `EngineState` as
  `live_owner.is_some()`, so plugins see the same thing as today.
- The base layer is the owner's focus. For `DESK` that is unchanged. For a remote owner, the render path reads
  that session's focus from the side store. The shared `render_base_scene` either takes the focus as an
  argument or gets a core-side wrapper; nothing else may recompute it.
- If the owner session disconnects, or its focused scene is deleted, clear the preview.
- UI: the LIVE banner shows when *any* preview is active (overlays are suppressed for everyone). The toggle
  shows as "on" when this frontend's session owns it.
- `view_perf.rs:278–279` renders `current_scene_focus` while live; read the preview scene instead.
  `korg.rs:218–219` does the same on the plugin side; document that with two operators, plugins see
  `live_mode` together with the `DESK` focus.

## Out of scope
- More than one preview. There is one rig.

## Key code
- `live_mode`, `render_base_scene`, `rendered_overlays` — `crates/shared/src/state/engine.rs:102–138`
- `set_live_mode` — `crates/core/src/dmx/mod.rs:996–1002`, `:1658`
- Render callers — `crates/core/src/dmx/mod.rs:552`, `:583`, `:1691`
- LIVE toggle — `crates/core/src/app/pages/programmer.rs:47`, `:273`; Performance `view_perf.rs:207`, `:278`

## Acceptance criteria
- [ ] Live preview behaves exactly as before for one operator.
- [ ] Unit tests: a remote session turns live on, and the output renders its focus; the owner disconnects, and
      the preview clears; the owner's scene is deleted, and the preview clears.

## Verification
- `cargo clippy --workspace --all-targets --all-features`, `cargo fmt`, `cargo test --workspace --all-features`.
- blenv: a screenshot of Programmer with LIVE on.

## Notes
