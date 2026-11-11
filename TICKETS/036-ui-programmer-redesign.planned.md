# 036 — Programmer in the desktop studio layout

| | |
|---|---|
| Epic | E9 Console UI |
| Depends on | 058, 059 |
| ABI / showfile | none |

**Touches:** `app/pages/programmer.rs`, `programmer_animations.rs`, `app/components/fixtures_shared.rs`

## Problem
The desktop Programmer should follow the agreed studio design.

## Scope
- Layout as `programmer.html`: Groups, Fixtures, then faders (440x320), colour picker (hue ring around a fixed triangle (pure hue right, white top left, black bottom left; it does not rotate), painted as one egui mesh with vertex colours) with editable RGB and XY pan and tilt pad in one row, with Palettes and the Scene pane (animations, Scene changes) below.
- Below 1400px wide, wheel and pad stack under the faders, with Palettes under them and the Scene pane at full height. Below 1200px wide, the sidebar also collapses to icons.
- The page bar holds the scene picker (a filterable popover of scene folders on pane surfaces, opening over the page dimmed by 55%), the scene's master intensity and speed knobs, Blackout, Reset layout and Exclusive preview, all the same height; Exclusive preview shows a blinking red dot while on. Panes resize and fold as in `RULES.md` > Resizable panes. Clear, Limit and Limit all sit in Fixtures, and Add sits in Animations.
- Faders, wheel and pad keep their size and never fold on the rack (1400px and up).
- Clicking, dragging or tabbing to a fader selects it and gives it focus, shown by a blue label (no border); arrow keys move a focused fader or knob by its smallest unit (1, one degree on Hue, 1% on intensity, one speed notch); held keys accelerate after 300 ms up to a cap of a 25th of the range per repeat, reached after about 2 s. Backspace or Delete undoes its changes from this session for the selected fixtures and removes changes this session added from Scene changes; with nothing to undo, it removes the scene's earlier changes for that value. Needs the scene state at load (or last save) kept in the UI or engine.
- Double-clicking a fader's value or name, or a knob, turns its readout into a text field in place (egui `TextEdit` over the label). Enter or focus loss applies, Esc cancels, values clamp to range in the control's units.
- Fixture filter, with a notice when it hides selected fixtures. Clear selection also clears it.
- RGB applies on Enter or leaving the field; Esc cancels. Esc cancels a drag. No selection shows `--`.
- Changed values are amber. Labels: Intensity, Exclusive preview.
- Clear all, removing an animation, and removing a Scene changes group ask first, naming what is lost. Removing a single row does not. Removals that affect dependent pointer palettes name them.
- Limit and Limit all stay as visible dummy buttons, with no action, until their behaviour is decided.

## Out of scope
- The kiosk. Undo. New engine features.

## Acceptance criteria
- [ ] No clipping from 1100px wide; the page scrolls below that.
- [ ] 500 fixtures, 50 palettes and 40-character names remain usable.
- [ ] Typing an RGB value sends only the final value.
- [ ] Clear all does nothing until confirmed, and the dialog shows the number of changes.
- [ ] Fader, wheel, pad and RGB edits appear in Scene changes.

## Verification
- `cargo clippy --all-targets --all-features`, `cargo fmt`, `cargo test --all-features`.
- UI: run with `--desktop` via `scripts/blenv` and compare screenshots against the mockup, in dark and light theme, at 1280x800 and 2560x1440. Confirm the 800x480 kiosk is unchanged.

## Notes
Reference: open the named pages in `design/studio/` in a browser. `design/studio/RULES.md` holds the agreed rules. The mockups are the design source, but their JavaScript is throwaway.
