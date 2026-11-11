# 070 — Stacked notifications

| | |
|---|---|
| Epic | E13 UI port |
| Depends on | 058 |
| ABI / showfile | none expected; check while implementing |

**Touches:** new `app/components/notifications.rs`, `app/ui.rs`, callers of current status messages

## Problem
Feedback is ad hoc; the mockup settled a JetBrains-like stack (`chrome.js` `BL.toast`, `RULES.md` Notifications). Mockup: `design/studio`; gaps listed in `design/studio/UI_GAP.md`.

## Scope
- Cards bottom right above the status bar, newest at the bottom, max 4 (oldest closes).
- Card: type icon and left edge (blue info, red error), message, optional action as a link, close button (always visible on errors).
- Info closes after 5 s, 8 s with an action, paused while hovered; errors stay until closed.
- A repeated message counts up (x2) and restarts its timer instead of adding a card.
- No Undo actions anywhere (no undo support).
- Raised from a dialog: survives the dialog closing.

## Out of scope
- Command replies in general (064).
- The kiosk layout (`Density::Compact`) stays unchanged.

## Acceptance criteria
- [ ] Matches the mockup at the same window size, both themes.
- [ ] Five quick notifications show four cards; a duplicate shows x2; an error stays after 10 s.

## Verification
- `cargo clippy --all-targets --all-features`, `cargo fmt`, `cargo test --all-features`.
- UI: screenshots per `crates/core/AGENTS.md` next to the mockup: one info, one error, a x2 card, both themes.

## Notes
