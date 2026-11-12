# 071 — One dropdown component

| | |
|---|---|
| Epic | E13 UI port |
| Depends on | 058, 069 |
| ABI / showfile | none expected; check while implementing |

**Touches:** new `app/components/dropdown.rs`, every `ComboBox` call site

## Problem
The mockup uses one popover for every choice list (`chrome.js` `BL.dropdown`); egui uses ComboBox. Mockup: `design/studio`; gaps listed in `design/studio/UI_GAP.md`.

## Scope
- Popover on the pane surface with a 1px line, opens below (or above if no room), max 480px tall.
- Filter field from 8 options; arrow keys, Home/End, Enter picks, Esc or outside click closes.
- Groups as folder bands (069); optional meta text right (counts, ids).
- Multi-select variant: checkboxes, an All row, Alt-click selects only that row, list stays open.
- Replace every ComboBox on desktop pages with it. Only the Programmer scene picker dims the page.

## Out of scope
- Kiosk widgets.
- The kiosk layout (`Density::Compact`) stays unchanged.

## Acceptance criteria
- [ ] Matches the mockup at the same window size, both themes.
- [ ] No `ComboBox` left in desktop page code.
- [ ] Keyboard-only use works: open, filter, pick, close.

## Verification
- `cargo clippy --all-targets --all-features`, `cargo fmt`, `cargo test --all-features`.
- UI: screenshots per `crates/core/AGENTS.md` next to the mockup: single-select with filter, multi-select (Logs levels).

## Notes
