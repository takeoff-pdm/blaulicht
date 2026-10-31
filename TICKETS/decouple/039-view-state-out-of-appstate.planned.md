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
- [ ] No `plugin_ui_*` field exists on `AppState`, and no frontend code takes a write lock on the mirror
      (checked with grep).
- [ ] A plugin-sent `SetPluginUIOpen` opens the window, including for a plugin with no previous entry.
- [ ] Pop-out, maximize and per-screen assignment (`PluginOpenState.screen_id`) still work.
- [ ] Unit tests cover the mirror (open in any frontend, cleared on disconnect) and the request routing.

## Verification
- `cargo clippy --workspace --all-targets --all-features`, `cargo fmt`, `cargo test --workspace --all-features`
  (in the blenv container).
- No plugin in the tree calls `ui_is_open` or `ui_maximize_screen`, so test those host functions with unit
  tests. Live: open, pop out and close a plugin window from the Plugins page (blenv screenshots), and check that
  `console`'s `SetPluginUIOpen` request still opens its window.

## Notes
