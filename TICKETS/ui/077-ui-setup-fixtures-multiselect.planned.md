# 077 — Setup Fixtures: multi-select and Move to group

| | |
|---|---|
| Epic | E13 UI port |
| Depends on | 069, 071, 073 |
| ABI / showfile | none expected; check while implementing |

**Touches:** `app/pages/setup/fixtures.rs`

## Problem
The mockup selects several fixtures and moves them between groups (`setup.html` fixtures, `treedrag.js`). Mockup: `design/studio`; gaps listed in `design/studio/UI_GAP.md`.

## Scope
- Fixture table: click selects one, Ctrl/Cmd+click toggles, Shift+click range.
- Several selected: pane title "Fixtures: N selected", type or "Mixed", universe, start channel, channels, position and rotation frozen as `--`; Identify and Initialize stay.
- Move to group: header button opens the group dropdown (071); dragging fixture rows onto a group in the tree moves them (a selected row carries the selection). Notification, no undo.
- Delete names the count; separated from Move by a divider.

## Out of scope
- Identify (078), position view (079).
- The kiosk layout (`Density::Compact`) stays unchanged.

## Acceptance criteria
- [ ] Matches the mockup at the same window size, both themes.
- [ ] Three fixtures moved by drag land in the target group and the patch is unchanged.

## Verification
- `cargo clippy --all-targets --all-features`, `cargo fmt`, `cargo test --all-features`.
- UI: screenshots per `crates/core/AGENTS.md` next to the mockup: three selected, drag onto a group.

## Notes
