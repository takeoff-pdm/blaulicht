# 035 — Shell: page header with action overflow, nav rail and tab bar

| | |
|---|---|
| Epic | E9 Console UI |
| Depends on | 032, 033 |
| ABI / showfile | none |

**Touches:** `crates/ui_egui/src/app/components/navbar.rs`, `horizontal_nav.rs`, new `components/page_header.rs`, `crates/ui_egui/src/app/ui.rs` (main screen), `crates/ui_egui/src/app/external_screen.rs` (pane chrome)

## Problem
Pages draw their own toolbars of equal-width buttons with cut labels ("Scene Anim…", "UnLmt"), the nav rail shows filled tiles, and sub-page tabs wrap into hidden rows in narrow panes. See PageHeader, NavRail and TabBar in the design system **Blaulicht Console** (https://claude.ai/artifact/ChF4HkbnuyT8bycizagoja).

## Scope
- **PageHeader** component: page name, optional context picker (label, caption, opens a sheet or menu), prioritised actions that overflow into **More** instead of being cut, and status (LIVE). Pages give it data; it decides what fits.
- **NavRail** restyle: Phosphor icons in `muted`, active in `accent` with a 2px bar, status dot slot at the bottom, density widths.
- **TabBar** restyle: bottom tabs on `surface`, active tab with `accent` text and top bar; scroll sideways instead of wrapping.
- Desktop pane chrome: thin `line` borders instead of 2px grey strokes, pane header uses PageHeader.

## Out of scope
- Moving each page's toolbar into the header happens in the page tickets (036–038); this ticket provides the component and converts one simple page (Logs or System) as the example.

## Key code
- Nav rail — `crates/ui_egui/src/app/components/navbar.rs:17-185`
- Sub-page tabs — `crates/ui_egui/src/app/components/horizontal_nav.rs`
- Pane frame — `crates/ui_egui/src/app/external_screen.rs:411-413`

## Acceptance criteria
- [ ] Header actions never truncate: extra actions appear under More.
- [ ] Nav rail and tab bar match the design system at both densities.
- [ ] One page uses PageHeader end to end.

## Verification
- `cargo clippy --workspace --all-targets --all-features`, `cargo fmt --all --check` and `cargo test --workspace --all-features` are clean (run them in the blenv container).
- Follow the `blaulicht-env` skill: run the app in blenv (`scripts/blenv run` and `run --desktop`), drive it with real clicks, and look at every screenshot yourself. Take before and after shots; keep them in `.blenv/shots/` and never commit them.
- Compare against the design system's component previews and rules (Blaulicht Console, linked under Problem).
- Screenshots of the nav rail, a page header with overflowing actions (open More) and the Setup tab bar in a narrow desktop pane.

## Notes
<!-- Implementation notes, decisions, and follow-ups found while working. Keep this updated before renaming to .done.md. -->
