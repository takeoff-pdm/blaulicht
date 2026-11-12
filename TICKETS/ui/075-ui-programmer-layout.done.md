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
- [x] Matches the mockup at the same window size, both themes.
- [x] Layout matches `programmer.html` at 1440 and 1280 wide.
- [x] Pane sizes survive a restart.

## Verification
- `cargo clippy --all-targets --all-features`, `cargo fmt`, `cargo test --all-features`.
- UI: screenshots per `crates/core/AGENTS.md` next to the mockup: 1440x900 and 1280x800, picker open.

## Notes

- Desktop only: `programmer_ui` returns into `pages/programmer_desktop.rs` on `Density::Compact` (desktop); the kiosk (`Density::Touch`) path is untouched. Kiosk Programmer screenshot (800x480, SPARTACUS_SOLI) is pixel-identical to `decouple` (ImageMagick AE = 0).
- Layout geometry, folding, clamps and persistence live in `pages/programmer_layout.rs` (`app/layout.rs` did not exist), unit-tested at 1440x900 rack and 1280x800 stack. Rack from 1400px window width; the sidebar is forced to its rail on the Programmer below 1200px (`Navbar::set_force_rail`, the saved choice is kept).
- Sizes are saved per machine in `<config dir>/programmer-layout.json` (rack and stack separately). Verified in blenv: fold and divider sizes survived an app restart. The web UI has no config dir, so it keeps the layout for the session only (follow-up if wanted: localStorage).
- Dividers: Groups|Fixtures, Palettes|Scene (rack), Scene pane edge (stack); 8px grab, blue line on hover/drag, double-click restores, Esc cancels a drag. Reset layout restores both modes.
- Page bar: scene picker uses the shared dropdown popover with `dim` (55%); its scrim moved to `Order::Middle` because a second Foreground layer sometimes drew over the list. The picker button repaints above the scrim while open. `KnobSize::Small` is now 32px (`.prog #masters`), used only here and in the showcase.
- Backspace/Delete on the picked (clicked/focused) fader: undoes this session's change for the selection (restores the value the scene had when this session first showed it, or drops a change the session added), else removes the scene's earlier changes; notification says which. Tested with real keys.
- Fader values read the edited scene's state of the first selected fixture, so they follow the scene picker (checked WHITE -> STATIC).
- Limit / Limit all are placeholders (RULES.md). Removing more than one Scene change or an animation asks first.
- Not done here / not verified: the HSV wheel (076; the Colour pane has a swatch with egui's picker plus R/G/B fields meanwhile), Add animation dialog restyle (076), faders looking disabled without selection (087). Light theme checked by screenshot only on the Programmer; the scene-picker popover's folder bands use the shared dropdown style (no caret), slightly different from the mockup.
- Checks: `cargo fmt` (blenv; host has no rustfmt), `cargo clippy --all-targets --all-features` (blenv, no new warnings in touched files), `cargo test --all-features` (blenv, all pass).
