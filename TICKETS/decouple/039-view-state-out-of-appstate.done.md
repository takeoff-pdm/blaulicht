# 039 — Move view state out of AppState

| | |
|---|---|
| Epic | E10 Decouple |
| Depends on | none |
| ABI / showfile | none: desk requests use a host-only channel, and no `MainUiEvent` variant is added |

**Touches:** `crates/core/src/state.rs`, `crates/core/src/app/mod.rs`, `crates/core/src/app/event.rs`, `crates/core/src/app/plugin_ui.rs`, `crates/core/src/app/external_screen.rs`, `crates/core/src/app/pages/system/misc.rs`, `crates/core/src/app/pages/animation_spec_editor.rs`, `crates/core/src/plugin/wasm.rs`, `crates/core/src/msg.rs`

## Problem
`AppState` is the engine's shared state, but it also holds view state that belongs to one frontend:
`plugin_ui_visibility`, `plugin_ui_popped_out`, `plugin_ui_maximize_requested` and `plugin_ui_tabs_selected`
(`state.rs:336–339`). Plugin host functions and the UI both write them directly.

`external_screens` (`state.rs:348`) is already a mirror. The real screens live in
`BlaulichtApp.external_screens` (`app/mod.rs:151`), and the UI copies their info into `AppState` with a write
lock (`external_screen.rs:515–518`). Create and remove already go out as `MainUiEvent`s
(`wasm.rs:1045`, `:1078`).

## Scope
- Move the four `plugin_ui_*` fields into a frontend-owned `NativeViewState` on `BlaulichtApp`. `render_plugin_ui`
  mutates them inside `show_viewport_immediate` closures (`plugin_ui.rs:104`, `:158`, `:165`), and
  `render_plugin_ops` is a recursive free function that writes `tabs_selected` (`:667`, `:691`). Restructure
  both: collect actions and apply them afterwards, or pass `&mut NativeViewState`.
- **Engine-side mirror:** `AppState` keeps a read-only `FrontendUiMirror { open_plugins, external_screens }`,
  keyed per frontend (just the desk for now). Frontends publish to it by message (a new `SystemMessage`-style
  host-only channel or `FromFrontend` variant), not by write lock. When a frontend disconnects, its entries are
  cleared.
  - `ui_is_open` = open in any frontend.
  - `ui_list_external_screens` = the desk frontend's screens.
- **Requests from plugins go to the desk frontend only**, over a host-only channel, following the
  `SystemMessage::PluginAlert` pattern (`wasm.rs:997`, `event.rs:181`). This covers `ui_maximize_screen`
  (today it writes view state at `wasm.rs:947–962`) and plugin-origin `SetPluginUIOpen`. Create and remove
  external screen already go through `MainUiEvent`; leave them.
- **`SetPluginUIOpen` is used both as a request and as a notification.** Plugins send it as a request
  (`console/src/lib.rs:712`, `midi_all/src/legacy/mapping.rs:1099`). The UI sends it as a status notice with
  `Originator::Web` (`plugin_ui.rs:14`, `misc.rs:470`, `external_screen.rs:706`). Keep plugin-origin
  requests, but replace the UI's notices with the mirror update.
- Fix the `Entry::Occupied` bug in the `SetPluginUIOpen` handler (`event.rs:56–62`): a plugin whose visibility
  entry doesn't exist yet couldn't open. 029 mentions this bug too; fixing it here is fine.
- **No desk frontend connected** (headless after 053): `ui_maximize_screen` and the create and remove calls
  are dropped with a debug log and return 0, and `ui_is_open` returns false.
- Plugin tab selection is still reported to plugins with `PluginUiEvent::TabChanged` (`plugin_ui.rs:695`). With
  several frontends, last write wins for the plugin's copy. Document this.

## Out of scope
- `wasm_animation_editor_requests` (043) and `pending_fixture_inits` (043). Both are frontend writes to engine
  state, not view state.
- `plugin_ui_ops` and its back buffer, `wasm_animation_ui_ops` and `logs` are engine output (042, 049).

