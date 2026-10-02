# 002 — Visible startup progress (per-stage checklist)

| | |
|---|---|
| Epic | E1 Foundations |
| Depends on | 001 |
| ABI / showfile | none (host-internal `SystemMessage` only; do not touch plugin-visible types) |

**Touches:** `crates/core/src/main.rs`, `crates/core/src/mainloop/mod.rs`, `crates/core/src/mainloop/supervisor.rs`, `crates/core/src/app/popup.rs`, `crates/core/src/app/mod.rs`, `crates/core/src/msg.rs`, `crates/core/src/app/components/navbar.rs`

## Problem
Original review notes (2026-09-27):

> no clear overview whether system startup is finished or still in progress

## Scope
- Add a `SystemMessage::StartupStage { stage, status }` (host-internal) with stages: Config, Showfile, Audio, Plugins (k/N, with names of failures), DMX engine, Main loop running.
- Emit them from `main.rs` (showfile load), `mainloop::run` (plugin init loop, DMX, audio) and the supervisor. The `EngineInitializationGuard` still completes on drop, and a failed stage is reported as failed rather than silently done.
- `render_init_popup` (`src/app/popup.rs:~73`) shows a checklist (pending / running spinner / ok / failed) instead of a bare "Loading…".
- Once the popup is gone, show a small navbar status indicator if any stage failed or is still running, e.g. plugins still compiling after the 15 s failsafe. Clicking it navigates to the Logs page.
- An engine restart by the supervisor after a crash also shows up as "restarting".

## Out of scope
- Making startup faster.
- Async DMX setup.

## Key code
- `render_init_popup` — `crates/core/src/app/popup.rs`
- `engine_initialization_complete` — `crates/core/src/app/mod.rs:227`, `app/event.rs:90`
- `EngineInitializationGuard`, `mainloop::run` — `crates/core/src/mainloop/mod.rs`
- `supervisor_thread` — `crates/core/src/mainloop/supervisor.rs`

## Acceptance criteria
- [x] The popup shows every stage's state during a real startup, and plugins show a k/N count.
- [x] A deliberately broken plugin (e.g. a truncated `.wasm` in a scratch config) makes the Plugins stage show as failed, and the navbar indicator stays visible.
- [x] A normal startup ends with no indicator.

## Verification
- `cargo clippy --all-targets --all-features` and `cargo fmt` are clean.
- `cargo test --all-features` passes, and new logic has co-located `#[cfg(test)]` tests.
- Follow **UI Verification** in `crates/core/AGENTS.md` (check that `swaylock` is not running, launch from the repo root, navigate via blctl, capture with grim). Use the headless Xvfb recipe when clicks are needed or another session is running (see memory notes). Take before and after screenshots, keep them in the scratchpad, and never commit them.
- Take several screenshots in a row during startup (debug build, so the popup stays long enough) and one after startup.

## Notes
<!-- Implementation notes, decisions, and follow-ups found while working. Keep this updated before renaming to .done.md. -->
**Implementation**
- Types live in `msg.rs`: `StartupStage`, `StageState` (`Pending/Running/Ok/Failed/Restarting`) and `StageStatus { state, detail }`.
  They're host-internal, so there's no ABI or showfile change. The UI model is `app/startup.rs::StartupProgress`. `indicator()` returns
  Failed > Restarting > Running/Pending > none.
- Emitters: `main.rs` (Config = file name, Showfile = name / failed / "none"), `mainloop::run` via the extended
  `EngineInitializationGuard` (`begin`/`finish`, Audio + DMX engine + Main loop), and the plugin manager. The plugin manager also touches
  `plugin/wasm.rs` + `plugin/mod.rs`, which aren't in Touches but are where the k/N count comes from:
  `running("k/N · <stem>")` per plugin, and a final `ok("N/N")` / `failed("k/N · failed: a, b")` after the initial tick.
  The supervisor emits Audio selection, `Main loop running("starting engine")`, `failed("crashed: …")` on engine error,
  and `restarting("restarting after crash")` + Pending for Audio/Plugins/DMX before a crash restart.
- If the guard is dropped before `complete()`, the current stage is marked `failed("engine stopped during init, see Logs")`,
  and the popup is still released.
- A plugin hot reload (`reload()` → `init()`) re-emits the Plugins stage too, so a reload shows BUSY and then clears it, or shows
  FAIL if the reloaded plugin is broken.
- Navbar: when there's an indicator, the navbar gets one extra slot at the bottom (BUSY blue / RESTART amber / FAIL red). Its hover text
  lists the non-ok stages, and a click navigates to Logs through the normal `NavigatePage` path.

**Verification (2026-10-03, headless Xvfb :9, debug x11 + audio-mock + wasmtime, scratch copies of config + showfile)**
- Before: the HEAD build showed only "Loading…" plus the last log line, and the popup closed at the 15 s failsafe while plugins were still compiling, with
  nothing visible afterwards.
- Normal startup: frames show Config/Showfile ok, Plugins `1/6 · inspector` spinner, and the rest pending. After the failsafe
  the navbar shows BUSY until all 6 plugins load (~90 s in debug), and then there's no indicator.
- Broken plugin (truncated `midilight.wasm`): with 6 plugins, the FAIL slot stays, the hover reads `Plugins: failed (5/6 · failed:
  midilight)`, and clicking it from the System page opens Logs (xdotool). With only the broken plugin enabled, the popup shows
  every stage resolved: Config/Showfile/Audio/DMX/Main loop ok and Plugins red `0/1 · failed: midilight`.
- `cargo clippy --all-targets --all-features` has no new warnings in touched code. `cargo test --all-features` passes 176 tests, 9 of them new
  (startup indicator, guard drop, plugin summary).
- `cargo fmt --check` only reports `app/pages/animations.rs:856`. That's existing drift on the base commit and was left untouched.
- **Not verified live:** the crash-restart path ("RESTART"). There's no cheap way to crash the engine on demand, so it's covered by
  unit tests only (`restart_sequence_ends_without_indicator`) and by reading the code.

**Follow-ups / limits**
- Desktop mode (`--desktop-mode`) has no navbar, so there's no indicator there. Only the init popup applies.
- The DMX engine stage is ok as soon as `DmxEngine::new` returns. Missing serial DMX devices (`/dev/ttyUSB0`) are logged, not
  reported as a failed stage. Audio is ok once a device is selected; a stream that later fails to open is retried and logged.
- With debug builds and 6 plugins, the popup always hits the 15 s failsafe before Plugins finishes. The navbar BUSY slot covers that.
- Building a second worktree reuses `~/.cache/cargo` (direnv overrides `CARGO_TARGET_DIR`), and that corrupted
  `blaulicht_core` incremental state (rustc ICE in `dep_graph`). Fixed by deleting `~/.cache/cargo/debug/incremental/blaulicht_core-*`.
