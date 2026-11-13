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
- New component `app/components/position_view.rs` (`position_view`, `PlanDot`, `PlanSize`), drawn only by
  the desktop Fixture pane (`pages/setup/fixtures/desktop.rs`, Density::Desktop). The kiosk touch editor in
  `fixtures.rs` is untouched; kiosk Setup screenshot (800x480) still shows the touch layout.
- Geometry follows `fxplan.js` in its viewBox units (268x190 small, 860x540 large), scaled to the width the
  pane gives: fit with 1 m margin, zoom 0.5-12 about the pointer (wheel/pinch) or the centre (-/+), Fit
  resets, pan by dragging empty space, grid 1 m or 5 m below k=12. View and zoom are shared by both sizes
  and kept per view in egui temp memory (not saved).
- Dragging the blue dot snaps to 5 cm, clamps height at 0, updates the editor fields live and sends
  `UpdateFixture` with only the new position when the drag ends (as the mockup marks the show dirty).
  Typed position/rotation fields still apply with Save, as before. Esc ends a drag where it is.
- Clicking another dot selects that fixture and switches group (also inside the dialog, whose title follows).
- Dialog: 900px studio modal, title "Position: <name>", large view, position fields, Done (Esc or outside
  click also close). Built on `egui::Modal` + `studio::open_modal` since `studio::modal` is fixed at 420px
  with Cancel.
- Fields: captions Across (X), m / Height (Y), m / Depth (Z), m and X / Y / Z under each field, 2 decimals
  for metres (the mockup shows bare numbers).
- Verified in blenv `--desktop` with SPARTACUS_SOLI.json against mockup shots (small Top/Front, large
  Top/Front, dark and light): Front drag changed X 0 -> 2.25 and Y 0 -> 1.25 with Z unchanged, dragging below
  the floor stopped at Y 0, wheel zoom, Fit, pan, dot click selection, dialog open/close.
- Checks: `cargo fmt` (in blenv; it also reformatted ~20 unrelated files, which were reverted),
  `cargo clippy -p blaulicht-ui-egui --all-targets --all-features` and `cargo test -p blaulicht-ui-egui
  --all-features` (167 passed) in blenv. Not run: workspace-wide clippy/test (only ui_egui changed).
- Not verified: pixel comparison of the kiosk against a decouple build (no decouple build made; the diff
  touches no kiosk code), trackpad pinch, label halo exactness.
- Review fixes (2026-11-12): restored `child_folders`' doc comment (it had been attached to `form_heading`),
  and the Notes and `desktop.rs` module doc now say Density::Desktop instead of Compact.
- Back to .doing: the review could not verify the criteria. OrbStack/Docker is down (no
  ~/.orbstack/run/docker.sock), and it was left stopped on purpose (16 GB Mac, load average around 9). Still to do
  once Docker is up: blenv screenshots (small and large Top/Front views, both themes) against the mockup, drag,
  zoom, pan and dot-click checks, a kiosk 800x480 comparison against a decouple build, and workspace-wide
  `cargo fmt`, `cargo clippy --all-targets --all-features` and `cargo test --all-features`. None ran: there is
  no rustfmt on the host, and the fixes only touch comments.
