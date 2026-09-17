# Repository Guidelines

## Workspace & Module Organization
Blaulicht is a Cargo workspace rooted two levels up (`../../`). Crates:
- `crates/core` (this crate, `blaulicht-core`): the engine, egui app, DMX/Art-Net output, and wasmtime plugin host.
- `crates/audio_engine`: audio capture and analysis (`stream_in` for real input, `noise` for mocks).
- `crates/shared`: wire types shared with plugins — `abi.rs` (ABI version), events, fixtures, scenes, palettes, UDP/serial payloads.
- `crates/plugin_framework`: the plugin-side SDK (artnet, midi, serial, udp, ui, state helpers).
- `crates/plugins/*`: WASM plugins, each a separate crate excluded from the workspace (e.g. `inspector`, `console`, `ddj_400`, `led_tubes`, `videowall`, `player`). Built bundles land in `bundle_debug/` and `bundle_release/`.

Inside `crates/core/src/`: `app/` drives UI state and pages, `dmx/` powers fixture output, `event/` schedules cues, `mainloop/` stitches the runtime together, `plugin/` hosts wasmtime plugins and the tick loop, `audio/` bridges the audio engine, `routes/` holds the (mostly disabled) HTTP routes, `stage.rs`/`stage_assets.rs` model the stage, `ui_ops.rs` defines `WasmUiOp`, and `cli.rs`/`config.rs`/`command.rs`/`msg.rs` cover CLI args, config, and message plumbing. Shared state and helpers sit in `state.rs`, `util.rs`, and `utils.rs`, with defaults in `config.toml`. The Svelte dashboard is under `web/` (assets in `web/static`). Skim `overview.puml` or `overview.png` for the system map.

Top level: `scripts/` (`blctl.py`, `loopback.py`, `udp_midi_sender.py`), `update.sh` (scp + remote update on the `bl` host), `screen_driver.sh` (xrandr setup for the 800x480 panel), `flake.nix` (toolchain env).

## Build, Test, and Development Commands
Run cargo through the flake environment: `direnv exec . cargo run -p blaulicht-core` from the repo root. Plain `cargo run` outside the flake fails with a glibc mismatch.

Core `Makefile` (sets `RUSTFLAGS=-C target-cpu=native -Awarnings`; `RARGS=...` passes runtime args):
- `make all-check` / `make all-run` / `make prod-run` / `make prod-build`: check, debug run, release run, release build with `wayland audio wasmtime x11`.
- `make mock-run` / `make mock-check`: `audio-mock` without wasmtime; `make audio-mock-run`: real audio, no wasmtime; `make wasm-mock-run`: `audio-mock` plus wasmtime.
- `make dhat-all-run`, `dhat-all-run-prod`, `dhat-mock-run`, `dhat-wasm-run`: same profiles with the `dhat` heap profiler.
- `cargo test --all-features`: Rust unit tests; add `-- --nocapture` when debugging runtime logs.
- `cargo clippy --all-targets --all-features` then `cargo fmt`: required lint + format gate.
- `pnpm install && pnpm dev --dir web`: start the Svelte UI; `pnpm build --dir web` for production.

Plugins: `cd crates/plugins && make build_noopt` rebuilds every plugin into `bundle_debug/<name>.wasm`; `build_plugins.sh` produces `bundle_release/`. Top-level `Makefile` handles packaging (`release`, `build-archives`, `gh-release`, `version`, `clean`).

## Coding Style & Naming Conventions
Rust code uses four-space indentation, snake_case modules/functions, and PascalCase types/enums. Keep imports sorted std/third-party/internal and gate feature code with `#[cfg(feature = "...")]`. Front-end files follow Prettier + ESLint (`pnpm format`, `pnpm lint`); components stay in PascalCase with co-located styles.

## Testing Guidelines
Co-locate `#[cfg(test)]` modules beside the logic they cover; there is no `tests/` directory yet, create one only when a multi-module flow needs it. Mock audio hardware with `make mock-run` / `make mock-check` (`--no-default-features --features "wayland audio-mock"`). For live end-to-end checks against the running engine, use the inspector plugin (see below). Any change that touches `src/app/` or otherwise affects what is drawn must be verified visually by the agent itself, following the UI Verification section, before it is reported as done. Web changes must pass `pnpm check` and `pnpm lint`, and include before/after screenshots when visuals shift.

