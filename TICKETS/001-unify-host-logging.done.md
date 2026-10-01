# 001 — Unify host logging (one path to terminal + Logs page)

| | |
|---|---|
| Epic | E1 Foundations |
| Depends on | none |
| ABI / showfile | none |

**Touches:** `crates/core/src/main.rs`, `crates/core/src/utils.rs`, `crates/core/src/msg.rs`, `crates/core/src/app/components/log.rs`, `crates/core/src/app/event.rs`, `crates/core/src/mainloop/supervisor.rs`, `crates/audio_engine (println -> tracing)`, many call sites (mechanical)

## Problem
Original review notes (2026-09-27):

> noticed that logging is a bit unstructured and wild

## Scope
- Make `tracing` the only host logging API. `init_tracing` in `main.rs` already forwards events to the UI through `SystemOutLayer`, so a `tracing` call shows up in both the terminal and the Logs page.
- Replace the `syslog!` macro (`src/utils.rs:18`), which logs to the UI only, and the ~15 direct `system_out.send(SystemMessage::Log(..))` sites with `tracing` calls. Remove the duplicated double-logging in `mainloop/supervisor.rs` and resolve the TODO there about an aggregate log macro.
- Replace the bracket/colon prefixes (`[SUPERVISOR]`, `[audio]` vs `[AUDIO]`, `WASM:`, `SUCCESS:`, …) with tracing `target:`s from one fixed list, e.g. `bl::engine`, `bl::audio`, `bl::plugin`, `bl::dmx`, `bl::midi`, `bl::udp`, `bl::serial`, `bl::ui`, `bl::init`. Keep `TERMINAL_ONLY_LOG_TARGET` working.
- Fill the Logs page `source` column from the target instead of the hardcoded `"System"` (`app/event.rs:93-125`). Plugin logs (`WasmLog`) use the plugin's file stem as the source instead of `PID: n |` inside the message.
- Add a source filter to the Logs page next to the existing level and text filters, e.g. to show only one plugin.
- Remove the unused `utils::init_logger`. Convert the `println!`s in `audio_engine` (22) and core (5) to `tracing`.
- Choose levels on purpose: per-frame or per-tick spam goes to `trace`/`debug`, and lifecycle events go to `info`.

## Out of scope
- Plugin-side `println!` → `bl_log` mapping in `plugin_framework` (can stay Info).
- Log persistence or flight recorder.

## Key code
- `init_tracing`, `SystemOutLayer` — `crates/core/src/main.rs`
- `syslog!` — `crates/core/src/utils.rs:18`
- `LogWindow`, `LogEntry` — `crates/core/src/app/components/log.rs`
- `SystemMessage::Log` / `WasmLog` handling — `crates/core/src/app/event.rs:93-125`

## Acceptance criteria
- [x] `grep -rn 'syslog!\|SystemMessage::Log(' crates/core/src` only hits the layer/handler itself.
- [x] No bracket-prefixed log messages remain (`grep -rnE '"\[[A-Za-z-]+\]' crates/core/src` near tracing calls is empty or justified).
- [x] Logs page shows a meaningful Source column and can filter by it.
- [x] Terminal output and Logs page show the same events (minus terminal-only).

## Verification
- `cargo clippy --all-targets --all-features` and `cargo fmt` are clean.
- `cargo test --all-features` passes, and new logic has co-located `#[cfg(test)]` tests.
- Follow **UI Verification** in `crates/core/AGENTS.md` (check that `swaylock` is not running, launch from the repo root, navigate via blctl, capture with grim). Use the headless Xvfb recipe when clicks are needed or another session is running (see memory notes). Take before and after screenshots, keep them in the scratchpad, and never commit them.
- Screenshot the Logs page after startup (with plugins loaded) and with a source filter applied.

