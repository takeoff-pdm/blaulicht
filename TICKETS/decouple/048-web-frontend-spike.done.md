# 048 — Web frontend spike: egui on wasm or Svelte

| | |
|---|---|
| Epic | E10 Decouple |
| Depends on | 047 |
| ABI / showfile | none |

**Touches:** `crates/ui_egui` (cfg gates only), a scratch wasm crate or build target

## Problem
The epic's goal includes a web version running next to the native UI. There are two options:
- **A. egui on wasm:** compile `blaulicht-ui-egui` with `RemoteClient`. Obstacles:
  - The visualizer shaders are `#version 330` (`visualizer/shaders/*.glsl`), and WebGL2 needs `300 es`.
  - The visualizer needs a 24-bit depth buffer and 4× multisampling (`main.rs:209–210`).
  - There are 31 `Instant::now` calls in `app/`, which panic on `wasm32-unknown-unknown` without `web-time`.
  - Threads are spawned at `ui.rs:354` and `:408`.
  - `std::fs` and `sysinfo` are used (`system/showfile.rs:13`), as are `cpal` (`app/event.rs:8`),
    `fdg_sim` (`scene_graph.rs`) and egui-file-dialog.
- **B. Svelte:** `crates/core/web` has 74 `.svelte` files and was last changed 2026-06-23. It speaks the dead
  actix JSON topic protocol (`web/src/lib/websocket.ts`), so it would effectively be a rewrite.

## Scope
- Try to build `blaulicht-ui-egui` for `wasm32-unknown-unknown` with the visualizer, file dialog and
  pop-outs turned off, and list what fails and roughly how much work each item is.
- Write a recommendation (A, B or a slim A without the visualizer) with an estimate into Notes. The maintainer
  decides before 054 starts.

## Acceptance criteria
- [x] Notes contain the build result, a blocker list with estimates and a recommendation.

## Notes

Done in one part (no split). Measured in the blenv container (rustc nightly 1.101, aarch64 Linux). Nothing was run
in a browser: the container has no browser and no `wasm-bindgen-cli`, so the runtime blockers below come from code
reading plus link-time evidence (see L3).

### Recommendation
**Slim A**: build 054 as egui on wasm, with only the Programmer exposed and the visualizer off. Estimate **about
7 days (6–8.5)**, broken down in the blocker table. Reasons:
- The whole UI already compiles for `wasm32-unknown-unknown` after four small gates (one is committed). The
  Programmer code (`programmer.rs`, `programmer_animations.rs`, `fixtures_shared.rs`, about 2900 lines plus
  widgets) and the engine's session-aware reads are reused as they are. B would rewrite all of it in TypeScript.
- With bincode, traffic is 1.1 MB/s (OUTDOOR, while animations run). B needs JSON, measured here at 10.8 MB/s, and
  needs engine-side deltas before it's usable on a tablet.
- A solo maintainer keeps one UI codebase in one language. Protocol and type changes are checked by the compiler
  end to end. B adds a node/pnpm toolchain and a second set of types to keep in sync with the Rust types.
- The costs of A are a 2.5 MB (gzip) download, canvas-rendered UI (weaker mobile text input, no browser zoom or
  reflow) and no phone-portrait layout. The native UI is already touch-first at 800x480, so a tablet in landscape
  gets the proven desk layout.

Full A (every page working remotely, plus the visualizer) adds about 7–12 days on top (see the end of the table).
**B** (Svelte, Programmer only) is about 10–15 days, plus the ongoing cost of a second UI.

