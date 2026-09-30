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
- [ ] The popup shows every stage's state during a real startup, and plugins show a k/N count.
- [ ] A deliberately broken plugin (e.g. a truncated `.wasm` in a scratch config) makes the Plugins stage show as failed, and the navbar indicator stays visible.
- [ ] A normal startup ends with no indicator.

## Verification
- `cargo clippy --all-targets --all-features` and `cargo fmt` are clean.
- `cargo test --all-features` passes, and new logic has co-located `#[cfg(test)]` tests.
- Follow **UI Verification** in `crates/core/AGENTS.md` (check that `swaylock` is not running, launch from the repo root, navigate via blctl, capture with grim). Use the headless Xvfb recipe when clicks are needed or another session is running (see memory notes). Take before and after screenshots, keep them in the scratchpad, and never commit them.
- Take several screenshots in a row during startup (debug build, so the popup stays long enough) and one after startup.

## Notes
<!-- Implementation notes, decisions, and follow-ups found while working. Keep this updated before renaming to .done.md. -->
