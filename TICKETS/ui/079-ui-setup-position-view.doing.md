# 079 — Fixture position view

| | |
|---|---|
| Epic | E13 UI port |
| Depends on | 077 |
| ABI / showfile | none expected; check while implementing |

**Touches:** `app/pages/setup/fixtures.rs`, new `app/components/position_view.rs`

## Problem
The mockup shows a 2D view of the rig next to the position fields (`fxplan.js`). Relates to 027 (position assistant). Mockup: `design/studio`; gaps listed in `design/studio/UI_GAP.md`.

## Scope
- Top (across, depth, audience at the bottom) and Front (across, height) views of every fixture; same group brighter, selected blue with its name.
- Click a dot selects that fixture (switching group); drag the selected dot moves it in 5 cm steps, height not below 0; fields update live.
- Wheel zoom about the pointer, -/Fit/+ buttons, drag empty space to pan, 1 m or 5 m grid; zoom kept per view.
- Expand button opens a large view in a dialog with the position fields.
- Position fields captioned Across (X) / Height (Y) / Depth (Z), m; rotation X/Y/Z degrees.

## Out of scope
- Room outline and stage objects.
- The kiosk layout (`Density::Compact`) stays unchanged.

## Acceptance criteria
- [ ] Matches the mockup at the same window size, both themes.
- [ ] Dragging a fixture in Front view changes only X and Y.

## Verification
- `cargo clippy --all-targets --all-features`, `cargo fmt`, `cargo test --all-features`.
- UI: screenshots per `crates/core/AGENTS.md` next to the mockup: small view, large dialog, both views.

## Notes
