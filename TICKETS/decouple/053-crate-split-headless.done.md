# 053 — Engine crate, egui crate and a headless binary

| | |
|---|---|
| Epic | E10 Decouple |
| Depends on | 039, 046, 050, 043, 044, 051, 045, 052 |
| ABI / showfile | none |

**Touches:** `Cargo.toml` (workspace), `crates/core/**`, new `crates/engine`, new `crates/ui_egui`, `Makefile`, `crates/core/Makefile`, `update_version.py`, `tools/blenv/**`, `.claude/skills/blaulicht-env/SKILL.md`, `crates/core/AGENTS.md`, `TICKETS/**` (path sweep)

## Problem
Engine and UI are one crate, and `main.rs` wires the engine threads and `eframe::run_native` together.

## Scope
- Split into crates:
  - `blaulicht-engine`: dmx, mainloop, plugin host, audio bridge, event bus and hub, state, config,
    persistence, `stage.rs`, `EngineClient` + `LocalClient`. No egui, eframe, epaint, winit, glow, three-d or
    image dependency.
  - `blaulicht-ui-egui`: `app/`, `stage_assets.rs`, built on `EngineClient` only. Gate the visualizer, the
    file dialog and plugin pop-out windows behind features, so a later wasm build (048) can turn them off.
  - Binaries: `blaulicht` (engine + native UI in one process, the same behaviour as today; keep the binary name
    `blaulicht-core` if renaming would break `update.sh` or blenv; decide and write it in Notes) and
    `blaulicht-headless` (engine only).
- `Engine::start(cfg) -> EngineHandle` wraps today's startup order: config → audio device → event bus →
  supervisor → plugin watcher → showfile load (`main.rs:23–200`). Both binaries call it.
- Split CLI flags: UI flags (`desktop_mode`, `window_decorations`, `external_screens`) live in the UI binary.
- Headless: handle SIGINT and SIGTERM through `nix` (already in the tree) or `signal-hook`, run the 052 shutdown
  hook and exit.
- Update every build reference: `Makefile:4`, `:21`, `:32`, `:67`; `crates/core/Makefile`;
  `update_version.py:8`; `tools/blenv/{Dockerfile,README.md,blenv.py,mcp_server.py,container/build.sh,container/run-app.sh}`;
  `.claude/skills/blaulicht-env/SKILL.md`; `crates/core/AGENTS.md`.
- Sweep the Touches and Key code paths in every planned ticket under `TICKETS/`.

## Acceptance criteria
- [x] `cargo tree -p blaulicht-engine | grep -E 'egui|eframe|epaint|winit|glow|three-d'` finds nothing.
- [x] `blaulicht` behaves as before (blenv smoke test). (The desk binary keeps the name `blaulicht-core`, see Notes.)
- [x] `blaulicht-headless` with a config that loads the inspector plugin starts, loads the last showfile, runs
      plugins, answers `blctl.py state` and keeps its memory flat over 10 minutes.

## Verification
- `cargo clippy --workspace --all-targets --all-features`, `cargo fmt`, `cargo test --workspace --all-features`.
- `scripts/blenv build` and `run` still work. Plugins: `cd crates/plugins && make build_noopt`, then grep for
  `^error`.

## Notes

Done in one part (no split).