### Build result (option A)
Command: `cargo build -p blaulicht-ui-egui --target wasm32-unknown-unknown --no-default-features` (the target was
already installed in the container). Results layer by layer:
- **L0 (HEAD):** fails in four third-party crates, all reached through `blaulicht-engine`:
  - `getrandom` 0.3.4 (rand 0.9, from the engine and `audio_engine`'s `noise` feature)
  - `getrandom` 0.2.16 (rand 0.8 via `tungstenite`)
  - `thread-priority` 1.2 (libc)
  - `midir` 0.10.3: its own Web MIDI backend doesn't compile against wasm-bindgen 0.2.120 (E0282 in
    `backend/webmidi/mod.rs:38`)

  With `--keep-going`, the other 187 crates compile for wasm. These include eframe (web backend), egui, egui_dock,
  egui-snarl, egui_code_editor, egui_extras (svg/resvg), egui_plot, egui-phosphor, fdg-sim (petgraph, glam,
  quad-rand), chrono, serialport, notify, spin_sleep, audio_thread_priority, tungstenite, tracing-subscriber,
  artnet_protocol, and image with audio_engine's full default features.
- **L1:** three changes in the engine make the **whole engine crate compile for wasm32**:
  - the getrandom backends: wasm-only target deps with `wasm_js` / `js`, plus `--cfg getrandom_backend="wasm_js"`
    in `CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUSTFLAGS`
  - `thread-priority` as a non-wasm target dep, plus a cfg in `util.rs`
  - `midir` as a non-wasm target dep, plus a 10-line `MidiManager` stub

  After that, `blaulicht-ui-egui` fails only in `desk.rs` (`eframe::run_native`, `NativeOptions`). So the wire
  types do **not** have to move into a light crate first, contrary to 047's note.
- **L2:** with `desk` gated (committed), `blaulicht-ui-egui` **builds**. A dev build after a change takes about
  4 s.
- **Other feature combinations:**
  - `file-dialog` compiles on wasm too (egui-file-dialog, sysinfo), but it's useless without a filesystem.
  - `plugin-popout` compiles (`cfg!` only).
  - `visualizer` has two compile errors: `three_d_asset::io::load_and_deserialize` is native-only
    (`stage_assets.rs:45`), and the web glow context isn't `Sync` for the paint callback
    (`visualizer/ui.rs:489`).
- **L3 (link):** a release cdylib that runs `eframe::WebRunner` with `BlaulichtApp` on an in-browser `LocalClient`
  came out at 4.05 MB. That is the size of an eframe hello world (4.10 MB), and it contained no Blaulicht strings.
  - The cause: `AppState::new` calls `Instant::now()`, which always panics on wasm32-unknown-unknown, so LLVM
    dropped the whole app as unreachable. This proves the `Instant` blocker.
  - I then swapped `std::time::{Instant, SystemTime}` for `web_time` with a script (36 files; the engine has
    85 `Instant::now` calls and 1 `SystemTime::now`, ui_egui has 20 and 5). Two more fixes were needed:
    crossbeam's `recv_deadline` takes a std `Instant` (`mainloop/supervisor.rs`), and chrono's
    `DateTime<Local>: From<SystemTime>` needs the std type (`components/log.rs`).
  - After that the app is linked: strings from Programmer, Audio, Scene Graph, Art-Net and the animation spec
    editor are present.
- **Ticket predictions that are stale:**
  - The two threads are now in the visualizer (`visualizer/state.rs:329`, `visualizer/ui.rs:1143`), behind the
    feature.
  - `cpal` in `app/event.rs` is gone (046).
  - `sysinfo` is only used by `file-dialog`.
  - `fdg_sim` compiles fine.

**Bundle size** (release: opt-level `s`, LTO, codegen-units 1, stripped, no debug info; raw cdylib before
wasm-bindgen; release LTO build 34–47 s):

| | raw | gzip -9 | xz -9 |
|---|---|---|---|
| eframe hello world (same deps and profile) | 4.10 MB | 1.61 MB | — |
| Blaulicht UI (all pages, no visualizer) | 7.34 MB | 2.74 MB | 2.02 MB |

- wasm-bindgen removes the 0.56 MB `__wasm_bindgen_unstable` section and the describe exports, so expect about
  6.5–6.8 MB raw and about 2.5 MB gzip in the shipped bundle.
