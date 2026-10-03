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
- [x] No absolute path is visible on the System page without hovering.
- [x] Audio header reads left to right: input name, then change button.

## Verification
- `cargo clippy --all-targets --all-features` and `cargo fmt` are clean.
- `cargo test --all-features` passes, and new logic has co-located `#[cfg(test)]` tests.
- Follow **UI Verification** in `crates/core/AGENTS.md` (check that `swaylock` is not running, launch from the repo root, navigate via blctl, capture with grim). Use the headless Xvfb recipe when clicks are needed or another session is running (see memory notes). Take before and after screenshots, keep them in the scratchpad, and never commit them.
- Screenshot System › General and the Audio page, before and after.

## Notes
- System › General shows `config::showfile_display_name` (file name only, falls back to the whole path if there is none); the absolute path is the label's hover tooltip.
- `config::absolute_showfile_path` joins relative paths onto the cwd by hand instead of `std::path::absolute`, which is above the crate's MSRV (1.76) and trips clippy's `incompatible_msrv`. Both helpers have unit tests in `config.rs`.
- Logging: `read_showfile` already logged `Loaded showfile …` at info; it now logs the absolute path. Saves log `Saved showfile <abs>` at info from the save thread on success (manual and autosave alike). A save with no changes is skipped by the existing hash check and logs nothing.
- Audio header: `Current Input:` + name, then `[Change Device]` with no separator in between, then a separator and `[Info]`.
- Verified headless (Xvfb :9, `audio-mock`, scratch config + scratch copy of `SPARTACUS_DRAFT.json`):
  - before/after shots of System › General and Audio at 800x480; the full path no longer shows, `show.json` does.
  - hovering the name shows the full absolute path as a tooltip.
  - logs: `Loaded showfile /tmp/…/show.json` on startup; `Saved showfile /tmp/…/show.json` after `SelectGroup 0` + `SetAlpha 77` and a click on Save Showfile.
  - narrow dynamic layout (Audio dock tab set to `Dynamic` in the scratch showfile, `--desktop-mode`): at ~540 px wide the header fits one line; at ~300 px it wraps to input + Change Device on line 1 and Info on line 2.
- Cosmetic follow-up (not changed): when the header wraps, the separator before `[Info]` stays at the end of the first line.
- `cargo fmt` also reformats `app/pages/animations.rs` (pre-existing drift); that change was left out of this ticket.
