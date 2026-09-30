# 003 — Showfile name instead of abspath; audio page header order

| | |
|---|---|
| Epic | E1 Foundations |
| Depends on | none |
| ABI / showfile | none |

**Touches:** `crates/core/src/app/pages/system/misc.rs`, `crates/core/src/app/pages/audio.rs`

## Problem
Original review notes (2026-09-27):

> showfile does not need abspath, maybe under detail or in console log
> audio page maybe put current input to the very left, and change device right next to it.

## Scope
- System › General (`system/misc.rs:147-163`): show only the showfile's file name, with the full path in a hover tooltip. Log the absolute path at `info` when a showfile is loaded or saved (`system/showfile.rs`, `config::read_showfile`).
- Audio page top row (`audio.rs:~783-830`): order it as `Current input: <name>` on the far left, then `[Change Device]` right next to it, then `[Info]`. It must still wrap cleanly in narrow dynamic layouts.

## Out of scope
- Dummy audio device (ticket 004).

## Key code
- `crates/core/src/app/pages/system/misc.rs:147-163`
- `audio_ui` top row — `crates/core/src/app/pages/audio.rs:~783`

## Acceptance criteria
- [ ] No absolute path is visible on the System page without hovering.
- [ ] Audio header reads left to right: input name, then change button.

## Verification
- `cargo clippy --all-targets --all-features` and `cargo fmt` are clean.
- `cargo test --all-features` passes, and new logic has co-located `#[cfg(test)]` tests.
- Follow **UI Verification** in `crates/core/AGENTS.md` (check that `swaylock` is not running, launch from the repo root, navigate via blctl, capture with grim). Use the headless Xvfb recipe when clicks are needed or another session is running (see memory notes). Take before and after screenshots, keep them in the scratchpad, and never commit them.
- Screenshot System › General and the Audio page, before and after.

## Notes
<!-- Implementation notes, decisions, and follow-ups found while working. Keep this updated before renaming to .done.md. -->
