# UI gap: studio mockups vs. the egui app

Behaviour the `design/studio` mockups have that the real desktop UI (`crates/ui_egui`) does not, so it isn't lost in the port.
Recorded 2026-11-11. "Missing" is from a keyword search of `crates/ui_egui/src`, not a full audit; "Partial" means something related exists.
The written rules are in `RULES.md`; this file lists the work.

## Shared components

| Feature | Mockup | egui |
|---|---|---|
| **Notifications**: stacked cards bottom right, max 4, info 5 s (8 s with action), errors sticky, hover pauses, repeats count up (×2), no Undo | `chrome.js` `BL.toast` | Missing (no notification system found) |
| **Dropdown**: one popover for every choice list; filter from 8 options, folder groups, arrow keys/Enter/Esc, multi-select with checks, All row, Alt-click for "only"; native selects open it | `chrome.js` `BL.dropdown` | Missing (egui ComboBox only) |
| **Type a value in place**: double-click a fader/knob value or name, Enter/blur applies, Esc cancels, clamped, no layout shift | `chrome.js` `BL.editValue` | Missing (no double-click handling) |
| **Keyboard on faders/knobs**: click/Tab focuses (blue label), arrows move by the smallest unit, held keys accelerate after 300 ms and cap at range/25 | `chrome.js` `BL.keySteps`, `show.js` | Missing (no arrow-key handling) |
| **Scrub number fields**: drag sideways on any number field, step from `data-step`, Shift = 10× finer, Esc restores, applies live | `chrome.js` | Partial (some `DragValue`s) |
| **Lit on/off buttons with a dot**: grey off, blinking red on, fixed text and width (Exclusive preview, Identify, Initialize) | `.livebtn` in `studio.css` | Partial (Exclusive preview exists, no dot style) |
| **Folder trees**: header grey bands with a 1px top line; drag items between folders in Setup lists (not in live Programmer lists) | `treedrag.js` | Missing (no drag and drop) |
| **Horizontal fader** styled as the vertical fader on its side (same cap, slot, ticks both sides) | `pages.css` `.hf*` | Check against egui fader |
| **40px top bars** with 28px buttons on page bars and master-detail top panes; title prefixes ("Selected group:", "Fixture:") | `studio.css` | Missing |

## Status bar

- One **issues cell** ("4 issues", error and warning icons, tooltip split, opens System → General) replaces the separate "DMX 1 offline" cell. Mockup: `chrome.js`. egui: missing.

## Programmer

- Resizable and foldable panes with saved sizes, Reset layout (`panels.js`). egui: missing.
- Scene picker in the page bar over a 55% dimmed page, with the scene's intensity/speed knobs beside it (`scenepicker.js`).
- HSV colour picker: fixed triangle inside a hue ring (`programmer.html` `TRI`). egui: missing.
- Backspace/Delete on the selected fader undoes this session's change for it, then removes earlier scene changes (`show.js` `undoFader`).
- Add animation: presets from `AnimationPreset`, grouped by fader property in the folder list, click to add/remove on the selection; palette-bound properties disabled. egui has a preset grid by category (`programmer_animations.rs`).
- Exclusive preview button with dot, constant width; Blackout same height.

## Setup

- **Fixtures, multi-select**: click / Ctrl-Cmd toggle / Shift range; "N selected" pane with per-fixture fields frozen as `--`.
- **Move to group**: header button (group dropdown) and dragging fixture rows onto a group in the tree.
- **Identify**: drop-down panel from the button, Previous/Next and ←/→, "Transfer to selection", Stop/Esc; red row for the fixture flashing. egui: no Identify found.
- **Initialize** applies to the whole selection (egui has a single-fixture Initialize button).
- **Fixture position view**: 2D Top/Front plot of all fixtures, selected blue, group brighter; click a dot to select, drag to move (5 cm snap), wheel/± zoom, pan, Fit, 1 m/5 m grid, larger view in a dialog (`fxplan.js`).
- Position fields captioned Across (X) / Height (Y) / Depth (Z) in m; rotation X/Y/Z in degrees.
- Scenes and Groups detail panes have no Folder field (folder is changed by dragging in the tree); Scenes detail drops master intensity/speed.

## Live pages

- **Views** (renamed from Scene Stacks): views list left with delete on the selected row; the view's name is the editable title (pencil); its scenes in folder groups with tall rows (name, "N in stack", Intensity and Speed knobs, Remove right-aligned, red on hover); Add view asks only for a name; Add overlay uses the dropdown.
- **Perform**: page bar as on the Programmer (40px, Grand master fader, reset icon, Blackout and Exclusive preview right).
- **Logs**: title in the filter bar; level filter as a multi-select dropdown with counts; equal-width source tags.

## System

- **General**: two columns: Show then Problems (with a one-line patch summary and Open patch map) on the left; Engine, About, Application on the right, Shut down separated with its explanation.
- **Screens**: list rows say who controls each screen: link icon and plugin name highlighted, or "Desk window", plus size.

## Not built in either

- Undo (the mockups show none, by decision).
- Stack reordering on Views.
- Screen ownership in the selected screen's pane (sheet `screens-variants.html` section B, not picked yet).

## Built but not verified in the mockup

- Dragging palettes between folders; Shift and Esc while scrubbing number fields; the light theme for most of the above.
