# 054 — Web frontend

| | |
|---|---|
| Epic | E10 Decouple |
| Depends on | 048 (and the maintainer's decision recorded there) |
| ABI / showfile | none |

**Touches:** `crates/ui_egui` (wasm cfg gates, `web_time`, enum client), `crates/engine` (wasm-gated deps, `RemoteClient` browser transport, static file serving), a web entry point, blenv/tooling

## Problem
The goal of the epic: a web version running alongside the native UI on the device.

## Decision (2026-11-01)
The maintainer chose **slim A** from 048: egui compiled to wasm, served by the engine, no visualizer on the web yet. Estimate about 7 days (rows 1–12 in 048's Notes). Full A (visualizer, native-only gating) and Svelte were not chosen.

## Scope
- Build the option chosen in 048, served by the engine's HTTP server on the configured port, with at least
  Programmer (selection, attributes, palettes) working end to end.

## Out of scope
- Feature parity with the native UI.

## Acceptance criteria
- [x] A browser on another device and the native UI control the same rig at the same time, each with its own
      selection. (Verified with chromium in the blenv container against the desk on the same engine; "another
      device" here means another client over the network protocol on loopback, see Notes.)

## Verification
- Screenshots of both frontends side by side after each changed something.

## Notes

Done in one part (no split, no 055). Slim A as decided: `blaulicht-ui-egui` compiled to wasm32, served by the
engine on its port, connected back as a `RemoteClient` in a session of its own. No visualizer on the web.

### How to build and open it
- `scripts/blenv build web` (container) or `crates/web/build.sh [--lint] [--dev] [OUT_DIR]` / `make build-web`
  (host with the `wasm32-unknown-unknown` target). Output: `$CARGO_TARGET_DIR/web` (`target/web`).
- wasm-bindgen-cli must equal the `wasm-bindgen` version in `Cargo.lock` (0.2.120). The script checks it and
  prints `cargo install wasm-bindgen-cli --version <v> --locked`; blenv installs the prebuilt release binary into
  the cargo volume (`/cargo/bin`) on first use (falls back to `cargo install`). The Dockerfile is unchanged.
- Start the desk or `blaulicht-headless` as usual and open `http://<device>:<port>/` (1234 by default). The engine
  logs `Web UI on http://0.0.0.0:1234/ (files from …)` or, without a bundle, where it looked; then `/` says how
  to build it.
- Where the bundle lives: `config.toml`'s new optional `web_ui_dir` (relative to the config file, not written
  back when unset), else `web/` next to the running binary or one folder up. So `target/web` serves binaries in
  `target/debug` and `target/release`, and a release archive can put `web/` next to the binaries.

### Bundle size (release, `web` profile: opt-level s, LTO, codegen-units 1, stripped)
| file | raw | gzip -9 (what browsers get) |
|---|---|---|
| `blaulicht_web_bg.wasm` | 6.61 MB | 2.69 MB |
| `blaulicht_web.js` | 76 KB | 11.5 KB |
| `index.html` | 2.2 KB | 0.9 KB |

`wasm-opt -Oz` (binaryen 120, tried in the container) gave 6.24 MB raw but the same 2.69 MB gzip, so the build
doesn't use it. Release build of the crate: about 60–70 s in blenv.

### What changed
- **Engine wasm gating** (`crates/engine/Cargo.toml`, `plugin/midi.rs`, `util.rs`): `midir` and
  `thread-priority` are native-only target deps; wasm32 gets an inert `MidiManager` and a no-op priority bump.
- **getrandom**: no rustflag or cfg was needed after all. getrandom 0.3.4 selects its `wasm_js` backend on
  wasm32-unknown-unknown from the `wasm_js` feature alone (`backends.rs`; the cfg is only an alternative), and
  0.2 uses `js`. Both features are enabled in `crates/web/Cargo.toml` only (wasm32 target deps), so only the
  web build has them. Plugins build in their own workspaces and never see them: `scripts/blenv build plugins`
  (default five) has no `^error`.
- **web_time** (engine and egui crate, 36 files): `Instant` / `SystemTime` / `UNIX_EPOCH` come from `web_time`, a
  re-export of std on native. The supervisor waits with `recv_timeout` instead of `recv_deadline` (std
  `Instant`), log rows keep a chrono `DateTime<Local>`, `server/http.rs` uses std's epoch for file mtimes. The
  native-only headless binary and `remote_probe` keep std and allow the lints.
  - **Lint:** `crates/{engine,ui_egui}/clippy.toml` ban std's `Instant`/`SystemTime` and their `now`/`elapsed`.
    Because `web_time` *is* std on native, clippy can only tell them apart in a wasm32 build; the crate roots
    allow the two lints on native (no new native warnings). `crates/web/build.sh --lint` (run by `blenv build
    web` and `make build-web`) fails on any hit. Checked: a planted `std::time::Instant::now()` was reported in
    the wasm32 run and not natively.
- **`RemoteClient` browser transport** (`client/remote.rs` + `remote/native.rs` + `remote/web.rs`): the shared
  state stays; the TCP/tungstenite thread moved to `native.rs`; `web.rs` uses `web_sys::WebSocket` (binary,
  bincode). `connect_web(url, streams, waker)` returns at once, the handshake runs in the callbacks, commands
  sent before the socket opened follow `Hello`, the waker requests an egui repaint. New on both: `status()`
  (Connecting / Connected / Disconnected(reason)), `reconnect()` (new connection and session, same streams),
  `streams()`, `has_snapshot()`, round trips in `stats()`. `engine_version()` is now `Option<&str>` (unknown
  before `Welcome` in a browser).
- **`AppClient`** (`crates/ui_egui/src/client.rs`): `enum { Local(LocalClient) /* native only */, Remote }`
  delegating every `EngineClient` call. The desk pays one match per call, no lock, copy or allocation. Helpers
  that took `&LocalClient` take `&impl EngineClient`.
- **HTTP on the same port** (`server/http.rs`, `server/mod.rs`): the connection thread peeks at the request head
  (`TcpStream::peek` + httparse, already in the tree); an upgrade goes on to tungstenite unchanged, anything
  else is answered and closed. GET/HEAD, files from the bundle only (no `..`, `%`, `\`), precompressed `.gz`
  for gzip-capable browsers, ETag + `Cache-Control: no-cache`, `Connection: close`. The head is consumed and the
  socket lingers 200 ms after the answer, otherwise closing with unread input reset the connection (found by the
  tests).
- **Remote-aware UI** (`app/connection.rs`, `ui.rs`, `event.rs`, `pages/system/{misc,showfile}.rs`,
  `visualizer_disabled.rs`): see the page table. Plus a connection notice (connecting / lost with the reason,
  automatic reconnect every 3 s, Reconnect and Reload Page buttons; reply closures of the dead connection run
  before seqs restart), page-dependent streams (`remote_streams`: no bus, spectrogram only on Audio), no
  `SetFrontendSection` from remote clients, `Exiting` doesn't close a remote UI, and in a browser the device pixel
  ratio stays (no forced 1.0), the 800x480 kiosk layout is zoomed to the window (0.6–1.5×), dark theme at start.
- **Web crate** (`crates/web`): `WebRunner` on `#blaulicht_canvas`, websocket URL from `window.location`
  (ws/wss, same host:port), eframe's `WebLogger` for `log` and tracing (tracing's `log` feature) to the console,
  a panic hook chained after eframe's that shows the message in `#crash`. `index.html` blocks browser zoom and
  touch gestures and is home-screen capable. New workspace member (empty on native) and `[profile.web]`.
- **Tooling/docs**: blenv `build web` (CLI and MCP), `make build-web`, `crates/core/AGENTS.md`, blenv README
  and skill (installing chromium in a running container, telling the `Blaulicht` browser window from the desk's
  `blaulicht`).

### Pages on the web
| Page | On the web |
|---|---|
| Programmer (selection, attributes, palettes, scene changes, masters) | works |
| Logs, Audio (spectrogram subscribed while shown), Setup (Fixtures, Groups, Scenes, Animations incl. the template editor, Palettes), View, Performance, Scene graph | work (toured; no crash) |
| System: General | works without Quit / Restart / Shutdown / Add Screen (hidden); Close / Save Showfile, theme, Debug, Reload, health; shows "Connected as remote N (session N): round trip …" |
| System: DMX, Art-Net, Plugins (incl. plugin UI windows, no pop-out), MIDI, Serial | work (data from the Health/Status/PluginUi streams) |
| System: Screens | notice: external screens are the desk's windows |
| Showfile Load / Save As | hidden (file dialogs are off on the web, and hidden for any remote client: paths are the engine host's) |
| Visualizer | notice ("The web UI has no visualizer yet") |

### Visual checks (screenshots read; desk via blenv/xdotool, browser = chromium 156 in the container on `:9`)
Chromium was installed in the running container only (`apt-get install chromium`), started in app mode with
SwiftShader WebGL. blenv's `--window` match ignores case and both windows are named Blaulicht/blaulicht, so I
drove both by exact title with xdotool (the first two "desk" screenshots were the browser; noticed and redone).
- Kuze copy, desk on the Programmer, browser loads the UI (dark, zoomed to 1000x500): its Logs page shows the
  replay and "Remote client remote 0 connected … (bincode, session 1)".
- Browser selects Par Truss (fixtures Mid 1/2, Back 1/2), Alpha 189: browser shows it with the unsaved dot; the
  desk still shows no selection and Alpha 0; engine state: scene 0's four Par Truss fixtures have alpha 189,
  the desk session's selection and control buffer are empty.
- Desk selects Spitzlicht, removes one scene change (Pan on Linse LL), drags Alpha to 182: both show "All (78)"
  with the Pan row gone and the unsaved dot (shared show); the desk shows Spitzlicht/182, the browser still Par
  Truss/189 (separate sessions and control buffers).
- Browser sets Hue 195 / Saturation 220 / Value 230 (cyan swatch), creates a color palette "Web Red" in Setup ›
  Palettes (typed name), assigns it from the Programmer's palette dialog ("Active: Web Red", color faders greyed,
  engine state: Par Truss colors are `PalettePointer { palette_id: 0 }`); the desk's Setup › Palettes lists
  "Web Red". The desk's Programmer shows "Palettes (0)" for its own selection: the count is the palettes active
  in the selection, Web Red is on Par Truss only.
- Final run on the committed bundle: browser Par Truss Alpha 189 / Hue 195, desk Frontlicht Alpha 140, the
  browser unchanged after the desk's edit.
- Engine stopped (`blenv stop`, SIGTERM): the browser shows "Connection to the engine lost" with the reason and
  the countdown; after `blenv run` it reconnected on its own (new session, empty selection). Same after
  switching the engine to OUTDOOR and to `blaulicht-headless` (the browser worked against the headless engine:
  Par Truss, Alpha 189).
- Resized the browser to 1280x800 (zoom 1.5, big touch targets) and 844x390 (zoom 0.81): the kiosk layout fits.
- Desk log: no warnings or refusals from the web client; browser console (chromium `--enable-logging`): only
  info lines, no panic.
- Not clicked anywhere: Quit, Restart, Shutdown.

### Responsiveness (measured in the browser, loopback in blenv, debug engine)
- Command round trip (send → result), from `RemoteStats`, shown on System › General: **Kuze 18 ms median,
  65 ms slowest (78 commands)**; **OUTDOOR 42 ms median, 100 ms slowest (98 commands)** while the engine's loop
  took 34 ms. So commands don't wait noticeably behind the 126 KB / 8.7 Hz snapshots; the engine tick dominates.
  The timers-out-of-the-snapshot change (048 row 15) therefore stays in the Inbox (entry updated: Wi-Fi bandwidth,
  ≈9 Mbit/s for OUTDOOR, is the real concern).
- Browser frame rate (Debug dialog, software WebGL): 48 fps on OUTDOOR's Programmer with S0's 100 fixture tiles
  (the native debug desk does about 10 fps there), 51 fps on Kuze, 40 fps on System.

### Decisions and deviations
- **Enum, not generics**, for the app's client (048's suggestion); `Local` doesn't exist on wasm32.
- **No getrandom cfg** (see above), contrary to 048's plan; features scoped to the web crate instead.
- **Quit / Restart / Shutdown hidden for remote UIs** (they would work for Quit/Restart, but a tablet shouldn't
  stop the rig; Shutdown runs a script on the *frontend's* machine). Add Screen hidden too.
- **The bundle is not embedded in the engine binary**: served from a directory, so the engine works and says so
  without one, and a rebuilt bundle is picked up without restarting the engine.
- **Reconnect makes a new session** (the protocol has no session resume); the notice says so.
- `ReplyHandlers` gets reset on reconnect after dispatching the old connection's refusals (the new client's seqs
  start at 1 again).
- Remote clients' log page shows the replayed lines with their arrival time (pre-existing; Inbox).
- Not done here (Inbox): web bundle in the release archives (the cross image has no wasm toolchain), the
  visualizer on the web, remote showfile Load/Save As, a native `--connect` desk, idle repaint rate on tablets,
  a test on a real touch device, the `runtime` feature that would stop runtime engine code compiling for wasm.

### Verification
- `cargo test --workspace --all-features`: all pass; blaulicht-engine 251 → 264 (new: `a_remote_client_times_
  round_trips_and_reconnects_in_a_new_session`, `the_port_explains_a_missing_web_bundle`,
  `the_port_serves_the_web_bundle_and_remote_clients`; `waiting_commands_fail_…` also checks `status()`),
  ui-egui 101.
- `cargo clippy --workspace --all-targets --all-features`: engine 39 / 42, ui-egui 89 / 104, binaries and
  blaulicht-web 0 (baseline). wasm32 clippy (`build.sh --lint`): no disallowed std time.
- `cargo fmt`: touched files clean; the usual unrelated hunks (audio_features, `pages/audio.rs` line 622,
  `visualizer/ui.rs`, one `app/ui.rs` hunk) reverted.
- `cargo tree -p blaulicht-engine | grep -E 'egui|eframe|epaint|winit|glow|three-d'`: empty, also `-e normal
  --all-features`. `PLUGIN_ABI_VERSION` 18; `ControlEvent`, `TickInput` and `crates/shared` untouched;
  `PROTOCOL_VERSION` unchanged (no wire type changed).
- Intermediate commits checked to compile (`cargo check` of the desk and engine tests on archived trees).
- Disk: down to 4.9 GB free at one point; I deleted only stale per-hash build and incremental directories of
  `blaulicht-{engine,ui-egui,core,headless}` in this worktree's `/build/target` (older than the last
  `Cargo.toml` change), back to 20 GB. Other worktrees untouched.

### For later tickets
- A remote frontend is `BlaulichtApp::new(cc, RemoteClient::connect(..)?.into(), ..)`; `AppClient::is_remote()`
  is the switch for desk-only UI.
- `RemoteClient::connect_web` is the browser transport; anything that needs the browser (reload, location) is
  available in ui_egui through `eframe::web_sys` under `cfg(target_arch = "wasm32")`.
- Engine and egui code must keep using `web_time`; run `crates/web/build.sh --lint` after touching timing code.
