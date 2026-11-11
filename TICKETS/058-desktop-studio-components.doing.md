# 058 — Desktop studio components and list style

| | |
|---|---|
| Epic | E9 Console UI |
| Depends on | none (builds on 056) |
| ABI / showfile | none |

**Touches:** `crates/ui_egui/src/app/ui_tokens.rs`, `app/components/` (fader, knob, tree rows, dialog, toggles, toolbar)

## Problem
The studio mockups use one set of tokens and controls, but the egui desktop layout does not have them yet. Every page ticket needs these first.

## Scope
- Add tokens from `design/studio/studio.css`: recessed list, folder band, hairline, changed amber, graph data, readable red text, dimmed selection.
- List and tree rows: folder band with icon and tree line, hairlines, single-line truncation with a tooltip, faint IDs, number-only counts.
- Selection: active row is solid blue with a left edge; included rows have a thin outline and no fill.
- One fader, one knob, one horizontal fader, one switch and one segmented control, applying the control table in `RULES.md`.
- One page bar: view tabs left, page actions right, 30px high.
- Modal dialogs: focus stays inside, one at a time, Cancel is default, Esc closes.
- Minimum target 24px, 28px for controls used during a show; minimum text 11px; 4.5:1 contrast.

## Out of scope
- Page layouts (036, 037, 038, 062). The kiosk: all of this applies only at `Density::Compact`.

## Acceptance criteria
- [ ] A showcase or the first ported page shows every component in both themes.
- [ ] A 40-character name truncates on one line and never overlaps the next row.
- [ ] Tab cannot leave an open dialog, and a second dialog cannot open.
- [ ] Kiosk screenshots are unchanged.

## Verification
- `cargo clippy --all-targets --all-features`, `cargo fmt`, `cargo test --all-features`.
- UI: run with `--desktop` via `scripts/blenv` and compare screenshots against the mockup, in dark and light theme, at 1280x800 and 2560x1440. Confirm the 800x480 kiosk is unchanged.

## Notes
Reference: open the named pages in `design/studio/` in a browser. `design/studio/RULES.md` holds the agreed rules. The mockups are the design source, but their JavaScript is throwaway.
