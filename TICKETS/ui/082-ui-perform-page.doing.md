# 082 — Perform page bar and horizontal fader

| | |
|---|---|
| Epic | E13 UI port |
| Depends on | 069 |
| ABI / showfile | none expected; check while implementing |

**Touches:** `app/pages/view_perf.rs`, `app/components/fader.rs` (horizontal)

## Problem
Perform's bar was cramped (`pages.html#performance`). Mockup: `design/studio`; gaps listed in `design/studio/UI_GAP.md`.

## Scope
- Page bar like the Programmer's: Add overlay, Grand master horizontal fader with value and a reset icon (greyed at 100 %), Blackout and Exclusive preview right.
- Horizontal fader: the vertical fader's cap, slot and ticks on its side, ticks above and below; used everywhere a horizontal fader appears (Audio too).

## Out of scope
- Nothing beyond the scope.
- The kiosk layout (`Density::Compact`) stays unchanged.

## Acceptance criteria
- [ ] Matches the mockup at the same window size, both themes.
- [ ] Grand master reset, Blackout and restore work from the bar.

## Verification
- `cargo clippy --all-targets --all-features`, `cargo fmt`, `cargo test --all-features`.
- UI: screenshots per `crates/core/AGENTS.md` next to the mockup: Perform bar, Audio faders.

## Notes