### What changed
- **Three crates** (workspace members `crates/engine`, `crates/ui_egui`, `crates/core`):
  - `blaulicht-engine` (`crates/engine`): `audio/`, `client.rs`, `command.rs`, `config.rs`, `dmx/`, `event/`,
    `frontend_ui.rs`, `hub.rs`, `log.rs`, `mainloop/`, `msg.rs`, `persistence.rs`, `plugin/`, `routes/`,
    `stage/`, the engine half of `stage_assets.rs`, `state.rs`, `ui_ops.rs`, `util.rs`, `utils.rs`, plus the new
    `lifecycle.rs` and `cli.rs`, and `wasmtime-cache-config.toml`. Moved with `git mv` (history follows). Features
    `audio` (default), `wasmtime` (default), `audio-mock`, `dhat`.
  - `blaulicht-ui-egui` (`crates/ui_egui`): `app/`, the three-d half of `stage_assets.rs`, new `desk.rs`
    (`desk::run(client, UiArgs, config_dir)` = `eframe::run_native` around `BlaulichtApp`) and `cli.rs`
    (`UiArgs`). Features `visualizer`, `file-dialog`, `plugin-popout` (default on), `wayland` (default), `x11`.
  - `blaulicht-core` (`crates/core`): only the desk binary `src/main.rs` (`Engine::start` with a desk client →
    `desk::run` → `EngineHandle::shutdown`, exit 42 on Restart). Features `audio`, `audio-mock`, `wasmtime`,
    `wayland`, `x11`, `dhat` forward to the two crates, so every existing `cargo … -p blaulicht-core
    --features …` command, the `crates/core/Makefile` targets and blenv's build keep working unchanged.
- **`Engine::start(EngineOptions { config_file, attach_desk }) -> anyhow::Result<Option<EngineHandle>>`**
  (`lifecycle.rs`, re-exported at the crate root) is the old `main.rs` startup, same order: tracing + hub,
  config, audio device, bus, `AppState`, desk client (only if `attach_desk`), startup showfile + persistence
  thread, supervisor, plugin watcher. `Ok(None)` = the config didn't exist and a default one was written (as
  before, nothing starts). `EngineHandle`: `take_desk()`, `connect_client()` (a `LocalClient` in a new
  session), `config_file()`, `exit_request()`, `shutdown(self) -> Option<ExitRequest>`
  (`PersistenceThread::stop` + `hub.shutdown()`). It does not hand out `AppState`.
- **CLI split:** `blaulicht_engine::cli::EngineArgs { config_file }` (`-c`, default `./config.toml`) and
  `blaulicht_ui_egui::cli::UiArgs { window_decorations, desktop_mode, external_screens, showfile_home }`, both
  `#[derive(clap::Args)]` and flattened into each binary's parser. Flags and short names are unchanged.
- **`blaulicht-headless`** (`crates/engine/src/bin/blaulicht-headless.rs`, so it builds without any UI crate):
  `Engine::start` without a desk, then polls every 100 ms for `exit_request()` (a client's Quit/Restart) or a
  signal. SIGINT/SIGTERM are blocked before the engine starts and received with `sigwait` on a `signals`
  thread (`nix` 0.26 `SigSet`, already in the tree via `serialport`; no signal handler; `std::process::Command`
  children get a cleared mask). The first signal sends `HostCommand::Quit { save: true }` through its own
  client and waits for the shutdown hook (max 15 s); a refusal ("Save failed: …"), the timeout or another
  signal ≥ 1 s later stops anyway with exit code 1. A repeat within 1 s is ignored (`timeout -s INT` and some
  service managers deliver the signal twice at once; that first made it skip the save). Exit code otherwise
  `ExitRequest::code()` (0 Quit, 42 Restart). On non-Unix it only polls `exit_request()`.
- **`stage_assets.rs` split:** engine: `MAX_GLB_BYTES`, `check_glb_name`, `check_glb_bytes` (now `pub`),
  `install_glb`, `resolve_asset`, `copy_assets_for_save_as`; egui crate: `StageModelStats`,
  `PreparedStageModel`, `load_prepared_model`, texture prep, `read_glb_for_import` (uses the engine checks).
  Tests split accordingly (engine: path traversal, bad contents, install; UI: bad contents before parsing, a
  read keeps name and bytes, bundled models, texture limits).
- **Visibility:** the only `pub(crate)` items the app used were `plugin::tick::stable_animation_{instance,
  template}_id`; they are `pub` and re-exported as `blaulicht_engine::plugin::stable_animation_*`. Nothing else
  had to become public: since 052 the app only reaches the engine through `LocalClient` (its `Arc<AppState>`
  stays private; the fence holds across the crate boundary). Only `programmer.rs`'s existing
  `undo_all_empties_the_focused_scene_and_keeps_the_others` test builds a `DmxEngine`/`AppState` directly, with
  items that were already `pub`.
- **`ScreenId`** moved from the engine's `state.rs` to `app/view_state.rs` (039 left it for 053; only the desk
  uses it).
