# 092 — Main loop uses ~50 % of a CPU while idle

| | |
|---|---|
| Epic | Ops / show host (performance) |
| Depends on | none |
| ABI / showfile | none |

**Touches:** `crates/engine/src/mainloop/mod.rs`, possibly `crates/audio_engine/src/collector.rs`

## Problem
On the show host `bl` (i5-4310M, 2 real cores + HT) the engine's main thread (`blaulicht`, policy normal) used
48.8–50.8 % of a CPU in every 30 s sample on 2026-11-12: idle, MIDI input, UI spam and beat model on. It wakes ~730×/s.
On 2 real cores this is the biggest single cost and competes with the UI, audio and `dmx-output`.

The loop ends with `spin_sleep::sleep(Duration::from_millis(1))` (`crates/engine/src/mainloop/mod.rs:419`).
`spin_sleep` sleeps natively and then **busy-spins** the last part to hit the deadline exactly. The spin share depends
on the native accuracy it assumes (and on the kernel timer slack). So part of the 50 % is spinning by design, part is
the per-iteration work (audio collector tick, plugin/MIDI/serial/UDP ticks every 12 ms, DMX tick every 25 ms,
spectrogram at 60 Hz, system messages).

## Scope
1. **Measure first**: split the per-iteration time into work vs. sleep/spin (e.g. `perf top -t <tid>` on `bl`, or
   timing counters in the loop). Record the numbers in Notes.
2. Replace the spin: plain `std::thread::sleep` (with `timer_slack` lowered via `prctl(PR_SET_TIMERSLACK, 1)` if needed),
   or a `spin_sleep::SpinSleeper` with a small spin, or — better — sleep until the **next due deadline** (min of the next
   DMX tick, plugin tick, spectrogram tick, audio frame) instead of a fixed 1 ms.
3. Keep the comment's requirement: predicted-beat scheduling must stay sub-frame accurate (check the beat path and
   what accuracy it really needs).
4. If the work itself is significant, note the hot spots and open follow-up tickets.

## Out of scope
- Real-time priority (ticket 091). Note: a FIFO thread that busy-spins can starve normal threads; this ticket should
  land before or with 091's main-loop part.
- DMX tick drift (ticket 094), though "sleep until next deadline" touches the same code; coordinate.

## Acceptance criteria
- [ ] Main loop CPU on `bl` while idle is well below today's ~50 % (target < 15 %), with the numbers in Notes.
- [ ] DMX tick rate, plugin tick rate, spectrogram rate and beat timing unchanged (tick stats, a beat-synced chase).

## Verification
- `cargo clippy --all-targets --all-features`, `cargo fmt`, `cargo test --all-features`.
- On `bl`: per-thread CPU from `/proc/<pid>/task/<tid>/schedstat` deltas (sampler from 2026-11-12) or `top -H`.

## Notes
