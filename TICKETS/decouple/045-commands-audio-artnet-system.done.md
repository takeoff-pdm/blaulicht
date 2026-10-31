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
- [x] No production code in these files touches `self.data.state`, `AppState`, `FromFrontend` or the config
      mutex.
- [x] Switching audio input (real and dummy), changing audio params, editing Art-Net receivers and reloading
      plugins work as before.

## Verification
- `cargo clippy --workspace --all-targets --all-features`, `cargo fmt`, `cargo test --workspace --all-features`.
- blenv: Audio page screenshot after switching Noise ↔ Silence; Art-Net page after adding a receiver.

## Notes

### What changed
- **`FromFrontend` is gone.** Its two cases became `HostCommand`s, carried out by the supervisor thread:
  - Routing: the page sends the command on the bus; `DmxEngine::apply_host` (main loop thread) checks it,
    answers, and forwards a `mainloop::SupervisorRequest` (`ReloadPlugins` / `SelectAudioInput(Option<AudioInput>)`)
    over a crossbeam channel (`DmxEngine::with_supervisor`, set in `mainloop::run`). The supervisor owns and restarts
    the main loop, so an input switch can't run on the main loop thread itself. The plugin watcher sends
    `ReloadPlugins` on the same channel; `main.rs` hands the configured input to `supervisor_thread` as
    `initial_input` (no request, so startup doesn't rewrite the config).
  - Latency: the supervisor no longer sleeps 1 s between rounds; it waits with `recv_deadline(next_heartbeat)`, so a
    request wakes it at once (heartbeats keep their 1 s cadence). In blenv the restart was logged in the same second
    as the click (stop to start 16 ms); on the before build it came ~1 s after the click (the heartbeat sleep).
  - The supervisor writes `default_audio_device` (`config::SharedConfig::update`, which locks, edits and writes the
    file). It does this, not `apply_host`, because `apply_host` holds the `dmx_engine` write lock and the desk's
    showfile code takes the config lock *before* that lock (`load_showfile_path`): locking the config inside
    `apply_host` would be an ABBA deadlock. `SharedConfig.config` is the same `Arc<Mutex<Config>>` as
    `AppStateWrapper.config` (052 makes it engine-only).
- **New `HostCommand`s** (`hub.rs`; engine side in the new `dmx/system_commands.rs`):
  - `SelectAudioInput { name: Option<String> }`: a device name or a `DummyInput` name, resolved with
    `utils::input_from_name` in the engine; `None` = "Dummy: Silence" and config `None` (as before). Refused "Audio
    input not found" (nothing changes; before, an unavailable device silently became Silence) or "The engine
    supervisor is not running". `edits_show() == false` (config, not showfile).
  - `SetAudioParam(AudioParam)` with `Volume(u8)`, `Gate(u8)`, `Boost(Option<u8>)` (0 = off), `AutoCalibrate(bool)`,
    `ManualBpm(Option<f32>)` (refused "BPM must be positive" unless finite and > 0). Writes one field of
    `AppState::audio_params` and sets `changed`; the `changed` protocol in `mainloop/mod.rs` is unchanged (now
    commented there). `edits_show() == false` (not in the showfile).
  - `ReloadPlugins`: forwarded; the supervisor only reloads while the loop is `CONTINUE`, as before. Refused "The
    engine supervisor is not running". `edits_show() == false`.
  - `AddArtNetReceiver(SocketAddr)` (refused "Art-Net receiver must be IPv4" / "Art-Net receiver <addr> already
    exists"), `RemoveArtNetReceiver(SocketAddr)` (user receivers only; "Art-Net receiver not found"),
    `SetArtNetReceiverEnabled { address, owner: Option<u8>, enabled }` (any receiver, matched by address *and*
    owner; `edits_show()` is `owner.is_none()`), `SetArtNetReceiverUniverses { address, first, last }` (user only,
    clamped/swapped by `set_universe_range`). Add/Remove/Universes are `edits_show() == true`. The engine logs
    "Added/Removed Art-Net receiver" as the page did. Lock order: `dmx_engine` then `artnet_output`, as showfile
    loading.
  - No `CreatedId`, no `ControlEvent`, no ABI change.
- **Plain-data system messages:** `SystemMessage::AudioDevicesView(Vec<(String, String)>)` (host name, device
  name; `utils::input_device_names`) and `AudioSelected(String)` (input name). 049's replay keeps the latest of each
  for late subscribers (new test `a_late_subscriber_gets_the_latest_audio_devices_and_input`). The supervisor now
  also publishes the device list whenever the input changes (before, with a configured real device, the list was
  never published). `AppState::audio` / `AudioState` (only the audio page read it) are removed; the desk keeps
  `selected_audio_input` from `AudioSelected`. The device-list handler compares names instead of only the count.
- **Client** (`client.rs`): `EngineClient::mainloop_state() -> AudioThreadControlSignal` (Reload button, "LOOP ..."
  label) and `showfile_path() -> Option<PathBuf>` (default method: `show.stage_showfile()`, the engine's copy of the
  open showfile since 051). `TestEngine::connect_supervisor()` returns the receiver of forwarded requests.
- **Pages:**
  - `audio.rs`: params via `with_audio_params` + `SetAudioParam` (faders, auto-calibrate, manual BPM switch and
    numberpad); device dialog sends `SelectAudioInput` through `send_command` (refusal popup); the label shows
    `selected_audio_input`; the spectrogram and the info dialog read `with_spectrogram`. The 13 spectrogram ring
    fields moved from `BlaulichtApp` into `pages::SpectrogramRing` (methods moved verbatim) so the ring can be
    updated inside `with_spectrogram` without borrowing the whole app (still no copy of the spectrogram).
  - `system/artnet.rs`: parsing stays in the page (inline errors as before); Create sends `AddArtNetReceiver` and
    clears the form on `Ok` / shows the refusal inline (e.g. duplicate); toggle, range and delete send their
    commands. Reads `with_artnet_receivers`.
  - `system/misc.rs`: showfile name from `showfile_path()`, Reload sends `ReloadPlugins` (popup on `Ok`), loop
    state from `mainloop_state()`, DMX/MIDI/Serial tabs and the plugin list via `with_health` / `with_plugins`
    (copied out, no guard held while drawing). `health.rs`, `dmx.rs`, `midi.rs`, `serial.rs`: `with_health`,
    `with_artnet_receivers`, `with_plugins`. `screens.rs`, `speed.rs`, `system/mod.rs` and
    `components/log.rs` already touched none of the forbidden state (logs and tick speeds arrive as hub messages in
    `event.rs`).

### Decisions and deviations
- Outside Touches: `hub.rs`, `dmx/mod.rs`, `client.rs`, `config.rs` (`SharedConfig`), `state.rs`, `utils.rs`,
  `plugin/mod.rs` (watcher), `app/event.rs`, `app/mod.rs`, `pages/system/{dmx,midi,serial}.rs`, `AGENTS.md`.
- `showfile_path()` reads the stage's showfile path, not `config.last_open_showfile`. They differ only when loading
  a showfile fails (config keeps the name, the engine has none): the General tab then shows "N/A" instead of the
  file that didn't load. 052 should give the engine one "open showfile" field.
- Resolving a real device name enumerates the audio hosts on the main loop thread; on success the loop is restarted
  right after, so it doesn't matter; a refusal (device unplugged) costs one stall.
- "Reload in progress..." pops up when the engine accepted the command (a tick later) instead of on click.
- Pre-existing, unchanged (same on the before build `2799414`): "Auto-calibrate" never shows as checked (the
  collector's one-shot calibration clears it at once); after switching back to Dummy: Noise the spectrogram catches
  up with a burst (update_max ~10 ms for ~30 s) because the restarted loop's column times start over; the Art-Net
  receiver list's viewport shows about one row and scrolls.
- `routes/ws.rs` still mentions `FromFrontend` in commented-out code; left alone.

### Verification
- `cargo test --workspace --all-features`: all pass; blaulicht-core 332 -> 340. New
  `dmx::system_commands::tests` (desk and remote clients): `an_audio_input_is_selected_by_name_through_the_supervisor`
  (no supervisor -> refused; Noise, Silence, `None` and `ReloadPlugins` reach the supervisor),
  `an_unknown_audio_input_is_refused` (`audio` feature), `audio_params_are_set_one_at_a_time_and_flagged_for_the_collector`
  (incl. boost 0 -> off and the four BPM refusals), `user_artnet_receivers_are_added_edited_and_removed`,
  `a_plugin_receiver_can_only_be_turned_on_and_off`, `only_user_receiver_edits_mark_the_show`; plus
  `hub::tests::a_late_subscriber_gets_the_latest_audio_devices_and_input`,
  `config::tests::a_shared_config_update_is_written_to_its_file`, and the stream reader test covers
  `mainloop_state` / `showfile_path`.
- `cargo clippy --workspace --all-targets --all-features`: blaulicht-core lib 130, lib test 148 (after 051: 131 /
  149; the removed `AudioState::default` was one; the two engine-plumbing functions with 8+ args got an `allow`).
  `cargo build`: 83 lib warnings (unchanged). Also checked `--no-default-features --features "x11 audio-mock
  wasmtime"`.
- `cargo fmt`: touched files clean; the usual unrelated hunks (audio_features, visualizer/ui.rs, app/ui.rs, three
  in pages/audio.rs) reverted.
- **blenv** (copies of `Kuze_Theater.json`; screenshots read):
  - Desktop, Audio: "Current Input: Dummy: Noise" (from `AudioSelected`), spectrogram scrolling. Change Device ->
    Dummy: Silence: label switches, spectrogram stops, bass graph flat; logs show the stop and "Engine started with
    audio input "Dummy: Silence"" right after the click; `/state/config.toml` has `default_audio_device = "Dummy: Silence"`.
    Back to Dummy: Noise: spectrogram moves again, config updated. Gate fader -> 60 (volume gated to 1).
  - Kiosk, Audio: Volume -> 175 and Boost -> 104 (collector output follows); Info -> Manual BPM on: tracker shows
    "manual yes", latched 120.0 BPM; off again.
  - System > Art-Net: created 192.168.1.50 and .51; creating .50 again shows "Art-Net receiver 192.168.1.50:6454
    already exists." under the form; toggled .51 -> DISABLED; numberpad Last -> 2 (row shows 0-2); deleted .51.
    Autosave ran; the saved showfile's `artnet` has only .50.
  - System > General: showfile "kuze045.json", LOOP CONTINUE, health column (ArtNet ONLINE); Reload -> "Reload in
    progress..." popup, LOOP RELOAD, supervisor logged the request in the same second as the click.
  - Logs: only the usual DMX/MIDI/XSETTINGS/XRandR/thread-priority lines.
  - Before build (`2799414`) compared for: device switch latency, auto-calibrate checkbox, post-restart spectrogram
    burst.

### For 046, 047, 052, 053
- 052: `SharedConfig` is the seam for an engine-owned config (the supervisor already writes through it); the desk's
  remaining config users are `pages/system/showfile.rs`, `app/ui.rs` and `event.rs` (`SavePluginState`). Merge
  `StageState.showfile` and `config.last_open_showfile` into one engine field and back `showfile_path()` with it.
  Mind the lock order above if the engine locks the config while holding `dmx_engine`.
- 053: `SupervisorRequest`, `SharedConfig`, `dmx/system_commands.rs` and `utils::input_device_names` are engine
  code; `pages::SpectrogramRing` is frontend.
- 047: all new commands and both audio messages are plain data (no cpal types); `AudioInput` stays engine-internal.