- **Feature gates in the egui crate** (for 048): `visualizer` (`#[cfg]`: the visualizer module, `stage_assets`,
  optional `three-d`, `three-d-asset`, `egui_glow`, `image`, `bytemuck`; without it `pages/visualizer_disabled.rs`
  shows a notice on the page and Setup hides "Show in Visualizer"), `file-dialog` (`#[cfg]`: showfile
  Load / Save As moved verbatim into `pages/system/file_dialog.rs` with `FileDialogOpenOrigin`, optional
  `egui-file-dialog` and `sysinfo`; the visualizer's Import GLB button), `plugin-popout` (`cfg!`: no "Pop out"
  button and no native plugin viewport; a maximize request opens a normal window; egui viewports compile
  everywhere, so no optional dep). Checked: default, no features, and each feature alone (+`x11`) build with no
  new warnings.
- **`log::source_for_target`:** `blaulicht_engine::…` maps like `blaulicht_core::…` did (plugin/midi/udp/serial,
  dmx, audio, config, routes, else Engine), `blaulicht_engine::lifecycle` → Init (the startup lines that
  `main.rs` used to log), `blaulicht_ui_egui` → UI, the binary crates `blaulicht_core` / `blaulicht_headless`
  → Init. The test `our_module_paths_map_to_subsystems` covers the new names. Checked live: "Starting
  BLAULICHT" shows as Init on the Logs page, DMX/Plugin/UDP/Engine/MIDI as before.
- **Dependencies:** the engine has no egui, eframe, epaint, winit, glow, three-d or direct `image` dependency.
  `image` still comes in through `blaulicht-audio-engine` (an unconditional default-feature dependency there,
  "TODO: remove all of this"); the bg_worker BMP dump now writes through a new
  `SpectrogramImage::save_bmp` in audio_engine instead of using `image` itself. `winit` was dropped (unused
  since 046). The engine now always enables audio_engine's `noise` (the dummy inputs; every working build had
  it through `audio` or `audio-mock`), so `blaulicht-engine` builds without features; `audio-mock` is an empty
  marker feature kept for the build commands.
- **Tooling:** blenv `build headless` (the engine binary with `BLENV_FEATURES` minus `x11`/`wayland`,
  overridable with `BLENV_HEADLESS_FEATURES`) and `run --headless` (same config/showfile handling, no window;
  `blctl`, `logs`, `status`, `stop` work; also `blenv_run(headless=True)` / `blenv_build("headless")` on the MCP
  server). Only `tools/blenv/{blenv.py,mcp_server.py,README.md,container/build.sh,container/run-app.sh}`
  changed; the Dockerfile (whose hash is the image tag) is untouched, so no image rebuild. `crates/core/Makefile`
  has `headless-run`, `headless-mock-run`, `headless-check`; `update_version.py` bumps the two new
  `Cargo.toml`s; the top-level `Makefile` only got a comment (it still builds and copies the desk binary
  `blaulicht-core`). `AGENTS.md`, the blaulicht-env skill and the blenv README describe the layout.