- `wasm-opt -Oz` usually saves another 10–15% (it isn't installed).
- The default fonts and eframe make up about 3.5 MB of the raw size; Blaulicht adds about 3.2 MB.
- The probe linked `LocalClient`/`AppState`. A `RemoteClient` build swaps those for the protocol decoder, at a
  similar size. A Programmer-only entry point would drop the other pages (maybe −1 to −1.5 MB raw), but the page
  `match` references every page, so that needs its own render path.

### Blockers and estimates
| # | Blocker | Found by | Fix | Estimate |
|---|---|---|---|---|
| 1 | getrandom 0.2 and 0.3 refuse wasm32 | build | Wasm-only target deps with `js` / `wasm_js`. Set the cfg in a `.cargo/config.toml` inside the web crate's directory, **not** repo-wide: the plugins build for the same target and run in wasmtime without JS. | 1–2 h |
| 2 | `thread-priority` | build | Non-wasm target dep, cfg in `util.rs` | 0.5 h |
| 3 | `midir` (its Web MIDI backend is broken) | build | Non-wasm target dep, plus a stub or a gate | 1 h |
| 4 | `desk.rs` (`run_native`, `NativeOptions`) | build | cfg gate | **done** (committed) |
| 5 | std `Instant::now` / `SystemTime::now` panic (about 110 calls in 36 files) | link (LTO) | Swap to `web_time` (a no-op on native), apply the two fixes above, and add clippy `disallowed-types` so it stays that way | 0.5 d |
| 6 | No wasm transport: `RemoteClient` uses `TcpStream`, tungstenite, an I/O thread and `thread::sleep` (`wait_for_snapshot`). On wasm, `TcpStream::connect` returns `Unsupported`. | code | `RemoteState::apply` is already transport-independent. Add a `web_sys::WebSocket` transport (binary frames, arraybuffer; `onmessage` → `apply` + `request_repaint`; async `Hello`/`Welcome`; a connection-state flag), or use `ewebsock` on both sides. | 1–1.5 d |
| 7 | `BlaulichtApp` takes `LocalClient` (14 references in 7 files; 41 `impl BlaulichtApp` blocks; `EngineClient` isn't object-safe) | code | `enum AppClient { Local, Remote }` that delegates `EngineClient`. A generic `BlaulichtApp<C>` would touch all 41 blocks. | 0.5 d |
| 8 | Web entry point and toolchain | code | A cdylib with `WebRunner` and an `index.html`. `wasm-bindgen-cli` must match the `wasm-bindgen` crate exactly (0.2.120) and isn't in the blenv image; trunk is an alternative. Also blenv `build web` and the release `Makefile`. | 1 d |
| 9 | Serving: the engine port only speaks websocket (tungstenite accepts any path) | code | On the same port, serve a non-Upgrade `GET` as a static file (`index.html`, `.js`, `.wasm` as `application/wasm`) | 0.5–1 d |
| 10 | Desk-only paths and web specifics | code | An `is_remote` switch, a connection overlay, `tracing-wasm` plus a panic hook. The paths are listed below the table. | 1 d |
| 11 | Touch and tablet polish of the Programmer | code | DPR, canvas size, kiosk layout on tablets, mobile text input via egui's hidden input | 1–2 d |
| 12 | Browser verification for 054 (screenshots of both frontends) | env | Chromium in the blenv image, or a host browser with the port published | 0.5 d |
| | **Slim A total (1–12)** | | | **about 7 d (6–8.5)** |
| 13 | The engine compiles for wasm only "by accident": `std::thread`, `std::net`, `std::fs`, serialport and notify all compile but fail at runtime, so the compiler doesn't catch runtime code that a web frontend reaches | build | A `runtime` feature (or cfg) that gates lifecycle, mainloop, plugin runtime, server, persistence thread and audio; or the light model crate that 047 proposed | 1–2 d (hardening; not needed for 054) |
| 14 | Visualizer on the web | build, code | The two compile errors, GLSL `#version 330` → `300 es` (4 shader files), depth 24 and MSAA 4 via `WebOptions`, the two threads, and fetching stage models from the engine host (047) | 3–5 d |
| 15 | Show snapshot traffic, all caused by animation timers (see B) | measured | Keep `active_animations[*].fixture_timers` out of the snapshot and its digest, or put them in a low-rate stream of their own | 0.5–1 d (optional for A, needed for B) |
| | **Full A extra:** 13–15, plus the remote behaviour of the other pages (showfile management and imports via engine-host paths, the audio debug view, plugin UIs) at 2–4 d | | | **+7–12 d** |

Desk-only paths and web specifics for #10:
- `SetFrontendSection` is desk-only. It is sent from `publish_frontend_section` and refused for remote clients;
  results are ignored, so no popups appear.
- External screens are native viewports.
- Quit / Restart / Shutdown.
- Showfile Load / Save use engine-host paths.
- `absolute_showfile_path` uses `current_dir`.
- `BlaulichtApp::new` forces `set_pixels_per_point(1.0)`.
- There is no disconnect UI, and a reconnect gets a new session, so the selection is lost.

### Option B (Svelte), assessed
- **What `crates/core/web` contains:**
  - 74 `.svelte` and 27 `.ts` files.
  - About 2200 lines of app code, plus about 2200 lines of the generated shadcn-svelte kit (button, card, dialog,
    slider, sheet, sidebar, tooltip).
  - Last changed 2026-06-23.
- **Reusable:**
  - the SvelteKit, Vite and Tailwind scaffold
  - the UI kit
  - perhaps `Terminal.svelte` (logs) and the d3 audio monitor (against the `Spectrogram` stream), for later pages
- **Dead:**
  - `lib/websocket.ts`, the old topic protocol on `api/ws`
  - `lib/fetch.ts` (`/api/state`)
  - `lib/types/state.ts`, a fixture model from before scenes, palettes and sessions
  - the stage editor and viewer, which use the same old model
  - every route

  There is no Programmer at all.
- **Work for 054:**
  - a TypeScript client for the 047 protocol, which must also parse composite map keys like `"[0,5]"` (1 d)
  - TypeScript types for `EngineState`, `OperatorSession`, `ControlEvent` and `HostCommand`, hand-written or with
    ts-rs/specta derives on shared and engine types; derives don't change the bincode ABI (1–2 d)
  - the Programmer UI from scratch, including logic that egui gets from the engine crate, such as
    `fixture_selection` / `resolve_selection` and the control buffer (4–6 d)
  - engine traffic work: #15 (0.5–1 d) plus fixture-level deltas for edits (1–2 d). While a fader is dragged, the
    focused scene's `fixture_states` changes at 10 Hz, which is 314 KB of JSON per OUTDOOR scene even with
    per-section snapshots.
  - node and pnpm in the blenv image and in the release build image (0.5–1 d)
  - serving, the same as A (0.5–1 d)
  - verification and tests (1.5 d)
  - Total: **10–15 d**.
- **Feasibility check** (measured; a stdlib-only Python client, headless engine with a copy of
  `crates/core/OUTDOOR.json`):
  - Text `Hello` → `Welcome {client: {Remote: 0}, session: 1, protocol_version: 1, engine_version: "0.30.0"}`.
  - First `Show`: 1241.7 KB. Then 68 snapshots in 8.0 s, or 8.5 per second at **10.8 MB/s**. `json.loads` took
    8.1 ms on average (CPython in the container).
  - **Which sections change:**
    - `scenes` (1296.5 KB of JSON) changed in 67 of 67 snapshots. `groups` (96.7 KB) never changed. Everything
      else is under 1 KB.
    - Inside `scenes`: `sink.fixture_states` (774 KB) never changed. `sink.changeset` (378 KB) never changed when
      compared order-insensitively; it is a `HashSet`, so its JSON order differs between snapshots, which matters
      for any text diff.
    - `sink.active_animations` (129 KB) changed in 42 of 42. Those changes are the per-fixture `fixture_timers`
      (`last_tick_time`, `timer`).
    - So about 10% of the bytes cause every snapshot. With the timers out (#15), idle traffic drops to about zero
      in both encodings.
  - **Programmer round trip in JSON:** `SelectGroup(0)` and `SetAlpha(200)` both returned `Ok`. The client's
    session selection became `{0}` with `control_buffer.alpha = Literal 200`, and the desk's selection stayed
    empty.
  - **The results arrived after 330 ms:** they queued behind 1.2 MB show frames (head-of-line blocking).

### Trade-offs
| | A / slim A (egui on wasm) | B (Svelte) |
|---|---|---|
| Code sharing with the native UI | Same Programmer code and engine read helpers. Fixes land in both. | None. The Programmer, selection logic and types are duplicated in TypeScript. |
| Bundle | About 2.5 MB gzip (6.5–6.8 MB raw) wasm, cacheable, plus wasm compile at startup | Probably 0.1–0.3 MB gzip (estimate) |
| Touch and kiosk UX | Touch works (eframe maps touch to the pointer), the layout is already touch-first at 800x480 and fits tablets in landscape. Canvas rendering means no hover, clumsier mobile keyboard, no browser zoom or reflow, and no phone-portrait layout. | Native inputs and responsive CSS. All of it is still to be built. |
| Bandwidth | bincode: 126 KB × 8.7 Hz = 1.1 MB/s (OUTDOOR), about 0 at idle after #15 | JSON: 10.8 MB/s measured, 8 ms parse per snapshot, 330 ms result latency behind snapshots. Needs #15 and deltas. |
| Frame cost | Native Programmer on OUTDOOR was 54 ms per frame in a debug build (047). Release wasm is probably fine for Kuze-size shows; a 500-fixture show on a tablet is a risk (not measured). | DOM updates on each snapshot; also not measured |
| Maintenance (solo) | One Rust codebase. The compiler catches protocol changes. Web-only risk: runtime panics on wasm (mitigated by #5 lints and #13). | A second stack (node/pnpm, Svelte 5, TypeScript types) that was stale since June and drifts from the native UI |

### What changed
- `crates/ui_egui/src/lib.rs`: `pub mod desk` is `#[cfg(not(target_arch = "wasm32"))]`. This is the only code
  change; native builds are unaffected.
- **Throwaway branch `spike/048-wasm`** (local, not pushed; one commit on top of `chore: start ticket 048`; do not
  merge). It contains:
  - the full experiment: the L1 engine gates, the getrandom deps, the `web_time` swap and its two fixes
  - `spike/048/web_probe` and `web_baseline` (the bundle-size cdylibs, built outside the workspace)
  - `json_client.py` and `json_scenes.py` (the JSON clients)
  - `webtime_swap.py`
- No tools were installed. The `wasm32-unknown-unknown` target was already in the container.

### Verification
- `cargo clippy --workspace --all-targets --all-features`: blaulicht-engine lib 39 / lib test 42,
  blaulicht-ui-egui lib 89 / lib test 104, binaries 0. That is the baseline.
- `cargo test --workspace --all-features`: all pass (engine 261, ui-egui 101).
- `cargo fmt --check -p blaulicht-ui-egui`: `lib.rs` is clean (only the usual unrelated hunks).
- Disk: 11 GB free at the start.
  - My wasm builds added 3.3 GB of debug artifacts and 0.7 GB for the release probe. I deleted both: only the
    artifacts newer than my first build in this worktree's `/build/target/wasm32-unknown-unknown/debug` (back to
    1.5 GB, plugin artifacts untouched), plus `/build/target/spike048`.
  - Free space ranged from 6.1 to 9.9 GB while other containers were building, and it never fell below 4 GB.

### Found on the way (Inbox)
- **`crates/engine/src/bin/blaulicht-headless.rs` was never committed.** `.gitignore`'s `blaulicht-*` (line 16)
  ignores it, so a fresh clone has no headless binary and `scripts/blenv build headless` fails. The file exists
  only in this worktree. My `web_time` script touched it, and I restored it by hand (one import line), because
  `git checkout` can't restore an ignored file.
- The existing Inbox entry on full show snapshots now names the cause (the animation timers).
