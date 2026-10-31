# 052 — Engine owns showfile, config and lifecycle

| | |
|---|---|
| Epic | E10 Decouple |
| Depends on | 049, 042, 051, 043, 044, 045, 050 |
| ABI / showfile | showfile shape unchanged |

**Touches:** `crates/core/src/app/ui.rs`, `crates/core/src/app/event.rs`, `crates/core/src/app/pages/system/showfile.rs`, `crates/core/src/config.rs`, `crates/core/src/state.rs`, `crates/core/src/main.rs`, new `crates/core/src/persistence.rs`

## Problem
The frontend owns persistence:
- Dirty tracking runs on the UI's bus connection (`control_event_marks_showfile_dirty`, `app/event.rs:36–48`,
  `:214`).
- The UI builds and autosaves the showfile (`build_showfile` `ui.rs:579`, autosave thread `:350–445`,
  `write_atomic` `:409`) and writes config (`:433`).
- Load, new and close are in `system/showfile.rs:62–103`. They also mutate `plugin_state_storage` (`:72`, `:85`)
  and the grand master (`:61`, `:81`).
- Global plugin state is written to `config.toml` in the UI's event handler (`app/event.rs:162–180`).
- Quit resets the grand master from the UI (`ui.rs:530`), and Restart calls `std::process::exit(42)`
  (`ui.rs:534`, `:566`).

A headless engine would never save, and two frontends would both autosave.

## Scope
- **Engine-side persistence service:**
  - It builds the showfile from show state (engine, stage, Art-Net, plugin state), plus the desk frontend's opaque
    `ui` / `external_screens` section, which that frontend publishes with a `HostCommand::SetFrontendSection`.
  - It owns dirty tracking (based on applied commands from any client) and the autosave timer.
  - It performs load, new and close as `HostCommand`s, with `CommandResult` for errors.
- On load, the engine broadcasts `ShowLoaded { generation }` on the hub. Sessions are repaired (040), and
  frontends reset view state that holds ids.
- **Config is engine-owned and has one writer:** `last_open_showfile`, `default_audio_device` (from 045), global
  `plugin_state` (`SavePluginState{Global}` handled in the engine) and `plugins[].enabled`.
- **Lifecycle:** quit and restart become `HostCommand`s. The engine runs a shutdown hook (grand master reset,
  final save), and then the native binary exits (with 42 for restart).
- **Compiler fence:** `BlaulichtApp` no longer holds `Arc<AppState>` or `AppStateWrapper`. Only `LocalClient`
  does, privately. Anything that still needs it fails to compile, so fix it here.

## Out of scope
- The crate split (053).

## Acceptance criteria
- [ ] `grep -rn 'AppState\b\|AppStateWrapper\|\.config\.lock' crates/core/src/app` finds nothing outside tests.
- [ ] Autosave, manual save, load, new and close work as before; a save after a plugin's global state write
      persists it.
- [ ] Unit test: two clients' edits both mark the show dirty, and one autosave writes both.

## Verification
- `cargo clippy --workspace --all-targets --all-features`, `cargo fmt`, `cargo test --workspace --all-features`.
- Showfile: load a scratch copy, edit, wait for autosave, reload, `blctl state`.
- blenv: System › Showfile screenshots for load and save.

## Notes
