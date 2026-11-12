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
- [x] Matches the mockup at the same window size, both themes.
- [x] With an offline universe and plugin warnings the cell shows both icons and the total.

## Verification
- `cargo clippy --all-targets --all-features`, `cargo fmt`, `cargo test --all-features`.
- UI: screenshots per `crates/core/AGENTS.md` next to the mockup: status bar with errors and warnings, with only warnings.

## Notes
- Removed the Outputs cell (and `dmx_outputs`/`artnet` from `StatusBarInfo` and `HealthSummary`). Offline DMX
  universes and Art-Net were already counted as errors in `health_summary`, so they now show only in the issues cell.
- Error icon is the solid `fill::X_CIRCLE` in `t.red` (mockup `ph-fill ph-x-circle`, `--red`); 4px between the
  icons and 6px before the count, as `.probs` in `studio.css`. Tooltip "2 errors, 3 warnings. Open the problem list."
- Fix: status bar clicks used `open_subpage`, which only changes the tab inside the current page, so clicking a
  cell from Logs did nothing. They now use `show_subpage` (switches page like a sidebar click). This applies to
  every status bar cell (Remote UI, routing, save failed).
- "Plugin warnings": plugins have no warning state today (only `has_errored`, counted as an error). Warnings are
  MIDI/serial devices in an error state, as before. A plugin warning state would be a separate ticket.
- Verified in blenv `--desktop` (1500x950) against `system.html`: errors+warnings (dark and light), warnings only,
  tooltip, click from Logs opens System > General. blenv always has 2 offline DMX universes (2 errors); the
  warnings, and the warnings-only case, were produced by temporarily overriding the counts in `app_navbar.rs`
  (reverted, not committed). Real MIDI/serial warnings were not reproduced in the container.
- Kiosk: the status bar is desktop-only; kiosk System page screenshot matches the earlier decouple shot.
- Checks run inside the blenv container (CARGO_BUILD_JOBS=4): `cargo fmt` (unrelated reformat hunks in other
  files dropped), `cargo clippy --all-targets --all-features` for ui_egui (no new warnings in touched code),
  `cargo test --all-features` in `crates/ui_egui` (145 passed). Full-workspace tests were not run.
