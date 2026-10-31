# 045 — Audio, Art-Net and System pages onto EngineClient

| | |
|---|---|
| Epic | E10 Decouple |
| Depends on | 042, 039 |
| ABI / showfile | none |

**Touches:** `crates/core/src/app/pages/audio.rs`, `crates/core/src/app/pages/system/{artnet,misc,health,screens}.rs`, `crates/core/src/msg.rs`, `crates/core/src/mainloop/*.rs`, `crates/core/src/main.rs`, `crates/core/src/app/components/log.rs`

## Problem
- `audio.rs` writes `audio_params` 6 times (`:661`, `:670`, `:1066`, `:1077`, `:1088`, `:1100`). The
  `changed` flag protocol is consumed at `mainloop/mod.rs:366`. The page writes config at `audio.rs:994`.
- Audio input selection sends a `cpal::Device` (`FromFrontend::SelectInputDevice`, `msg.rs:232`, `:253`), and
  `SystemMessage::AudioDevicesView` / `AudioSelected` (`msg.rs:69–70`) carry devices. None of these can be
  serialized.
- `artnet.rs` writes `artnet_output` directly (3 sites).
- Plugin Reload is at `misc.rs:242`; there is no enable toggle.

## Scope
- Audio params, Art-Net receivers and plugin reload become `HostCommand`s. `FromFrontend` is folded into
  `HostCommand`.
- Audio input is selected **by name** (`AudioInput` resolved in the engine, including the `Dummy:` inputs). The
  device list is published as plain data (host name, device name). The config write for `default_audio_device`
  moves into the engine handler.
- Logs, health, tick speeds and the device list reach these pages through `EngineClient` streams (049).

## Out of scope
- Showfile, autosave and config ownership (052). 019 (DMX overrides redesign).

## Acceptance criteria
- [ ] No production code in these files touches `self.data.state`, `AppState`, `FromFrontend` or the config
      mutex.
- [ ] Switching audio input (real and dummy), changing audio params, editing Art-Net receivers and reloading
      plugins work as before.

## Verification
- `cargo clippy --workspace --all-targets --all-features`, `cargo fmt`, `cargo test --workspace --all-features`.
- blenv: Audio page screenshot after switching Noise ↔ Silence; Art-Net page after adding a receiver.

## Notes
