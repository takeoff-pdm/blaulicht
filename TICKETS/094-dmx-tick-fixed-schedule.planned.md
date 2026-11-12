# 094 — Schedule the DMX tick on a fixed beat instead of `elapsed() >= period`

| | |
|---|---|
| Epic | Ops / show host (latency) |
| Depends on | none (touches the same code as 089 and 092) |
| ABI / showfile | none |

**Touches:** `crates/engine/src/mainloop/mod.rs`

## Problem
The DMX tick runs when `dmx_time_of_last_tick.elapsed() >= DMX_TICK_TIME` and then sets
`dmx_time_of_last_tick = Instant::now()` (`crates/engine/src/mainloop/mod.rs:362-365`). Every frame is therefore late by
the loop's granularity (up to ~1 ms sleep + work), and the lateness **accumulates**: the real rate is below 40 Hz and
the frame interval jitters. The plugin tick (`:327`) works the same way.

The spectrogram tick right below already does it correctly (`next_spectrogram_tick += spec_period`, resync after a
long stall).

## Scope
- Use the spectrogram pattern for the DMX tick (and the plugin tick): `next_dmx_tick += period`; if more than ~3
  periods behind, resync to `now + period` instead of bursting catch-up frames.
- Animations use elapsed time, so a steadier tick only changes timing quality, not speed.

## Out of scope
- Configurable rates (089), sleeping until the next deadline (092).

## Acceptance criteria
- [ ] Measured DMX tick rate is 40.0 Hz on average (not below), and the interval jitter is no larger than before
      (use 091's frame stats if available, otherwise a temporary log of intervals).
- [ ] After a long stall (e.g. showfile load) the tick resumes without a burst of frames.

## Verification
- `cargo clippy --all-targets --all-features`, `cargo fmt`, `cargo test --all-features`.
- Unit test for the scheduling helper (on time, slightly late, stalled).

## Notes
