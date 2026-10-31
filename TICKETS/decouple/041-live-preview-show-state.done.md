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
- [x] Live preview behaves exactly as before for one operator (one deliberate edge-case change, see Notes:
      deleting the previewed scene now ends the preview).
- [x] Unit tests: a remote session turns live on, and the output renders its focus; the owner disconnects, and
      the preview clears; the owner's scene is deleted, and the preview clears.

## Verification
- `cargo clippy --workspace --all-targets --all-features`, `cargo fmt`, `cargo test --workspace --all-features`.
- blenv: a screenshot of Programmer with LIVE on.

## Notes

### What changed
- **Core wrapper (`dmx/state.rs`, `dmx/session.rs`).** `EngineState` is now
  `EngineState(pub blaulicht_shared::EngineState, pub(super) SessionStore, pub(super) SessionMeta)`.
  `SessionMeta { live_owner: Option<SessionId>, swapped_in: SessionId }` (`Default`: no owner, `DESK`).
  - `live_owner()`, `render_base_scene()` (the owner's focus via `scene_focus_of`, passed to the shared
    function), `pub(super) set_live_owner(Option<SessionId>) -> bool` (keeps the shared `live_mode` equal to
    `live_owner.is_some()`) and `pub(super) applying_session()` (the sender while `dispatch` runs, `DESK`
    otherwise).
  - `swap_session` records the swapped-in session, so `session(id)` / `scene_focus_of(id)` are now correct
    *during* `dispatch` too (before, they returned the swapped fields for the wrong id). This is what lets
    `set_live_mode` and a future in-dispatch scene delete read the owner's focus by id.
  - `close_session` ends the preview if the closed session owns it (so `apply_bus_messages`' `Closed` loop
    clears it without extra code). `delete_scene` ends it when the deleted scene was the owner's focus.
    `load_showfile` clears the owner along with `live_mode`.
- **Shared (`state/engine.rs`).** `render_base_scene(&self, live_focus: u8)` takes the owner's focus; core
  only calls it through the wrapper. The `live_mode` doc says what plugins see with two operators. No field
  change, `PLUGIN_ABI_VERSION` unchanged (18).
- **Engine (`dmx/mod.rs`).** `SetLiveMode(true)` makes the sender the owner (taking it over from another
  session), `SetLiveMode(false)` ends the preview whoever owns it, and `ToggleLiveMode` ends it if the sender
  owns it and otherwise makes the sender the owner. `set_live_mode(&mut EngineState, Option<SessionId>)`
  restarts the animations of the *owner's* focused scene whenever the owner changes, so the reset scene and the
  rendered base are the same scene (the 040 inconsistency). A failed `Transaction` restores `live_owner`
  together with its `.0` snapshot. `scene_graph_tick` and `render_universes` use the wrapper's
  `render_base_scene()`; `apply_scene_graph_node_controls` takes `base` as an argument instead of recomputing it.
- **UI.** Programmer `live_mode_toggle(ui, &EngineState, size)`: the button is "on" when
  `live_owner() == Some(client.session())`; the red dot shows whenever any session previews, with a hover text
  for "another operator is previewing". Performance (`view_perf.rs`) shows `render_base_scene()` while live,
  instead of `current_scene_focus`; its banner already keys off `live_mode` (any preview).
- **Docs.** `session_scope` comment in `hub.rs`: live-mode events stay `Global` (one rig; every session sees the
  overlays suppressed, and plugins and the desk see the event). `korg.rs` got a comment that with another
  operator previewing, plugins see `live_mode` with the `DESK` focus (comment only, plugin not rebuilt).

### Decisions and deviations
- **Deleting the previewed scene ends the preview, for `DESK` too.** Before, deleting the desk's focused scene
  while live moved the focus and the rig jumped to that scene. The ticket asks for the preview to clear; I applied
  that to every owner rather than special-casing `DESK`, so single-operator behaviour differs in this one edge
  case.
- **Takeover:** `SetLiveMode(true)` / `ToggleLiveMode` from a non-owner while live take the preview over (last
  write wins). A takeover restarts the new base scene's animations, like entering live mode does.
- `live_owner` sits in a third tuple field rather than inside `SessionStore`, so 040's store code is unchanged;
  `swapped_in` shares the struct.
- Live-mode events remain `SessionScope::Global` (no change to `session_scope` or to dirty marking).

### Verification
- `cargo test --workspace --all-features`: all pass; blaulicht-core 280 → 285. New in
  `dmx::session::engine_tests` (real `DmxEngine`, DMX output read back from `dmx_universes`):
  - `the_live_preview_renders_its_owners_focus`: desk toggle on/off as before; a remote `SetLiveMode(true)`
    renders the remote's red scene 2 while the desk focus (blue scene 0) stays in `current_scene_focus`; the
    preview follows the owner's focus; takeover by toggle in both directions; `SetLiveMode(false)` from a
    non-owner ends it.
  - `closing_the_owner_session_ends_the_preview` (and closing a non-owner leaves it).
  - `deleting_the_owners_focused_scene_ends_the_preview` (another scene's deletion keeps it; also for `DESK`).
  - `a_failed_transaction_restores_the_preview_owner`, `loading_a_show_ends_the_preview`.
  - Extended: `sessions_open_and_close_through_the_hub` (a real remote client turns live on over the bus, then
    dropping it clears the preview via `apply_bus_messages`), and `swapping_twice_restores_both_sessions`
    (lookups by id while swapped).
- `cargo clippy --workspace --all-targets --all-features`: blaulicht-core lib 141, lib test 160 (unchanged), none
  in new code. `cargo fmt` clean for touched files; the usual unrelated hunks (`audio_features`,
  `pages/audio.rs`, `visualizer/ui.rs`, `app/ui.rs`) were reverted.
- blenv (kiosk, copy of `Kuze_Theater.json`, screenshots read):
  - Programmer before: LIVE off, no dot.
  - Clicked LIVE (desk's own send): button highlighted with the red dot; inspector `live_mode: true`.
  - Performance with LIVE on: red banner, and the single card is the previewed scene "Empty Scene (0)".
  - Clicked LIVE again: off, no dot, `live_mode: false`.
  - `blctl event {"SetLiveMode":true}` (inspector plugin, `DESK` session): button on with dot, so plugins and
    the desk share ownership as before.
  - No errors in the app log besides the usual missing DMX devices.
  - Not checked live: a remote owner (no in-tree client opens a second session; covered by the tests above).

### Follow-ups
- **042:** `EngineClient` reads the toggle state as `live_owner() == Some(session)`; `clone_for_ui` copies
  `SessionMeta`. A remote frontend's Performance view can use `render_base_scene()` as-is.
- **Plugins:** they can't tell who previews or which scene renders (`live_mode` + `DESK` focus only). Exposing the
  rendered base scene to plugins would need a shared field and an ABI bump; korg's faders drive the desk's focus
  while a remote operator previews.
- 043+: a `HostCommand` that deletes scenes can call the wrapper's `delete_scene` inside `dispatch`; the owner
  check is swap-aware now.
