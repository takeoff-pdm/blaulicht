# 076 — Programmer colour picker and Add animation

| | |
|---|---|
| Epic | E13 UI port |
| Depends on | 075 |
| ABI / showfile | none expected; check while implementing |

**Touches:** `app/pages/programmer.rs`, `app/pages/programmer_animations.rs`, `app/components/color.rs`

## Problem
The wheel becomes an HSV triangle and Add animation becomes a list grouped by property (`programmer.html` TRI, `show.js` Add animation). Mockup: `design/studio`; gaps listed in `design/studio/UI_GAP.md`.

## Scope
- Colour: hue ring around a fixed (non-rotating) saturation/value triangle, hue right, white top-left, black bottom-left; drag on ring sets hue, inside sets S/V. Draw as an egui mesh with vertex colours.
- Add animation dialog: `AnimationPreset`s grouped by the fader they drive (Intensity, Strobe, Focus, Hue, Value, Pan, Tilt) in folder rows; each row shows the preset and its category; click adds to the selection, click again removes; selected rows blue; folder shows "N on"; properties bound to a palette disabled with a reason. Done closes.

## Out of scope
- Saved templates section (keep as in egui today).
- The kiosk layout (`Density::Compact`) stays unchanged.

## Acceptance criteria
- [ ] Matches the mockup at the same window size, both themes.
- [ ] Clicking Kick Flash then again leaves the animation list unchanged.

## Verification
- `cargo clippy --all-targets --all-features`, `cargo fmt`, `cargo test --all-features`.
- UI: screenshots per `crates/core/AGENTS.md` next to the mockup: colour picker, Add animation dialog with one preset on.

## Notes