- **Ticket sweep:** `Touches` / `Key code` paths in every planned ticket and in `TICKETS/README.md` now point at
  `crates/engine/src/…` / `crates/ui_egui/src/…` (`crates/core/src/ui.rs`, a stale name, became
  `crates/ui_egui/src/app/ui.rs`; 026's `stage_assets.rs` the egui half). Done tickets are untouched. Two code
  comments that named `crates/core/src/dmx/mod.rs` (`shared/src/folder.rs`, `visualizer/src/core/profiles.ts`)
  were updated too.
- **TODO Inbox:** the always-black bg_worker spectrogram dump (046: 480x100 leaves 75 rows for 128 bins, needs
  ≥ 153 px), `-e` external screens never opening in `--desktop` mode (pre-existing: only the kiosk branch drives
  them), and headless not being in the release archives.

### Decisions and deviations
- **The desk binary keeps the name `blaulicht-core`** (package and binary, in `crates/core`), not `blaulicht`:
  renaming would break blenv's `run-app.sh` (binary path, `pkill -x blaulicht-core`), the release `Makefile`
  copies, `cargo … -p blaulicht-core` in AGENTS.md/the skills, and the `pgrep`/`pkill -x blaulicht-core`
  convention that agents in other worktrees follow. The release archive already renames it to `blaulicht`.
  `update.sh` only uploads the archive, so it is unaffected either way.
- **Binary placement:** the headless binary lives in the engine crate (so `cargo build -p blaulicht-engine --bin
  blaulicht-headless` needs no UI dependency); the desk binary in its own small package instead of the egui crate,
  for the name above.
- **`Engine::start` returns `Option`** for the "default config created" case instead of an error, as `main` did.
- **The desk client is connected inside `Engine::start`** (`attach_desk`), because it must exist before the
  showfile load and the supervisor (startup `ShowLoaded`, owned external screens from plugin init); the
  ticket's "config → audio device → event bus → supervisor → plugin watcher → showfile load" order was replaced
  by 052's (showfile before supervisor).
- **No headless desk:** desk requests are dropped as 039/049 designed (no desk subscription); the headless run
  showed no related warnings, and plugins that save global state write the config without a desk (052).
- **`ShowfileUiState` stays a typed struct in the engine's `config.rs`** (052 suggested it could become opaque
  JSON when the crates split): the engine stores, compares and saves it; turning it into a `serde_json::Value`
  would reorder its keys in the file. Left for 047/054 if a remote frontend needs its own section.
- **Release archives** still contain only the desk binary (cross build image not run here); noted in the Inbox.
- **No unit test for `Engine::start`:** it installs the global tracing subscriber, opens real devices and
  starts threads that never end (bus, watcher). It was verified live (both binaries, below); every piece it
  wires is covered by the existing engine tests.

### Verification
- `cargo tree -p blaulicht-engine | grep -E 'egui|eframe|epaint|winit|glow|three-d'`: empty (also with
  `-e normal --all-features`).
- `cargo clippy --workspace --all-targets --all-features`: blaulicht-engine lib 39 / lib test 42,
  blaulicht-ui-egui lib 89 / lib test 104, both binaries 0. That is the old blaulicht-core lib 128 / lib test 146
  split between the crates; the (file, warning) list is identical to the baseline. `cargo build` (blenv features
  or defaults): engine 17 + egui 64 = 81 warnings, every one present in the baseline list.
- `cargo test --workspace --all-features`: all pass; engine 251 + egui 101 = 352 (was 350: the five
  `stage_assets` tests became three engine and four egui tests).
- `cargo fmt`: clean; the usual unrelated hunks (audio_features, `pages/audio.rs`, `visualizer/ui.rs`,
  `app/ui.rs`) reverted.
- Feature builds: `blaulicht-core --no-default-features --features "x11 audio-mock wasmtime"` and `"wayland
  audio-mock"`; `blaulicht-engine` with no features, `audio-mock`, `audio wasmtime`; `blaulicht-ui-egui` default,
  `x11` only, `x11` + each of `visualizer`, `file-dialog`, `plugin-popout`.
