# 036 — Programmer in the Console layout

| | |
|---|---|
| Epic | E9 Console UI |
| Depends on | 033, 034, 035 |
| ABI / showfile | none |

**Touches:** `crates/core/src/app/pages/programmer.rs`, `programmer_animations.rs`, `crates/core/src/app/components/fixtures_shared.rs` (`fixture_selection`, `scene_overview`), `fader.rs`, `knob.rs`, `speed_knob.rs`

## Problem
The Programmer is the busiest page: seven equal toolbar buttons with cut labels, a scene column, group cards, horizontal sliders and round knobs. The concept page's direction A mockup and the design system **Blaulicht Console** (https://claude.ai/artifact/ChF4HkbnuyT8bycizagoja) show the target layout (README "Layout patterns" → Programmer).

## Scope
- PageHeader with the focused scene as a picker (scene tree in a sheet or menu) and the actions Animations, Palettes, Scene animations, Limit to fixture, More, LIVE.
- Kiosk: groups tree on the left; selection bar (name, fixture count, Limit, Clear), fixture chips and vertical **Faders** grouped by family in the middle; **Readouts** for scene master and speed plus the scene's changes (with Undo all) on the right.
- Desktop: the same, plus the scene tree as a column on the left.
- Faders: relative drag, tap on the value opens the numberpad sheet, value in `accent` when the scene has a change on that attribute.
- Replace the knobs with Readouts.

## Out of scope
- New programmer features. Behaviour stays as it is; only layout and components change.

## Key code
- `programmer_ui` and the toolbar — `crates/core/src/app/pages/programmer.rs`
- `toolbar_button_width`, `TOOLBAR_BUTTONS` — `programmer.rs:21,1187` (remove)
- Faders and knobs — `crates/core/src/app/components/fader.rs`, `knob.rs`, `speed_knob.rs`

## Acceptance criteria
- [ ] At 800 × 480 nothing is cut or overlapping, and all Programmer functions remain reachable.
- [ ] Scene switching works from the header picker on the kiosk.
- [ ] Fader and readout changes show up in `blctl state` exactly as before.

## Verification
- `cargo clippy --workspace --all-targets --all-features`, `cargo fmt --all --check` and `cargo test --workspace --all-features` are clean (run them in the blenv container).
- Follow the `blaulicht-env` skill: run the app in blenv (`scripts/blenv run` and `run --desktop`), drive it with real clicks, and look at every screenshot yourself. Take before and after shots; keep them in `.blenv/shots/` and never commit them.
- Compare against the design system's component previews and rules (Blaulicht Console, linked under Problem).
- Before and after screenshots of the Programmer at 800 × 480 and on desktop, plus the scene picker open, the numberpad sheet and Undo all.

## Notes
<!-- Implementation notes, decisions, and follow-ups found while working. Keep this updated before renaming to .done.md. -->
