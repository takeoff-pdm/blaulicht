# 087 — Programmer: fader bank looks disabled without a selection

| | |
|---|---|
| Epic | E9 Console UI |
| Depends on | none |
| ABI / showfile | none |

**Touches:** `crates/ui_egui/src/app/components/fixtures_shared.rs`, possibly `crates/ui_egui/src/app/components/fader.rs`

## Problem
Original note (2026-11-12): "when we have no selection, the fader bank should probably be greyed out a bit / visually disabled".

The Programmer's fader bank (Alpha, Strobe, Focus, Tilt, Pan, Hue, Saturation, Value) looks the same with and without a
fixture selection. While testing the kittest driver (`tools/blenv/KITTEST.md`), dragging the Alpha fader with nothing
selected changed nothing, and the UI gave no hint why. The toolbar already greys out its selection-dependent buttons
(`Palettes`, `UnLmt`, `Clear` via `add_enabled_ui(!selection_empty, ...)` in `programmer.rs`); the faders don't.

## Scope
- With an empty selection, draw the fader bank visibly disabled (dimmed track, handle and labels, no hover/drag feedback),
  for example by wrapping it in `ui.add_enabled_ui(!selection.is_empty(), ...)` or passing an enabled flag to the fader.
- First confirm that the faders really do nothing without a selection (not a bug where they should act on something).
- Both layouts: the kiosk (Touch density) and the desktop (Compact).

## Out of scope
- Other pages' faders (View Performance, plugin UIs).
- Changing what the faders do.

## Key code
- `fixture_controls` — `crates/ui_egui/src/app/components/fixtures_shared.rs:723`
- Toolbar enable pattern — `crates/ui_egui/src/app/pages/programmer.rs:285`

## Acceptance criteria
- [ ] With no selection, the fader bank is clearly dimmed and can't be dragged.
- [ ] Selecting a group or fixture makes it look and work as before.

## Verification
- `cargo clippy --all-targets --all-features`, `cargo fmt`, `cargo test --all-features`.
- UI: screenshots of the Programmer with and without a selection, desktop and kiosk layouts (`scripts/kt` on macOS,
  blenv otherwise).

## Notes
