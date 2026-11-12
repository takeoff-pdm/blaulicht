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
- [x] Matches the mockup at the same window size, both themes.
- [x] Clicking Kick Flash then again leaves the animation list unchanged.

## Verification
- `cargo clippy --all-targets --all-features`, `cargo fmt`, `cargo test --all-features`.
- UI: screenshots per `crates/core/AGENTS.md` next to the mockup: colour picker, Add animation dialog with one preset on.

## Notes
- Colour: `components::hsv_triangle` (color.rs) draws the hue ring and the fixed triangle (hue right, white top
  left, black bottom left) as egui meshes with vertex colours, same geometry as the mockup's `TRI` (176px, ring
  88/72, triangle 68). The drag keeps to where it started (ring: hue, inside: S/V); black keeps its saturation.
  The ring or triangle is disabled while hue or sat/value are palette-bound. The swatch is now a plain preview
  (no egui colour popup), the R/G/B fields sit right under the wheel as in the mockup.
- Add animation (desktop only, `Density::Compact`): own 440px modal with folder bands per fader (Intensity,
  Strobe, Focus, Hue, Value, Pan, Tilt), rows `ListRow` with the category as a new `note`; folder shows "N on"
  (mono), the count when collapsed, a link icon and disabled rows when the property is palette-bound. Done or
  Esc closes. Toggling goes through the existing `HostCommand::ToggleAnimationPreset`.
- Saved templates kept as before (one row each, toggling the template), in a "Saved" folder after the presets so
  the top of the list matches the mockup. The kiosk dialog's "On selection" list and "Save as preset" stay
  kiosk-only; the desktop has the Animations pane for active animations (no desktop Save-as-template yet:
  follow-up).
- `studio::folder_band_with` generalises `folder_band` (free right text, optional icon); `folder_band` callers
  draw the same.
- Verified in blenv `--desktop` with SPARTACUS_SOLI.json next to mockup shots (dark and light): colour pane
  (ring click set hue 58, triangle drag set S/V, RGB followed), dialog with Kick Flash on (1 on, blue row,
  Animations pane 1), second click removed it (Animations 0), Done and Esc close. Kiosk Programmer page and its
  Animations dialog screenshots are byte-identical to decouple.
- Checks ran inside blenv: rustfmt on the touched files (the workspace `cargo fmt` also reformats unrelated
  pre-existing files, which were reverted), `cargo clippy -p blaulicht-ui-egui --all-targets --all-features`
  (no warnings in touched files), `cargo test -p blaulicht-ui-egui --all-features` (162 passed). Not run: the
  full-workspace test suite (laptop memory). Not verified: palette-bound disabled rows/link icon on screen
  (needs a palette-bound selection), ring edge antialiasing (mesh edges are not feathered, looks fine at 1x).

