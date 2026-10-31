# 037 — Setup tabs as master–detail in the Console style

| | |
|---|---|
| Epic | E9 Console UI |
| Depends on | 033, 034, 035 |
| ABI / showfile | none |

**Touches:** `crates/ui_egui/src/app/pages/setup/{fixtures,groups,scenes,animations,palettes}.rs`, `crates/ui_egui/src/app/pages/animation_spec_editor.rs`

## Problem
Each Setup tab has its own toolbar, button sizes and dialog style, and long forms run under the tab bar at 800 × 480. The design system **Blaulicht Console** (https://claude.ai/artifact/ChF4HkbnuyT8bycizagoja) defines the master–detail pattern (README "Layout patterns").

## Scope
- Every Setup tab: list or tree on the left, details of the selection on the right, actions in the PageHeader, row actions on long-press or right-click.
- Fixtures: the fixture editor as a scrollable detail form using standard fields.
- Animations: the template editor as a scrollable detail form; the starter presets as a sheet.
- Palettes: palette details with the editor; the pointer-palette target picker as a tree picker.
- Replace abbreviations with full labels ("Rename scene", "Clone scene").

## Out of scope
- Programmer (036) and other pages (038).

## Key code
- Setup pages — `crates/ui_egui/src/app/pages/setup/`
- Spec editor — `crates/ui_egui/src/app/pages/animation_spec_editor.rs`

## Acceptance criteria
- [ ] All five tabs follow master–detail at both densities, with no content under the tab bar at 800 × 480.
- [ ] Every action that existed before is still reachable.

## Verification
- `cargo clippy --workspace --all-targets --all-features`, `cargo fmt --all --check` and `cargo test --workspace --all-features` are clean (run them in the blenv container).
- Follow the `blaulicht-env` skill: run the app in blenv (`scripts/blenv run` and `run --desktop`), drive it with real clicks, and look at every screenshot yourself. Take before and after shots; keep them in `.blenv/shots/` and never commit them.
- Compare against the design system's component previews and rules (Blaulicht Console, linked under Problem).
- Before and after screenshots of each Setup tab at 800 × 480 and on desktop.

## Notes
<!-- Implementation notes, decisions, and follow-ups found while working. Keep this updated before renaming to .done.md. -->