- **Headless** (blenv container, `/state/headless/config.toml` from the blenv template = inspector, julia, audio
  reactor, chaos, sample egui; a copy of `Kuze_Theater.json` as `last_open_showfile`):
  - Startup: "Loaded showfile …kuze053h.json", all five plugins loaded, inspector ping (ABI 18), `blctl state`
    returns the show (Frontlicht, Linse LL, …), `blctl beat` answers.
  - `SelectGroup(6)` + `SetAlpha(201)` then SIGTERM: "Received SIGTERM; saving", "Saved showfile", "Shutdown hook
    done (Quit)", "Engine stopped"; the file has the edit.
  - `timeout --preserve-status -s INT 25 blaulicht-headless …`: exit code 0 (before the repeat guard it was 1,
    see above).
  - Memory over 10 minutes (sampled from `/proc/<pid>/status` every 60 s, from the moment the last plugin was
    loaded; one edit per minute via blctl, 10 autosaves, one `blctl state` dump per minute; debug build):

    | t (min) | 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 |
    |---|---|---|---|---|---|---|---|---|---|---|---|
    | RssAnon (MB) | 196.0 | 197.5 | 198.6 | 199.7 | 199.7 | 199.7 | 199.7 | 195.3 | 196.3 | 196.3 | 196.3 |
    | VmRSS (MB) | 225.5 | 217.7 | 218.8 | 219.8 | 208.4 | 208.4 | 208.4 | 203.5 | 205.6 | 205.0 | 205.0 |

    12 threads throughout, VmSize constant after t=2. VmRSS also moves with file-backed pages (RssFile 29 → 8 MB);
    a first attempt that sampled only `ps rss` saw 225 → 24 MB at t=1 for the same reason, so anonymous RSS is
    the figure to compare. Flat: +0.3 MB after 10 minutes, peak +3.7 MB.
  - `scripts/blenv build headless` + `scripts/blenv run --headless --showfile …` + `status` + `logs` + `stop`
    (graceful SIGTERM) work.
- **Desk** (blenv, kiosk 800x480, copy of `Kuze_Theater.json`; screenshots read):
  - Startup and Logs page: sources as before ("Starting BLAULICHT 0.30.0" = Init, persistence = Engine).
  - Programmer: clicked "Par Truss" (inspector: `group_ids: {6}`), dragged Alpha 167, Hue 165, Saturation 206,
    Value 216: cyan swatch, unsaved dot.
  - Setup › Fixtures: Par Truss › Mid 1 editor; "Show in Visualizer" opens the Visualizer's top view with Mid 1
    selected; View › 3D renders the room and the fixture.
  - System › General: Load / Save to Showfile buttons present; after a `SetAlpha(150)`, Save Showfile → "Saved
    Showfile" popup, "(saved just now)", log line.
  - Plugin window #4: "Click Me" (Clicks: 1), Pop out opens the 800x600 viewport, Pop in closes it.
  - `--desktop` (1500x950 dock layout) and `-e 1280,720` (kiosk: window `bl_ext_0`) parse and work.
  - Logs: only the usual DMX/MIDI/XSETTINGS/XRandR/thread-priority lines.
  - Not clicked (blenv rule): Quit, Restart, Shutdown.
- **Plugins:** no plugin crate depends on `blaulicht-core`. `scripts/blenv build plugins` (default five) builds;
  `cd crates/plugins && make build_noopt` (in the container): 21 crates, the only `^error` is
  `pioneer_pro_dj_link`'s absolute `prolink` path (pre-existing, already in the Inbox). No ABI change.

### For 047, 048, 054
- 047: a server thread can be given an `EngineHandle`-derived client per connection
  (`EngineHandle::connect_client()` today returns a `LocalClient` in a new session; a `RemoteClient` server will
  need the hub and bus handles, so add an accessor on `EngineHandle` rather than exposing `AppState`). The
  headless binary is the natural host for it. `routes/` (disabled) is in the engine crate now.
- 048: `blaulicht-ui-egui` builds with `--no-default-features --features x11` (no visualizer, file dialog or
  pop-out). Still native-only: `desk::run` (`eframe::run_native`), `BlaulichtApp` takes a concrete
  `LocalClient` (generic or enum client still to decide, 042), external screens are native viewports (not
  gated), and the rest (`chrono`, the plugin UI renderer, `egui_dock`, `egui-snarl`) is unchecked on wasm.
- Release packaging of `blaulicht-headless` is open (Inbox).