## Commit & Pull Request Guidelines
Commits follow conventional prefixes (`feat:`, `fix:`, `refactor:`, `chore:`) and should stay atomic. Reference issues inline when relevant (`feat: add midi panic button (#342)`) and document config toggles in the body. Pull requests need a concise summary, verification steps, and linked issues; attach UI media or DMX capture notes when behavior changes. Confirm the Rust and web test commands above before requesting review.

## Configuration & Deployment Notes
Adjust defaults via `config.toml` and keep secrets out of version control. Release artifacts live in `target/` and `dist/`; do not commit them. Coordinate with maintainers before modifying `update.sh` (deploys to the production `bl` host) or `screen_driver.sh`.

## Plugin ABI
`PLUGIN_ABI_VERSION` lives in `crates/shared/src/abi.rs` (currently 5). Any change to serialized shared types (`TickInput`, `UdpReceived`, `ControlEvent`, host function signatures, …) must bump it and rebuild all plugins with `cd crates/plugins && make build_noopt`. The engine rejects a `.wasm` with a mismatched ABI at load, so stale bundles fail loudly rather than silently misbehave.

## Live Inspection (inspector + blctl)
The `inspector` plugin (`crates/plugins/inspector`, enabled in `config.toml`, hot-reloaded by the plugin watcher) listens on `127.0.0.1:9099`, loopback only. Drive it with:
`python3 scripts/blctl.py <ping|audio|beat|state [path]|event '<ControlEvent JSON>'|override U C V|clear U C|tap>`
Full `state` dumps fall back to a Debug string once tuple-keyed maps (overrides, fixture states) are non-empty; prefer `beat`, `audio`, or path-narrowed `state` queries for structured output.

## UI Verification (screenshots)
The agent verifies UI changes by running the app, navigating it, and looking at screenshots itself. No pointer or keyboard injection tool is installed, so all navigation and state changes go through inspector events.
1. **Check the session is unlocked**: `pgrep -x swaylock` must print nothing. If it does, screenshots only show the lock screen; stop and ask the user to unlock instead of reporting a visual result.
2. **Launch from the repo root** in the background so `./config.toml` (which enables the inspector plugin) is picked up: `direnv exec . cargo run -p blaulicht-core --no-default-features --features "wayland audio-mock wasmtime"` (drop `--no-default-features …` for real audio). The `make -C crates/core …` targets run inside `crates/core` and load that crate's own `config.toml`, which has no inspector; pass `-c ../../config.toml` in `RARGS` if you use them. Wait until `python3 scripts/blctl.py ping` answers (it also reports the ABI version).
3. **Locate the window**: `hyprctl clients -j` and pick the client whose `title` is `blaulicht` (its `class` is empty, so do not match on class); read `at` (x, y) and `size` (w, h). The default window is 800x480 and undecorated; `--desktop-mode` / `--window-decorations` make it resizable. If it is on another workspace, `hyprctl dispatch focuswindow title:blaulicht`.
4. **Navigate**: `python3 scripts/blctl.py event '{"MainUi":{"NavigatePage":"System"}}'`. Page names come from `AppPage` in `crates/shared/src/page/mod.rs`: `Logs`, `System`, `Audio`, `FixturesSetup`, `View`, `ViewPerformance`, `FixturesPerformance`, `Animations`, `Palettes`, `SceneGraph`, `Visualizer`. Other state (overrides, `SetPluginUIOpen`, any `ControlEvent`) is injected the same way.
5. **Capture**: wait ~300 ms for egui to repaint, then `grim -g "<x>,<y> <w>x<h>" <scratchpad>/<page>.png` and open the PNG with the Read tool. Take a before and after shot for visual changes. Keep shots in the session scratchpad; never commit them.
6. **Limits**: widgets cannot be clicked. If a check needs a real click, say so in the report and ask the user to do it. Headless fallback: `Xvfb :9 &` then run with `DISPLAY=:9` and the `x11` feature, and `import -display :9 -window root shot.png`.
7. **Report**: state which pages were captured and what was checked. If a screenshot could not be taken or the session was locked, say so explicitly instead of claiming visual verification. Stop the app afterwards (`pkill -x blaulicht-core`).

## Plugin UI Rendering Notes
- Plugins queue egui instructions by calling host-provided `ui_*` exports; `ui_begin` swaps the double-buffer so the frontend reads stable frames.
- Host stores queued `WasmUiOp` lists in `AppState.plugin_ui_ops`; the `render_plugin_ops` walker in `src/app/plugin_ui.rs` materialises widgets and canvases.
- User actions emit `ControlEvent::PluginUi` through the event bus, letting the originating plugin observe button presses, text edits, and canvas gestures in its next `TickInput`.
