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
- [x] Both dummy devices can be selected, persist across restarts, and `blctl.py audio` shows plausible values (zero for silence, non-zero for noise).
- [x] A system without audio hardware starts on `Dummy: Silence` with no error spam.

## Verification
- `cargo clippy --all-targets --all-features` and `cargo fmt` are clean.
- `cargo test --all-features` passes, and new logic has co-located `#[cfg(test)]` tests.
- Follow **UI Verification** in `crates/core/AGENTS.md` (check that `swaylock` is not running, launch from the repo root, navigate via blctl, capture with grim). Use the headless Xvfb recipe when clicks are needed or another session is running (see memory notes). Take before and after screenshots, keep them in the scratchpad, and never commit them.
- Screenshot the Audio page with `Dummy: Noise` selected (the spectrogram moves).

## Notes
**Implementation**
- New `msg::AudioInput { Device(AudioDeviceT), Dummy(DummyInput) }` replaces the raw device in `FromFrontend::SelectInputDevice`,
  `SystemMessage::AudioSelected` and `mainloop::run`. `DummyInput::name()` holds the reserved names `Dummy: Silence` /
  `Dummy: Noise`; `utils::input_from_name` resolves them before looking up hardware (used by startup and the Audio page).
- The `audio` feature now enables `blaulicht-audio-engine/noise`. `RecoveringAudioSource` has three inputs: hardware (reopened
  every 2 s as before), silence (no frames, so the collector reports `Disconnected`, volume 0, no analysis cost) and noise.
- Noise frames are rate-limited to one per 10 ms. `AudioSourceNoise` reports a new frame on every call, which would otherwise run
  the full analysis at mainloop tick rate. `AudioSourceNoise::frequencies()` was added for the in-between ticks.
- Supervisor: `SelectInputDevice(None)` and "no hardware found" both become `Dummy: Silence`. The "No input device" error,
  the failed Audio stage and the "waiting for selection" log are gone. While a dummy is active the device list keeps refreshing
  every heartbeat (as it did for "none" before), so hot-plugged hardware shows up in the popup.
- The popup's `None` entry is replaced by the two dummy entries. The page shows the dummy's name as "Current Input".
- No ABI or showfile change.

**Verification (2026-10-03)**
- `cargo clippy --all-targets --all-features`: no new warnings in touched code. `cargo fmt` was applied.
  `cargo test --all-features`: 180 passed, including the new `mainloop::audio::tests::{dummy_silence_*, dummy_noise_*, dummy_names_round_trip}`.
  Also checked the mock build (`wayland audio-mock wasmtime`).
- Headless Xvfb at 800x480 (not desktop mode), `x11 audio wasmtime`, scratch config and showfile copy:
  - No hardware (`ALSA_CONFIG_PATH` pointed at an empty file), no configured device: the log has one
    `WARN No input device is available; using Dummy: Silence`, the engine starts, and the page shows `Current Input: Dummy: Silence`.
    `blctl audio`: `Disconnected`, volume 0. In the same setup before the change: `N/A`, a FAIL startup badge, an ERROR log, and the popup only offered `None`.
  - Selecting `Dummy: Noise` in the popup (xdotool clicks) writes `default_audio_device = "Dummy: Noise"`. `blctl audio`: `Active`,
    volume ~190, bass 255, BPM moves; the spectrogram fills in from the right.
  - After a restart with real hardware present it comes back on `Dummy: Noise`. The popup lists pipewire/default/sysdefault plus both dummies.
  - Selecting `Dummy: Silence` writes the config; after a restart it is still on silence (volume 0). Switching back to `pipewire` works (`Active`).

**Follow-ups**
- The noise is loud: uniform 0..50 per bin saturates bass/volume and draws the spectrogram as a solid red block. A quieter or
  shaped noise (e.g. pink) would be more useful for testing animations.
- After a device change the UI jumps to the Logs page while the engine restarts (existing behaviour, not changed here).
- The device list is still only sent while no hardware device is active, so with a configured hardware device the popup
  lists just the two dummies (existing behaviour, the list used to be empty; from reading the code, not tested).