## Notes
**Decisions**
- New module `crates/core/src/log.rs` owns `init_tracing`, the UI layer (moved out of `main.rs`) and the fixed target list `log::target::{ENGINE, INIT, CONFIG, AUDIO, PLUGIN, DMX, MIDI, UDP, SERIAL, UI, WEB}`.
- The Logs page source comes from the event target. An explicit `target: target::X` wins; otherwise the core module path is mapped (`plugin::midi` -> MIDI, `dmx` -> DMX, `app` -> UI, `mainloop`/others -> Engine, `main.rs` -> Init). So only the ~10 call sites whose module is the wrong subsystem needed an explicit target, not all ~200 tracing calls. Foreign crates show their crate name.
- `SystemMessage::Log` is now `Log { message, level, source }`, and only the layer sends it. `log` crate records (audio_engine) are normalized with `tracing_log::NormalizeEvent`, so they get their real target and no `log.*` noise fields.
- Plugin logs (`WasmLog`) use the plugin's file stem as the source, e.g. `midi_all`.
- Senders that were only used for logging were removed: the `DmxEngine::system_out` field and the `sys` param, the `read_showfile` sender param, and the `increase_thread_priority` sender.
- Removed a duplicate "Loaded showfile" UI log in `system/showfile.rs` that fired even when loading failed.
- Level changes: per-fixture setup timing `debug` -> `trace`, per-event bus dump `debug` -> `trace`, clean engine-thread exit no longer warns "Thread died".
- audio_engine library `println!`s now go through the `log` crate. The CLI binaries (`audio_engine/src/main.rs`, `src/bin/audio_corpus.rs`) keep `println!`, since that is their output.

**Verification done**
- Tests: `cargo test -p blaulicht-core -p blaulicht-audio-engine` passes (191 tests, 5 new: target mapping, source filter, plugin source).
- `cargo check` passes for the default features and for `--no-default-features --features "wayland audio-mock"`.
- clippy (`--all-targets --all-features`) raises no warning on a changed line. The workspace still has ~226 older clippy warnings, so clippy is **not** clean overall.
- `cargo fmt` was run. Unrelated formatting drift in `animations.rs` was reverted to keep the diff scoped.
- Terminal: 27 log lines on startup, no prefixes, no info-level output per tick.
- Visual: headless Xvfb recipe, `--desktop-mode`, `SPARTACUS_DRAFT.json` loaded, plugins loaded.
  - The Logs page shows the sources MIDI, Plugin, DMX, Engine, Init and plugin names.
  - The Filter Source dialog lists 14 sources. Selecting DMX leaves only the two DMX errors, and the button is highlighted.
  - The same DMX errors appear in the terminal.
- No "before" screenshot was taken. Before this change the column showed only "System"/"WASM", with `PID: n |` inside plugin messages (from the code).

**Follow-ups (added to the TODO.md Inbox)**
- Plugin-side messages still carry their own prefixes (`[MIDI]`, `[LEGACY]`, `[Midi All]` in `midi_all`), and `midi_all` logs every event at info (`---> EVENT: ...`).
- The Logs page keeps only 100 entries (`LogWindow::new(100)`). Startup alone nearly fills it.
- A nightly rustc ICE (incremental dep-graph) happened once during the x11 build; `CARGO_INCREMENTAL=0` worked around it.

**Follow-ups done (2026-10-01)**
- Plugin-side prefixes are removed in all plugins: 433mhz_antenna, ddj_200, ddj_400, drums, inspector, midi_all, midilight, pioneer_pro_dj_link, player, sample_egui_plugin, screens. The Logs page source already names the plugin.
- `midi_all` has a "Verbose logging" switch on its Misc tab (widget id 1, next to Fans). It is off by default and not persisted. While it is on, per-event diagnostics are logged at Debug: bus events, unhandled MIDI, nanoKONTROL selection-mode notices and scene-pad sync. Lifecycle messages are always logged. Mapping (de)serialization failures and presses on a missing scene now log at Err/Warn.
- The Logs page keeps 200 entries.
- Verified on the headless display: no `Event:` lines with verbose off; `Event: …` Debug lines appear right after switching it on and stop after switching it off.

