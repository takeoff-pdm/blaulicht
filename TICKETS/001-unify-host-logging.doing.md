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
- [ ] `grep -rn 'syslog!\|SystemMessage::Log(' crates/core/src` only hits the layer/handler itself.
- [ ] No bracket-prefixed log messages remain (`grep -rnE '"\[[A-Za-z-]+\]' crates/core/src` near tracing calls is empty or justified).
- [ ] Logs page shows a meaningful Source column and can filter by it.
- [ ] Terminal output and Logs page show the same events (minus terminal-only).

## Verification
- `cargo clippy --all-targets --all-features` and `cargo fmt` are clean.
- `cargo test --all-features` passes, and new logic has co-located `#[cfg(test)]` tests.
- Follow **UI Verification** in `crates/core/AGENTS.md` (check that `swaylock` is not running, launch from the repo root, navigate via blctl, capture with grim). Use the headless Xvfb recipe when clicks are needed or another session is running (see memory notes). Take before and after screenshots, keep them in the scratchpad, and never commit them.
- Screenshot the Logs page after startup (with plugins loaded) and with a source filter applied.

## Notes
<!-- Implementation notes, decisions, and follow-ups found while working. Keep this updated before renaming to .done.md. -->
