# 004 — Selectable dummy audio input (silence / noise)

| | |
|---|---|
| Epic | E1 Foundations |
| Depends on | none |
| ABI / showfile | none expected (`default_audio_device` in config stays a string; use a reserved name) |

**Touches:** `crates/core/src/mainloop/audio.rs`, `crates/core/src/mainloop/supervisor.rs`, `crates/core/src/app/pages/audio.rs`, `crates/core/Cargo.toml`, `crates/audio_engine/src/audio_source/noise.rs`

## Problem
Original review notes (2026-09-27):

> also use a dummy audio device if no audio source is selected

## Scope
- Make `AudioSourceNoise` (`audio_engine` feature `noise`) available in normal `audio` builds as well, not only as the `audio-mock` replacement.
- Add two virtual devices to the device list: `Dummy: Silence` and `Dummy: Noise`. They can be selected in `render_choose_audio_device_popup` like real devices and persist via `default_audio_device`.
- When no device is selected or available, the supervisor uses `Dummy: Silence` explicitly (today it is an implicit silent `RecoveringAudioSource`), and the Audio page shows that it is on the dummy input.

## Out of scope
- Audio normalization, beat-detection fixes.

## Key code
- `RecoveringAudioSource`, `CaptureSource` cfg switch — `crates/core/src/mainloop/audio.rs`
- Auto-select logic — `crates/core/src/mainloop/supervisor.rs`
- `render_choose_audio_device_popup` — `crates/core/src/app/pages/audio.rs:256`
- `AudioSourceNoise` — `crates/audio_engine/src/audio_source/noise.rs`

## Acceptance criteria
- [ ] Both dummy devices can be selected, persist across restarts, and `blctl.py audio` shows plausible values (zero for silence, non-zero for noise).
- [ ] A system without audio hardware starts on `Dummy: Silence` with no error spam.

## Verification
- `cargo clippy --all-targets --all-features` and `cargo fmt` are clean.
- `cargo test --all-features` passes, and new logic has co-located `#[cfg(test)]` tests.
- Follow **UI Verification** in `crates/core/AGENTS.md` (check that `swaylock` is not running, launch from the repo root, navigate via blctl, capture with grim). Use the headless Xvfb recipe when clicks are needed or another session is running (see memory notes). Take before and after screenshots, keep them in the scratchpad, and never commit them.
- Screenshot the Audio page with `Dummy: Noise` selected (the spectrogram moves).

## Notes
<!-- Implementation notes, decisions, and follow-ups found while working. Keep this updated before renaming to .done.md. -->
