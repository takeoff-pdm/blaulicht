# 074 — Status bar issues cell

| | |
|---|---|
| Epic | E13 UI port |
| Depends on | 059, 070 |
| ABI / showfile | none expected; check while implementing |

**Touches:** `app/components/status_bar.rs`

## Problem
The separate DMX offline cell is folded into one issues cell (`chrome.js` status bar, `RULES.md` Status bar). Mockup: `design/studio`; gaps listed in `design/studio/UI_GAP.md`.

## Scope
- Remove the per-output cell. One cell at the right: error icon and/or warning icon for the kinds present, then "N issues".
- Tooltip splits the count ("1 error, 3 warnings"); click opens System, General.
- Engine disconnected or restarting still gets its own cell.

## Out of scope
- Nothing beyond the scope.
- The kiosk layout (`Density::Compact`) stays unchanged.

## Acceptance criteria
- [ ] Matches the mockup at the same window size, both themes.
- [ ] With an offline universe and plugin warnings the cell shows both icons and the total.

## Verification
- `cargo clippy --all-targets --all-features`, `cargo fmt`, `cargo test --all-features`.
- UI: screenshots per `crates/core/AGENTS.md` next to the mockup: status bar with errors and warnings, with only warnings.

## Notes
