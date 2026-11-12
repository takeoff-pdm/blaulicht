# 091 — Real-time priority for `dmx-output` and the main loop, with frame-interval stats

| | |
|---|---|
| Epic | Ops / show host (latency) |
| Depends on | none |
| ABI / showfile | none (optional `config.toml` keys) |

**Touches:** `crates/engine/src/util.rs`, `crates/engine/src/mainloop/mod.rs`, `crates/engine/src/dmx/mod.rs`,
`crates/engine/src/config.rs`, the tick-speed message (`msg::DmxTickSpeeds`/`TickSpeeds`) and its UI graph

## Problem
Measured on the show host `bl` (2026-11-12, `/proc/<pid>/task/*/stat`): every Blaulicht thread runs with
`SCHED_OTHER` (normal). `util::increase_thread_priority()` (`crates/engine/src/util.rs:5`) calls
`thread_priority::set_current_thread_priority(ThreadPriority::Max)`, which under `SCHED_OTHER` only changes the nice
value. The real-time path (`realtime()`, via `audio_thread_priority`) exists but is commented out. The `dmx-output`
thread (`crates/engine/src/dmx/mod.rs:371`) never raises its priority at all.

`limits.conf` already allows `rtprio 99` for the `blaulicht` user. A `cyclictest` run on `bl` at FIFO priority 90 had a
worst case of ~0.17 ms under load with full preemption; normal-priority threads can do much worse under contention.

The threads also move between all 4 CPUs (seen on CPUs 0–3 across four 30 s samples), sometimes landing on the core
that takes USB (`xhci`, CPU1) or graphics (`i915`, CPU3) interrupts.

Average waits are tiny today (~1 µs per wakeup for `dmx-output`), but averages can't show the rare spikes that cause
visible stutter. We need the engine to measure its own frame timing.

## Scope
1. **SCHED_FIFO** for `dmx-output` (suggest priority 80) and the main loop (suggest 70), via `thread_priority`
   (`set_thread_priority_and_policy` with `RealtimeThreadSchedulePolicy::Fifo`) or `libc::sched_setscheduler`.
   Fall back to today's behaviour with a warning if it fails (no rtprio limit, macOS, container). Linux only;
   keep macOS/wasm unchanged.
2. **Frame-interval stats** in `dmx-output`: time between consecutive frame sends and the time a frame's send takes
   (break + MAB + write, per universe). Keep min/avg/max and a count of late frames (interval > 1.5× tick) over a
   rolling window; publish them with the existing tick speeds and show them in the loop-speed graph/System page. Same
   for the main loop's DMX tick interval.
3. **Optional CPU affinity** via `config.toml`: `dmx_output_cpu` and `mainloop_cpu` (unset = no pinning). Pin with
   `core_affinity` or `sched_setaffinity`. On `bl` the intended layout is `dmx-output` on CPU1 (the USB interrupt CPU,
   set by `install/tune.sh`), main loop on CPU3 (CPU1's hyperthread sibling).
4. Optional config keys for the priorities (`dmx_output_rt_priority`, `mainloop_rt_priority`, 0 = off).

## Out of scope
- Why the main loop uses ~50 % of a CPU (ticket 092). A FIFO main loop that busy-spins can starve normal threads
  on its CPU (UI, ksoftirqd). **Check 092's state first**; if the loop still spins, keep the main loop at normal
  priority and only make `dmx-output` FIFO (it sleeps between frames, ~1.5 % CPU).
- `mlockall` (ticket 095).

## Acceptance criteria
- [ ] On `bl`, `dmx-output` shows policy FIFO (`ps -eLo comm,cls,rtprio | grep dmx`), the main loop too if 092 allows.
- [ ] Failing to get RT priority logs a warning and the app runs as before.
- [ ] Frame-interval min/avg/max and late-frame count are visible in the UI and the log.
- [ ] Before/after numbers from `bl` (idle and with MIDI/UI load) in Notes.

## Verification
- `cargo clippy --all-targets --all-features`, `cargo fmt`, `cargo test --all-features`.
- On `bl`: thread policies with `ps -eLo comm,cls,rtprio,psr`; frame stats over 10 minutes of a running show.

## Notes
