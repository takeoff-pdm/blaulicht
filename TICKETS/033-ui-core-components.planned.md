# 033 — Core components: Button variants, ListRow, Chip, Status

| | |
|---|---|
| Epic | E9 Console UI |
| Depends on | 032 |
| ABI / showfile | none |

**Touches:** `crates/core/src/app/components/button.rs`, `button_new.rs`, `clickable.rs`, `tree.rs`, `folder_tree.rs`, `fixtures_shared.rs` (chips and cards), `navbar.rs` (status slot), new component files as needed

## Problem
Everything is drawn by one painted button style (`components::button`, 191 call sites) with no variants, so Save and Delete look the same and every row is a filled box. The design system **Blaulicht Console** (https://claude.ai/artifact/ChF4HkbnuyT8bycizagoja) defines Button, ListRow, Chip and Status; read their `components/<Name>/README.md` and previews.

## Scope
- **Button**: `primary`, `tonal`, `quiet`, `danger` variants and disabled-with-reason, sized from the density, sizing to the label (no fixed `ButtonSize` widths). Keep a thin compatibility layer so existing call sites compile, mapping today's `active` flag to the selection look and plain buttons to tonal.
- **ListRow**: flat rows on `bg` with `line` hairlines, indent guides, twisty, folder icon, label and meta; the selected look (`selection` fill, `accent` label, 2px bar); desktop hover with a "⋯" button. Restyle `tree.rs` rows with it and remove the per-row "Move…" button (row actions arrive in 034; until then keep Move reachable from the header or toolbar, and note it).
- **Column header**: overline label plus one quiet action.
- **Chip** for fixtures in a selection, with a colour swatch.
- **Status**: the LIVE pill and status dots with a word; use the dot in the nav rail's status slot.

## Out of scope
- Sheets, menus and toasts (034). Header, nav rail and tab bar (035). Page layouts (036–038).

## Key code
- `button`, `ButtonSize`, `paint_button_label` — `crates/core/src/app/components/button.rs:105-178`
- Tree rows — `crates/core/src/app/components/tree.rs:436-628`
- Group and fixture cards — `crates/core/src/app/components/fixtures_shared.rs:122-192`

## Acceptance criteria
- [ ] All four button variants exist and are used for at least Store/Cancel/Delete in the folder dialogs.
- [ ] Trees and lists render as flat rows with the selected look; no row has its own permanent button.
- [ ] No new colour or size literals: components read tokens and density only.

## Verification
- `cargo clippy --workspace --all-targets --all-features`, `cargo fmt --all --check` and `cargo test --workspace --all-features` are clean (run them in the blenv container).
- Follow the `blaulicht-env` skill: run the app in blenv (`scripts/blenv run` and `run --desktop`), drive it with real clicks, and look at every screenshot yourself. Take before and after shots; keep them in `.blenv/shots/` and never commit them.
- Compare against the design system's component previews and rules (Blaulicht Console, linked under Problem).
- Screenshots of Setup › Groups, the Programmer group tree and a folder dialog in both densities.

## Notes
<!-- Implementation notes, decisions, and follow-ups found while working. Keep this updated before renaming to .done.md. -->
