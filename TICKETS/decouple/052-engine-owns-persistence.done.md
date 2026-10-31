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
- [x] `grep -rn 'AppState\b\|AppStateWrapper\|\.config\.lock' crates/core/src/app` finds nothing outside tests.
- [x] Autosave, manual save, load, new and close work as before; a save after a plugin's global state write
      persists it.
- [x] Unit test: two clients' edits both mark the show dirty, and one autosave writes both.

## Verification
- `cargo clippy --workspace --all-targets --all-features`, `cargo fmt`, `cargo test --workspace --all-features`.
- Showfile: load a scratch copy, edit, wait for autosave, reload, `blctl state`.
- blenv: System › Showfile screenshots for load and save.

## Notes

Done in one part (no split): persistence service, dirty tracking, autosave, config, load/close/save/save as,
`ShowLoaded`, the frontend section, quit/restart and the compiler fence.

### What changed
- **`crates/core/src/persistence.rs` (new): the engine's persistence service.**
  - `PersistenceLink` lives on `AppState` (`app.persistence`): `mark_edited()` / `edits()` (an atomic counter),
    `forward(client, seq, HostCommand)` (used by `DmxEngine::dispatch`), `save_global_plugin_state(key, value)`
    (used by the plugin host) and `exit_request()`. It works without a running service (requests queue up).
  - `PersistenceService::new(app, hub, SharedConfig)`, `open_startup(Option<PathBuf>)` (synchronous, in `main`),
    `spawn() -> PersistenceThread` (thread `persistence`, wakes on a request or every 200 ms),
    `PersistenceThread::stop() -> Option<ExitRequest>` (handles the queued requests first). Tests step it with
    `step(now)` instead of spawning it (deterministic timers).
  - **No IO under the engine lock.** The service builds the showfile under short, separate read locks (engine:
    clone of `.0` and the stage; plugin storage; Art-Net), then serializes, hashes and writes without any lock.
    A loaded file is read and parsed first (`config::parse_showfile`), then installed under the engine + Art-Net
    write locks (`config::install_showfile`, same lock order as before: engine, Art-Net, plugin storage). The
    config is only written with no other lock held (`SharedConfig` is documented as a leaf lock); the supervisor
    still writes `default_audio_device` through the same `SharedConfig`.
  - **Dirty tracking** (`SaveStatus { NoShowfile, Clean, Dirty, Saving, Failed(String) }`):
    `DmxEngine::dispatch` calls `mark_edited()` after every *applied* (`Ok`) message that changes saved data,
    from any client: `HostCommand::edits_show()`, or `persistence::control_event_marks_showfile_dirty(event,
    session == DESK)` (moved here from `app/event.rs`, with its two tests). The service turns a new edit into
    `Dirty` on its next step, and keeps the old 2 s hash check (`DIRTY_CHECK_INTERVAL`) against the last
    loaded/saved hash: it catches changes no command reports (plugins' showfile state) and clears an undone
    edit. `Failed` stays until a save succeeds or the show matches the file again (before, the desk's check
    turned it into `Dirty` after 2 s). Autosave every 30 s (`AUTOSAVE_INTERVAL`) when `Dirty`/`Failed`.
  - **Status on the hub:** `SystemMessage::ShowfileStatus(ShowfileStatus { path, save, saved_at:
    Option<SystemTime> })`, published on change; `SystemMessage::ShowLoaded { generation, frontend_section:
    Option<ShowfileUiState> }` on every load/close and at startup (generation 1). Both are replayed (latest) to
    late subscribers (`hub::Replay`, fixed replay size + 2). `SystemMessage::Exiting { restart }` after the
    shutdown hook.
  - **Config writes:** `last_open_showfile` on load, Save As and close (not at startup; not after every save
    any more, since the path didn't change), global plugin state (coalesced per step). `plugins[].enabled` is
    never written by anything (no UI toggles it); it is kept as read.
- **New `HostCommand`s** (`hub.rs`; `dispatch` forwards them to the service before swapping the session in;
  the service answers them itself with a `CommandResult` once done; `apply_host` has a defensive arm). All
  `edits_show() == false`.
  - `SaveShowfile` → `Refused("No showfile is open" | "Save failed: <why>")`. Writes only if the show changed.
  - `SaveShowfileAs(PathBuf)` → copies model assets next to the new file, points the stage's assets at the
    copies (`EngineState::relocate_stage(&relocated, path)`, which now merges asset paths by id instead of
    replacing the stage, so an edit made while the copies are written survives), always writes, updates the
    config. `Refused("Failed to copy stage assets: <why>" | "Save failed: <why>")`.
  - `LoadShowfile(PathBuf)` → `Refused("Could not load <file>: <why>")` with nothing changed; else resets the
    grand master, installs, `ShowLoaded`, config.
  - `CloseShowfile` (= "new"): an empty show without a showfile, grand master reset, `ShowLoaded`, config entry
    removed. There is no separate "new" command: closing *is* the new, empty show (the desk has no New button).
  - `SetFrontendSection { generation, section: ShowfileUiState }` → answered `Ok` at once. Ignored unless
    `generation` is the current one. The first section for a generation is the frontend's form of the loaded
    one: stored, and if nothing was edited since the load the baseline hash is recomputed with it (so the
    desk's normalisation of an old section doesn't show the show as unsaved right after a load). Later changed
    sections mark the show edited (replaces the desk's `mark_showfile_dirty()` in the dock context menu).
  - `Quit { save }` / `Restart { save }`: the shutdown hook — save if `save` and a showfile is open (refused
    with "Save failed: …", then nothing else happens), grand master to 100, record `ExitRequest`, publish
    `Exiting`. `main` then exits: after `run_native` returns it calls `PersistenceThread::stop()` (which resets
    the grand master if no Quit/Restart ran, e.g. the window was closed), `hub.shutdown()`, and
    `std::process::exit(42)` for `ExitRequest::Restart` (`ExitRequest::code()`).
- **One "open showfile" field (051/045 follow-up):** `StageState.showfile` is the engine's only copy;
  `EngineState::stage_showfile()` is renamed `showfile()`, and `EngineClient::showfile_path()` reads it.
  `config.last_open_showfile` is written from it. They differ only when the startup showfile can't be read:
  the engine then has an empty show without a showfile, and the config keeps its entry so the next start
  retries (before, the desk kept the config path and the next autosave wrote an empty show over the broken
  file).
- **`config.rs`:** `read_showfile`/`read_showfile_logic`/`close_showfile`/`read_showfile_ui_state`/
  `LoadedShowfileState` are replaced by `ParsedShowfile` (+ `ParsedShowfile::empty()`), `parse_showfile(&Path)`
  and `install_showfile(parsed, file, &mut dmx, &mut artnet, &plugin_storage) -> Option<ShowfileUiState>`.
  `ShowfileUiState` and its parts derive `PartialEq`. Showfile shape unchanged.
- **Plugin host:** `bl_save_plugin_state` with `Global` hands the state to the service
  (`save_global_plugin_state`), which writes `config.toml` — with or without a desk (049's "lost with a
  warning" is gone). `SystemMessage::SavePluginState` is removed (host-only, not ABI).
- **`main.rs`:** the startup showfile is loaded by the service *before* the supervisor starts (it used to race
  with plugin init, so a plugin could initialize before its showfile state was in storage); the desk is
  connected before that, so it receives the startup `ShowLoaded`. `BlaulichtApp::new(cc, client, popup,
  desktop_mode, showfile_home, config_dir, external_screens)`.
- **Desk (compiler fence):** `AppStateWrapper` is deleted; `BlaulichtApp` holds only `client: LocalClient`
  (which keeps its `Arc<AppState>` private). Removed: `data`, `ShowfileSaveStatus`, the save/dirty-check
  threads and channels, `build_showfile`, `tick_autosave`, `mark_showfile_dirty`, `reset_save_tracking`,
  `lifecycle_action_after_save`, `last_save_time`.
  - `save_status: SaveStatus` / `saved_at` come from `ShowfileStatus` (unsaved dot, "(saved Xs ago)", the
    close/load guards). Save → `SaveShowfile` ("Saved Showfile" popup on `Ok`, refusal popup otherwise); Save
    As → `SaveShowfileAs`; Load → `LoadShowfile` ("Loaded Showfile" / refusal); Close → `CloseShowfile`. File
    dialogs start in `showfile_home`, else the open showfile's folder, else `config_dir` (new field, from
    `main`).
  - `ShowLoaded` → `apply_show_loaded`: resets id-holding view state on a new generation
    (`reset_show_view_state`: visualizer `reset_stage_view`, Setup fixture/group/dialog targets, View, View
    Performance, Animations, Scene graph, Palettes and the three Setup folder UIs, the Programmer's palette
    cache and animation selection edit), applies the frontend section, and republishes its own.
  - `publish_frontend_section()` at the end of every frame sends `SetFrontendSection` when the section
    changed (after the first `ShowLoaded`).
  - Quit/Restart: `request_guarded_lifecycle(action)` sends `Quit/Restart { save: true }` when the desk thinks
    the show is clean (the engine's final save catches a late edit), else shows the guard; Save → `{ save:
    true }`, Discard → `{ save: false }`; a refusal shows the popup and the guard again. `Exiting` (from any
    client's Quit) closes the window.
  - Remaining reads moved to readers: plugin log source and plugin tab ids (`with_plugins`),
    `plugin_ui_title(&impl EngineClient, id)` (`with_plugins`), plugin windows (`with_plugin_ui_ops`, now
    copying only the ops of windows open on that screen; `with_plugin_runtime_kinds`), the audio graphs
    (`with_spectrogram`). `render_system_general` lost its unused `ctx`.

### Decisions and deviations
- **Commands are answered by the service, not by `apply_host`:** `dispatch` returns no result for them and the
  service publishes the `CommandResult` to the sender when the file work is done (so "Saved Showfile" means
  written). This keeps all file IO off the main loop thread and out of the engine lock.
- **`SetFrontendSection` carries a generation** (not in the ticket text): without it, a section published
  before the desk had applied a loaded one could overwrite it, and the first publish after a load couldn't be
  told from an edit.
- **The frontend section stays typed** (`config::ShowfileUiState`) instead of opaque JSON: the engine stores,
  compares and saves it but never reads its fields. Making it a `serde_json::Value` would reorder its keys in
  the file (serde_json without `preserve_order`); 053 can do it when moving the type to the egui crate.
- **Save As always writes** the new file. Before, an unchanged show (hash equal to the last save) wasn't
  written, so the config pointed at a file that didn't exist. A failed Save As write leaves the engine on the
  new path with status `Failed` (as before); the config keeps the old path.
- **A failed load changes nothing.** Before, the desk switched `last_open_showfile` to the broken file even
  though the show stayed the old one, so the next autosave wrote the old show into it.
- **A manual save of an unchanged show** answers `Ok` (popup "Saved Showfile") without writing; before it showed
  nothing.
- **Dirty marking only counts applied commands** (refused ones changed nothing); before, the desk marked any
  event it saw on the bus.
- **Plugin showfile-state saves don't bump the edit counter** (some plugins save every tick; the 2 s hash check
  finds real changes, as before).
- Outside Touches: `hub.rs`, `msg.rs`, `client.rs` (doc, test harness `TestEngine::persistence()` / `hub()`),
  `dmx/mod.rs`, `dmx/stage_commands.rs`, `plugin/wasm.rs`, `lib.rs`, `app/{mod,plugin_ui,external_screen}.rs`,
  `app/pages/system/misc.rs`, `app/pages/visualizer/ui.rs` (rename), `crates/core/AGENTS.md`.

### Verification
- `cargo test --workspace --all-features`: all pass; blaulicht-core 340 → 349. New `persistence::tests` (real
  `DmxEngine`, bus and hub via `TwoClients`/`TestEngine`, the service stepped by the test, files in a temp dir):
  - `two_clients_edits_mark_the_show_dirty_and_one_autosave_writes_both` (the acceptance test): a desk
    `RenameGroup` and a remote `RenameScene` + `SetAlpha` each bump the edit counter (a remote `SelectGroup` and
    a refused command don't), status `Dirty`, the file is untouched until one autosave writes both; then a
    plugin showfile-state change is found by the hash check and autosaved; an edit undone before the check is
    `Clean` again.
  - `saving_without_a_showfile_is_refused_and_save_reports_when_done` (refusal, Save As + config, manual save
    and the published status with `saved_at`, a failing write → `Failed`).
  - `loading_and_closing_replace_the_show_and_announce_it` (missing file refused with nothing changed; load
    resets the grand master and the remote's selection, config, `ShowLoaded` generation 2; close → empty show,
    `NoShowfile`, config entry gone, `ShowLoaded` 3).
  - `the_frontend_section_is_saved_and_handed_back_on_load` (first section not an edit, stale generation
    ignored, change marks dirty, saved as `ui`, handed back in `ShowLoaded`, replay of `ShowLoaded` and
    `ShowfileStatus` for a late subscriber).
  - `save_as_copies_model_assets_next_to_the_new_file` (Box.glb imported next to `first.json`, Save As
    `second.json` → `second.assets/<hash>.glb`, engine asset path relocated).
  - `global_plugin_state_reaches_the_config_without_a_frontend` (headless, coalesced).
  - `quit_and_restart_run_the_shutdown_hook` (Restart without save: file untouched, grand master 100,
    `ExitRequest::Restart`, code 42, `Exiting { restart: true }`; Quit with save writes the edit).
  - `a_quit_whose_save_fails_is_refused` (no exit request, grand master untouched).
  - `a_startup_showfile_that_cant_be_read_leaves_an_empty_show`.
- `cargo clippy --workspace --all-targets --all-features`: blaulicht-core lib 129, lib test 147, bin 1
  (before: 130 / 148 / 1; one pre-existing warning was in replaced desk code). None in new code.
  `cargo check -p blaulicht-core --no-default-features --features "x11 audio-mock wasmtime"` passes.
- `cargo fmt`: touched files clean; the usual unrelated hunks (`audio_features`, `pages/audio.rs`,
  `visualizer/ui.rs`, the `model_peak` hunk in `app/ui.rs`) reverted.
- **blenv** (kiosk 800x480, copies of `Kuze_Theater.json`; screenshots read):
  - Startup with `kuze052.json`: log "Loaded showfile" from `blaulicht_core::persistence`; no unsaved dot after
    load nor after 5 s (the old `ui` section is normalised by the desk; the rebase keeps it clean). System
    General shows `kuze052.json`.
  - Autosave: `blctl event SelectGroup(6)` + `SetAlpha(201)` (plugin-originated) → yellow dot at once; the next
    30 s tick logged "Saved showfile", the dot went away and General showed "(saved 6s ago)". The saved JSON has
    `format_version`, `stage`, the (normalised) `ui` section, the three `plugin_state` entries unchanged and
    alpha 201.
  - Manual save: `SetAlpha(150)` → Save Showfile → "Saved Showfile" popup, log line.
  - Save As through the file dialog (opens in `/state/showfiles`, the open showfile's folder) as
    `kuze052b.json`: written although unchanged, "Saved Showfile", General shows `kuze052b.json`,
    `/state/config.toml` has `last_open_showfile = "/state/showfiles/kuze052b.json"`.
  - Close Showfile → confirm → "N/A", Close/Save buttons disabled, `last_open_showfile` gone from the config,
    log "Closed showfile".
  - Load Showfile (dialog starts in the config folder without a showfile) → `kuze052b.json` → Replace dialog →
    Load → "Loaded Showfile", config updated; `blctl state` shows the four Par Truss fixtures at alpha 150 and an
    empty desk selection.
  - Load of a broken file (`{ not a showfile`) → popup "Could not load broken052.json: key must be a string at
    line 1 column 3", show and config unchanged.
  - Add Screen (System) → dot (section change marks the show edited); room width 20 → 35 m in the Visualizer
    (stage `PatchStage`) → autosaved: JSON `stage.room.width` 35, `ui.external_screens` with one 1920x1080 screen.
  - Restart of the app with the kept config (startup load of `kuze052b.json`): `bl_ext_0` window restored from
    the `ui` section, Visualizer width 35.0 m.
  - Global plugin state: built `udp_listener` into `bundle_debug`, added it to the container config (plugin 5),
    `blctl event PluginUi Text "9123"` + `Button` (Bind) → `config.toml` got `plugin_5 = '{"version":1,"user":
    "{\"port\":9123}",…}'` right away (no desk involved); after another restart the plugin bound port 9123.
  - Logs: only the usual DMX/MIDI/XSETTINGS/XRandR/thread-priority lines plus the deliberate broken-file error.
  - Not checked live: Quit, Restart and the window-close guard (blenv rule: never click Quit/Restart; covered
    by the unit tests above and the unchanged desk dialog code), and a remote client's commands in the running
    app (no in-tree remote client; covered by the harness tests).
- Observed, pre-existing: after restarting with an extra plugin, the external screen came back at another
  window position and its dock layout gained the new plugin's tab, so the desk's section changed and the show
  became unsaved a few seconds after startup (the old hash check did the same). Scene changesets are a
  `HashSet`, so their order in the JSON changes on every load-and-save (no effect on dirty detection within a
  run).

### For 053, 047, 054
- 053: `persistence.rs` and `config::{parse_showfile, install_showfile, ShowfileUiState}` are engine code
  (`ShowfileUiState` could become opaque JSON there). The startup order in `main.rs` is now config → audio
  device → bus → `AppState` → desk client → **showfile (persistence service) → spawn persistence** → supervisor
  → plugin watcher. A headless binary: `PersistenceService::new` + `open_startup` + `spawn`, then wait for
  `app.persistence.exit_request()` (or a signal → send `Quit { save: true }` / call `PersistenceThread::stop`),
  `stop()`, `hub.shutdown()`, exit with `ExitRequest::code()`.
- 047/054: every new command and message is plain data except `ShowfileStatus.saved_at` (`SystemTime`,
  serialize as UNIX time). A remote frontend reads `ShowfileStatus`/`ShowLoaded` from the replay on connect,
  resets its id-holding state on a new generation, and must not send `SetFrontendSection` (it is the desk's).
  Paths in `SaveShowfileAs`/`LoadShowfile` are engine-host paths.
- `SaveStatus::Saving` is only ever published for the instant of a write (saves run synchronously on the
  service thread).
