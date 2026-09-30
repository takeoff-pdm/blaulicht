# 023 — Grand master control redesign

| | |
|---|---|
| Epic | E6 Performance |
| Depends on | 022 |
| ABI / showfile | none |

**Touches:** `crates/core/src/app/pages/view_perf.rs (or its successor from 022)`

## Problem
Original review notes (2026-09-27):

> also the grand master is ugly

## Scope
- Replace the plain `egui::Slider` plus "100%"/"BLACKOUT" buttons (`view_perf.rs:258-282`) with a proper fader: a tall vertical control on the right edge of Performance, a large percent readout, a BLACKOUT toggle styled as a danger button showing an active state, and a 100% reset.
- It must be usable by touch at 800x480 and keep its placement consistent across Performance sub-pages.

## Out of scope


## Key code
- `set_grand_master_percent` — `crates/core/src/app/pages/view_perf.rs:258-282`

## Acceptance criteria
- [ ] Visually distinct fader; blackout state is unmistakable; value changes are reflected in DMX output.

## Verification
- `cargo clippy --all-targets --all-features` and `cargo fmt` are clean.
- `cargo test --all-features` passes, and new logic has co-located `#[cfg(test)]` tests.
- Follow **UI Verification** in `crates/core/AGENTS.md` (check that `swaylock` is not running, launch from the repo root, navigate via blctl, capture with grim). Use the headless Xvfb recipe when clicks are needed or another session is running (see memory notes). Take before and after screenshots, keep them in the scratchpad, and never commit them.
- Screenshot the Performance page at 100%, 40% and blackout.

## Notes
<!-- Implementation notes, decisions, and follow-ups found while working. Keep this updated before renaming to .done.md. -->
