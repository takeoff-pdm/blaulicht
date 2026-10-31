# 039 — Move view state out of AppState

| | |
|---|---|
| Epic | E10 Decouple |
| Depends on | none |
| ABI / showfile | none (plugin host functions keep their signatures) |

**Touches:** `crates/core/src/state.rs`, `crates/core/src/app/mod.rs`, `crates/core/src/app/event.rs`, `crates/core/src/app/plugin_ui.rs`, `crates/core/src/app/external_screen.rs`, `crates/core/src/app/pages/system/screens.rs`, `crates/core/src/plugin/wasm.rs`

## Problem
`AppState` is the engine's shared state, but it also holds state that belongs to one frontend: which plugin UI
windows are open, popped out or maximized, which plugin tab is selected, and which external screens exist. With
several frontends, each one has its own windows and screens, so this state has to live in the frontend.

## Scope
- Move these fields from `AppState` into a frontend-owned struct (for example `NativeViewState`, owned by
  `BlaulichtApp`):
  `plugin_ui_visibility`, `plugin_ui_popped_out`, `plugin_ui_maximize_requested`, `plugin_ui_tabs_selected`,
  `external_screens`.
- Plugin host functions that read or change this state (`ui_is_open`, `ui_maximize_screen`,
  `ui_list_external_screens`, `ui_create_external_screen`, `ui_remove_external_screen`, `wasm.rs:931–1060`)
  stop touching it directly:
  - Requests (open, maximize, create or remove a screen) become messages to the **desk frontend** (the native UI)
    over a channel, the same way `ControlEvent::MainUi` already reaches the app (`app/event.rs`).
  - Queries (`ui_is_open`, `ui_list_external_screens`) read a small engine-side mirror that the frontends
    publish. `ui_is_open` means "open in at least one frontend". `ui_list_external_screens` lists the desk
    frontend's screens.
- The showfile's `external_screens` (`config.rs:102`) is still loaded and saved, but the desk frontend owns it at
  runtime.

## Out of scope
- New plugin UI host functions. 029 adds those, so don't run it at the same time; it touches `app/event.rs`
  and `plugin_ui.rs` too.
- Visualizer camera state is already UI-local; leave it alone.

## Key code
- `AppState` fields — `crates/core/src/state.rs:313`
- `SetPluginUIOpen` handling — `crates/core/src/app/event.rs:~44–60`
- `notify_plugin_ui_open` — `crates/core/src/app/plugin_ui.rs:9`
- Host functions — `crates/core/src/plugin/wasm.rs:931`, `:943`, `:1009`, `:1035`, `:1060`

## Acceptance criteria
- [ ] None of the five fields exist on `AppState` any more.
- [ ] A plugin can still open and maximize its UI and create and remove an external screen, verified live, for
      example with `console` or `screens`.
- [ ] `ui_is_open` reports correctly when the plugin window is opened and closed in the native UI.

## Verification
- `cargo clippy --all-targets --all-features`, `cargo fmt`, `cargo test --all-features`.
- UI: screenshots per `crates/core/AGENTS.md` of a plugin window opened by the plugin, and of an external screen
  created by a plugin.

## Notes
