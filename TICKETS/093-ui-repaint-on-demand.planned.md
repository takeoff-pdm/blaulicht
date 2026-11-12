# 093 — UI repaints at a fixed 60 fps even when nothing changes

| | |
|---|---|
| Epic | Ops / show host (performance) |
| Depends on | none |
| ABI / showfile | none |

**Touches:** `crates/ui_egui/src/app/ui.rs`, pages and components with live data

## Problem
`crates/ui_egui/src/app/ui.rs:103` calls `ctx.request_repaint_after(Duration::from_millis(16)); // ~60 FPS` every
frame, so egui redraws ~60×/s regardless of input. On `bl` (2026-11-12) graphics interrupts stayed at 74–95/s and the
UI thread at 10–13 % of a CPU whether idle or spammed with page switches. Only fader input raised it (22 %). The cost
is constant, on a 2-core machine shared with the engine, and every frame is rendered with 4× MSAA (ticket 090).

## Scope
- Remove the global 16 ms repaint, or make it a lower fallback (e.g. 250 ms) for "something might have changed".
- Let pages/components with live data request their own repaints at the rate they need: meters, spectrogram,
  visualizer, tick graphs, beat indicators, live DMX values, notifications (some already do).
- Repaint when the engine pushes state that the visible page shows (hub/client message arrives →
  `ctx.request_repaint()`).
- Optional: a `config.toml` key `ui_max_fps` (default 60) to cap the rate on weak machines.

## Out of scope
- MSAA (ticket 090).

## Acceptance criteria
- [ ] Idle on a static page (e.g. Setup), the UI repaints rarely (check with a frame counter or `i915` interrupt rate
      on `bl`).
- [ ] Every live element still updates smoothly: meters, spectrogram, visualizer, fader feedback from MIDI, beat
      indicators, tick graphs, notifications, connection notice. Go through each page.
- [ ] Before/after UI thread CPU on `bl` in Notes.

## Verification
- `cargo clippy --all-targets --all-features`, `cargo fmt`, `cargo test --all-features`.
- `scripts/kt` / blenv screenshots of live pages; on `bl`, per-thread CPU and `/proc/interrupts` (`i915`).

## Notes
