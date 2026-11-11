# 078 — Identify panel and Initialize for a selection

| | |
|---|---|
| Epic | E13 UI port |
| Depends on | 077 |
| ABI / showfile | none expected; check while implementing |

**Touches:** `app/pages/setup/fixtures.rs`; engine command for identify if missing

## Problem
Identify does not exist in egui; Initialize works on one fixture (`fxplan.js` Identify). Supersedes the dialog idea in 008. Mockup: `design/studio`; gaps listed in `design/studio/UI_GAP.md`.

## Scope
- Identify and Initialize in their own row under Patch, left-aligned, lit buttons (069).
- Identify drops down a panel from the button: fixture name and "2 of 3", Previous/Next and arrow keys, Transfer to selection (stop and select only that fixture), Stop/Esc/outside click. Only the button blinks.
- Steps through the selection, or through the group when one fixture is selected. The identified fixture's row is red and wins over blue.
- Initialize sends start-up values to every selected fixture.
- Offline: not sent.

## Out of scope
- Nothing beyond the scope.
- The kiosk layout (`Density::Compact`) stays unchanged.

## Acceptance criteria
- [ ] Matches the mockup at the same window size, both themes.
- [ ] Stepping moves the flashing fixture on the rig and the red row together.

## Verification
- `cargo clippy --all-targets --all-features`, `cargo fmt`, `cargo test --all-features`.
- UI: screenshots per `crates/core/AGENTS.md` next to the mockup: panel open with a red row.

## Notes
