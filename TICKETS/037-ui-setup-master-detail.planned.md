# 037 — Setup pages in the desktop studio layout

| | |
|---|---|
| Epic | E9 Console UI |
| Depends on | 058, 059 |
| ABI / showfile | none |

**Touches:** `app/pages/setup/{fixtures,groups,scenes,animations,palettes}.rs`, `app/pages/animation_spec_editor.rs`

## Problem
The desktop Setup pages should follow the agreed studio design, and first-time testing found dead ends.

## Scope
- Layouts as `setup.html` and `animation-editor.html`, including the live patch check on Fixtures and a fixture filter.
- Add fixture: group field, default avoiding offline outputs, validation that blocks Add, new row selected.
- Patch numbers apply as a whole when leaving the field; invalid values are rejected with a message.
- Animations show Used in and Add to scene.
- Delete names its consequences. Every tab has a designed empty state.

## Out of scope
- The kiosk. Programmer (036).

## Acceptance criteria
- [ ] Typing 99999 never moves a fixture.
- [ ] Deleting a palette lists the pointers that follow it.
- [ ] Deleting the last item of each kind shows the empty state with no error.

## Verification
- `cargo clippy --all-targets --all-features`, `cargo fmt`, `cargo test --all-features`.
- UI: run with `--desktop` via `scripts/blenv` and compare screenshots against the mockup, in dark and light theme, at 1280x800 and 2560x1440. Confirm the 800x480 kiosk is unchanged.

## Notes
Used in needs scene data per animation; check it is available to the client. The mockup list is sample data.

## Fixtures: multi-select, move, Identify (mockup 2026-11-11)

- Fixture table: click, Ctrl/Cmd+click, Shift+click. Several selected: the pane shows "N selected", per-fixture fields frozen as `--`.
- Move to group: header button (group dropdown with filter) and dragging rows onto a group in the tree. No undo.
- Identify: drop-down panel from the button with Previous/Next, "Transfer to selection" (stop and select only the identified fixture) and Stop; red row for the fixture flashing. Initialize applies to the whole selection.
- Fixture position: 2D Top/Front view with zoom, pan, larger dialog; X/Y/Z captions; number fields scrub sideways.
