# 075 — Programmer layout, page bar and resizable panes

| | |
|---|---|
| Epic | E13 UI port |
| Depends on | 069, 071, 072 |
| ABI / showfile | none expected; check while implementing |

**Touches:** `app/pages/programmer.rs`, `app/layout.rs`

## Problem
Port the agreed Programmer layout (`programmer.html`, `panels.js`, `scenepicker.js`, `RULES.md` decision 1). Mockup: `design/studio`; gaps listed in `design/studio/UI_GAP.md`.

## Scope
- Rack at 1400px and wider: Groups, Fixtures, faders 440x320, wheel and pad in one row; Palettes and the Scene pane below. Stack below 1400px; sidebar collapses below 1200px.
- Page bar 40px: scene picker (filterable folder list over the page dimmed 55%) with the scene's intensity and speed knobs, Blackout and Exclusive preview (lit button) right, Reset layout.
- Dividers between lists, Palettes and the Scene pane (8px grab, double-click restores), every pane folds to its title; sizes saved per machine, separately for rack and stack. Faders, wheel and pad keep their size.
- Backspace/Delete on the selected fader undoes this session's change for the selection, then removes earlier scene changes; no confirmation.

## Out of scope
- Colour picker and Add animation (076).
- The kiosk layout (`Density::Compact`) stays unchanged.

## Acceptance criteria
- [ ] Matches the mockup at the same window size, both themes.
- [ ] Layout matches `programmer.html` at 1440 and 1280 wide.
- [ ] Pane sizes survive a restart.

## Verification
- `cargo clippy --all-targets --all-features`, `cargo fmt`, `cargo test --all-features`.
- UI: screenshots per `crates/core/AGENTS.md` next to the mockup: 1440x900 and 1280x800, picker open.

## Notes