## Key code
- Fields — `crates/core/src/state.rs:336–348`; constructor `:379–415`
- Host functions — `crates/core/src/plugin/wasm.rs:931`, `:943`, `:1009`, `:1035`, `:1060`
- Show/hide toggle — `crates/core/src/app/pages/system/misc.rs:375`, `:453`
- External screen sync — `crates/core/src/app/external_screen.rs:515–535`, `:687–707`

## Acceptance criteria
- [x] No `plugin_ui_*` field exists on `AppState`, and no frontend code takes a write lock on the mirror
      (checked with grep). (`plugin_ui_ops` / `plugin_ui_ops_back` stay: engine output, out of scope.)
- [x] A plugin-sent `SetPluginUIOpen` opens the window, including for a plugin with no previous entry.
- [x] Pop-out, maximize and per-screen assignment (`PluginOpenState.screen_id`) still work. (Maximize: unit
      tests only, no plugin in the tree calls it; see Notes.)
- [x] Unit tests cover the mirror (open in any frontend, cleared on disconnect) and the request routing.

## Verification
- `cargo clippy --workspace --all-targets --all-features`, `cargo fmt`, `cargo test --workspace --all-features`
  (in the blenv container).
- No plugin in the tree calls `ui_is_open` or `ui_maximize_screen`, so test those host functions with unit
  tests. Live: open, pop out and close a plugin window from the Plugins page (blenv screenshots), and check that
  `console`'s `SetPluginUIOpen` request still opens its window.

## Notes

### What changed
- **`crates/core/src/frontend_ui.rs` (new, engine side):** `FrontendId` (`DESK` = 0), `FrontendUiSnapshot
  { open_plugins: BTreeSet<u8>, external_screens }`, `FrontendUiMirror` (on `AppState` as `frontend_ui`,
  replaces `AppState.external_screens`), `FrontendUiPublisher`, `DeskRequest`, and the routing helpers
  `send_desk_request`, `desk_requests_in_plugin_event`, `route_plugin_event_to_desk`.
  - Frontends publish by message: `connect(id)` returns a publisher that sends `Connected`, `publish(snapshot)`
    sends only when the snapshot changed (so the desk calls it every frame), and `Drop` sends `Disconnected`,
    which clears that frontend's entries. The mirror's map is a private `Mutex`, written only when the engine
    reads (`is_plugin_open`, `desk_connected`, `desk_external_screens` drain pending updates first).
    Nothing outside the module can lock it. There is no extra engine thread or tick hook. Publishing on change
    keeps the queue tiny even if no plugin reads for a long time.
  - The desk connects in `main.rs` **before** the supervisor starts, so plugins that create an owned
    external screen in their init still find a desk. `BlaulichtApp` owns the publisher (`desk_ui`), so
    closing the app disconnects the desk.
- **`crates/core/src/app/view_state.rs` (new, frontend side):** `NativeViewState { plugin_visibility,
  plugin_popped_out: HashSet, plugin_maximize_requested: HashSet, plugin_tabs_selected }` on `BlaulichtApp`.
  `PluginOpenState` moved here from `state.rs`. `ScreenId` stays in `state.rs` (used across `app/`; 053 can
  move it).
- **Host functions (`plugin/wasm.rs`):**
  - `ui_is_open` = open in any frontend (false with no frontend).
  - `ui_maximize_screen` sends `DeskRequest::MaximizePluginUi`, or drops it with a debug log when no desk is
    connected. It no longer writes view state, and no longer sends a `SetPluginUIOpen` bus notice.
  - `ui_list_external_screens` returns the desk's screens.
  - `ui_create_external_screen` / `ui_remove_external_screen` return 0 with a debug log when no desk is
    connected. Otherwise they still send their `MainUiEvent`s, unchanged.
  - `bl_send_event`: a plugin-sent `SetPluginUIOpen`, also inside a `Transaction`, is additionally routed to the
    desk as `DeskRequest::SetPluginUiOpen`. The event still goes on the bus unchanged, so other plugins keep
    seeing it.
- **Desk (`app/event.rs`):** `SystemMessage::DeskRequest` (new host-only variant in `msg.rs`) is applied to
  `NativeViewState`. The bus `MainUiEvent::SetPluginUIOpen` arm is now a no-op, so a request is not applied
  twice. This also fixes the `Entry::Occupied` bug: `set_plugin_open` inserts a `CLOSED` entry first. Since
  `NativeViewState` starts empty (the old constructor pre-filled an entry per configured plugin), without the
  fix no plugin could have been opened by request.
