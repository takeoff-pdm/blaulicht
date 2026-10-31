# 045 — System and Audio pages onto EngineClient

| | |
|---|---|
| Epic | E10 Decouple |
| Depends on | 042 |
| ABI / showfile | none |

**Touches:** `crates/core/src/app/pages/audio.rs`, `app/pages/system/*.rs`, `crates/core/src/msg.rs`, `crates/core/src/mainloop/*.rs`, `crates/core/src/app/components/log.rs`

## Problem
`audio.rs` writes `audio_params` directly 19 times. The System pages write the showfile and Art-Net output
directly: `showfile.rs` (4), `artnet.rs` (3). Audio input selection sends a `cpal::Device` through
`FromFrontend::SelectInputDevice`, which can't be serialized for a remote client.

## Scope
- Audio params, Art-Net receivers, showfile load and save, grand master and plugin enable/reload become commands.
  `FromFrontend` either merges into the same command type or becomes a serializable sibling; decide here and
  write it down in Notes.
- Audio input is selected **by name** (`AudioInput` resolved inside the engine). The device list arrives as plain
  data (host name, device name).
- Logs, health and tick speeds reach the page through `EngineClient` as read-only data.
- Afterwards no file under `crates/core/src/app/` takes a write lock on `AppState`. Add a CI-style test or
  `clippy.toml`/grep check to the `Makefile` so new ones don't creep back in.

## Out of scope
- 019 (DMX overrides redesign). Don't run them at the same time.

## Key code
- `FromFrontend`, `AudioInput` — `crates/core/src/msg.rs`
- Default device selection at boot — `crates/core/src/main.rs:~70`

## Acceptance criteria
- [ ] `grep -rnE '\.(write|lock)\(\)' crates/core/src/app` finds no `AppState` access.
- [ ] Switching audio input (real and dummy), changing audio params, editing Art-Net receivers, and loading and
      saving a showfile all work as before.

## Verification
- `cargo clippy --all-targets --all-features`, `cargo fmt`, `cargo test --all-features`.
- `make mock-run` and a real-audio run both start and switch inputs.
- UI: screenshots of the Audio and System pages.

## Notes
