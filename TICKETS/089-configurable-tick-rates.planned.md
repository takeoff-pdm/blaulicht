# 089 — Configurable DMX and plugin tick rates in `config.toml`

| | |
|---|---|
| Epic | Ops / show host |
| Depends on | none |
| ABI / showfile | none (config only; `config.toml` gets two optional keys) |

**Touches:** `crates/engine/src/config.rs`, `crates/engine/src/mainloop/mod.rs`, `crates/engine/src/audio/defs.rs`,
`crates/ui_egui/src/app/mod.rs`, `crates/core/config.toml`

## Problem
The engine's tick rates are compile-time constants:

- `DMX_TICK_TIME = 25 ms` (40 Hz), `crates/engine/src/mainloop/mod.rs:32`. A second, unused copy lives in
  `crates/engine/src/audio/defs.rs:6`.
- `PLUGINS_TICK_TIME = 12 ms`, `crates/engine/src/mainloop/mod.rs:33`.

While tuning the show host `bl` for low DMX latency (2026-11-12; see `install/tune.sh` and ticket 088) we want to try
other rates without rebuilding: for example a faster DMX tick (lower light-to-beat delay, smoother fades) or a slower one
on a weak machine. A full DMX universe over serial (512 slots, 250 kbaud, break + MAB) takes ~22.7 ms on the wire, so
25 ms is close to the limit for the serial outputs; ArtNet alone can go faster. That trade-off should be a config choice.

`spectrogram_refresh_hz` already shows the pattern (`config.rs:62`, read in `mainloop/mod.rs:187`).

## Scope
- Add two optional keys to `Config` with serde defaults equal to today's values:
  - `dmx_tick_hz` (default 40)
  - `plugin_tick_hz` (default ~83, i.e. 12 ms; or express both as `*_tick_ms` if that reads better. Pick one unit
    for both and note why.)
- Clamp to a sane range and log a warning when clamped (suggestion: DMX 10–100 Hz, plugins 10–250 Hz). Warn when the DMX
  rate is above ~44 Hz while a serial DMX output is configured, because a full universe no longer fits in one frame.
- Read them once at main loop start, next to `spec_refresh_hz`, and use them instead of the constants.
- Remove the duplicate `DMX_TICK_TIME` in `audio/defs.rs`. Replace the UI's use of `mainloop::DMX_TICK_TIME`
  (`ui_egui/src/app/mod.rs:448`, the loop-speed graph's initial scale) with the configured value or a plain constant;
  the graph autoscales anyway.
- Add the keys with their defaults (commented) to `crates/core/config.toml`.
- Old config files without the keys must load unchanged.

## Out of scope
- Scheduling the DMX tick like the spectrogram (`next_tick += period` instead of `elapsed() >= period`). Today the
  DMX tick drifts by the loop's jitter each frame. Worth a follow-up ticket; note it in Notes if you see it matter.
- A UI control for the rates (config file only for now).
- Fixing serial DMX timing at higher rates. Ticket 088 ("break and write ordering") explains why a faster tick can let
  the next frame's break cut off the previous frame on FTDI outputs (`write_all` returns before the data is on the
  wire). Above 40 Hz with serial outputs, watch the last channels of each universe for flicker and note it.
- Real-time thread priority for the main loop and `dmx-output` (separate follow-up).

## Key code
- Constants — `crates/engine/src/mainloop/mod.rs:32-33`
- Tick checks — `crates/engine/src/mainloop/mod.rs:327` (plugins), `:362` (DMX)
- Existing config pattern — `crates/engine/src/config.rs:62`, `:83`, `:370`
- Animations already use elapsed time, not tick counts (`dmx/animation/audio.rs:439`
  `attack_and_release_depend_on_elapsed_time_not_tick_count`); `dmx/animation/mod.rs:2050` has a test that assumes
  25 ms frames. Check that nothing else assumes a fixed 25 ms.

## Acceptance criteria
- [ ] Without the new keys, the engine ticks at 40 Hz / 12 ms exactly as before.
- [ ] With `dmx_tick_hz = 60`, DMX frames go out at ~60 Hz (check the loop-speed / tick stats or ArtNet packet rate).
- [ ] Out-of-range values are clamped with a warning in the log.
- [ ] Animations (fades, chases, audio-reactive) run at the same speed at 20, 40 and 60 Hz.

## Verification
- `cargo clippy --all-targets --all-features`, `cargo fmt`, `cargo test --all-features`.
- Unit test: config with and without the keys deserializes to the expected periods; clamping.
- Run the engine with 20/40/60 Hz and compare a timed chase or fade (and the ArtNet packet rate, e.g. with
  `tcpdump -i <if> udp port 6454`).

## Notes