- **The UI's `Originator::Web` `SetPluginUIOpen` notices are gone** (`notify_plugin_ui_open`, the
  Plugins-page toggle, external screen tab tracking). Plugins learn the state through `ui_is_open`. No plugin
  in the tree listened to those notices (checked with grep: `console` and `midi_all` only send it).
- **`render_plugin_ui`** is now `&mut self`. The viewport and window closures only record `pop_toggled` /
  `close_requested`, and these are applied after the closure returns. `render_plugin_ops` takes
  `&mut PluginTabSelections` and passes it down the recursion. The animation editor (`AnimationEditState::show`,
  called from Programmer and Setup > Animations) passes `&mut self.view_state.plugin_tabs_selected` through.
- **Tab selection:** still reported with `PluginUiEvent::TabChanged`. With several frontends each keeps its
  own selection, and the plugin's copy follows whichever changed last (last write wins). This is documented on
  `PluginTabSelections`.

### Verification
- `cargo clippy --workspace --all-targets --all-features`: builds and finds no new warnings. The only warning
  that changed is the existing `too_many_arguments` on `AnimationEditState::show` (8 to 9 args). `blaulicht-core`
  lib warning count: 84.
- `cargo fmt` is clean for the touched files. Note: `cargo fmt` also reformats files this ticket didn't touch
  (`audio_features`, `pages/audio.rs`, `visualizer/ui.rs`, one hunk in `app/ui.rs`). I reverted those, so they are
  not in this change.
- `cargo test --workspace --all-features`: all pass. blaulicht-core went from 241 to 251 tests: 7 in `frontend_ui`
  (open in any frontend, cleared on disconnect, desk-only screens, connect before first snapshot and no resend
  of unchanged snapshots, `SetPluginUIOpen` routing including inside a `Transaction`, maximize routing, requests
  dropped without a desk even with another frontend connected) and 3 in `view_state` (request opens a plugin
  with no entry, open keeps the screen, maximize opens popped out and the maximize request fires once).
- The host functions themselves are thin wrappers over these tested functions. The wasmtime linker closures
  are not called from a unit test.
- Live (blenv, screenshots read):
  - Kiosk mode, Plugins page: "Show UI" opens the sample egui window, "Pop out" opens the separate 800x600
    viewport, a tab clicked in the popped-out viewport ("Details") is still selected after "Pop in", and the
    window's close button closes it (the button reads "Show UI" again).
  - `blctl event SetPluginUIOpen{plugin_id:5}` (inspector, plugin origin) opened `console` with no previous
    entry. `console`'s own Main UI > Set Plugin UI Open > Send Event opened the inspector window (id 0).
  - `-e 1280,720`: the external screen's active plugin tab opens that plugin with the external `screen_id`: no
    window on the main screen, and the Plugins page shows "Hide UI". Switching the tab moves the open state to
    the other plugin.
  - Desktop mode: plugin tab switching inside a docked plugin pane works.
  - Maximize could not be checked live (no plugin calls `ui_maximize_screen`).

### Follow-ups
- **049:** replace `FrontendId` with `ClientId`. Connect and disconnect should follow client lifetime: a remote
  client's publisher drops when it disconnects. `DeskRequest` currently rides on the desk-only
  `SystemMessage` channel. When the hub exists, route it to the desk client's subscription instead.
  `SetPluginUIOpen` routing is only done in `bl_send_event`, so an event injected on the bus by anything other
  than a plugin no longer opens a window. Today nothing does that. `NavigatePage` / `NavigateSubPage` from
  plugins still go over the bus to the desk (029).
- **040/042:** the mirror is per frontend, not per session. `EngineClient` should expose
  `publish_ui_snapshot`, or the `FrontendUiPublisher` itself, to frontends.
- **053:** `ScreenId` (view state) still lives in `state.rs`. `frontend_ui.rs` belongs in the engine crate,
  `app/view_state.rs` in the egui crate.
- Connecting the same `FrontendId` twice is unsupported: the first publisher's drop disconnects both.
- Plugin tab selection is keyed `(plugin_id, tabs_id)` and shared between a plugin's own window and its
  animation-editor instances (as before).

