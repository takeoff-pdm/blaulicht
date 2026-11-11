# 081 — Views page (formerly Scene Stacks)

| | |
|---|---|
| Epic | E13 UI port |
| Depends on | 069, 071 |
| ABI / showfile | none expected; check while implementing |

**Touches:** `app/pages/view.rs`, nav label

## Problem
Rename and rebuild the page as in `pages.html#views` (`pages-orig.js` V.views). Mockup: `design/studio`; gaps listed in `design/studio/UI_GAP.md`.

## Scope
- Rename Scene Stacks to Views everywhere in the desktop UI.
- Left: Views list (count of scenes), Add in its header, delete icon on the selected row (asks first; hidden on the last view).
- Add view: dialog with only a name (free name prefilled, duplicates refused).
- Right: 40px title bar with the view name as an editable field with a pencil (Enter/blur saves, Esc restores); Add overlay (dropdown) and Apply.
- The view's scenes in folder groups, 72px rows in columns: name with "#id, N in stack", Intensity and Speed knobs next to it, Remove right-aligned turning red on hover.

## Out of scope
- Stack reordering.
- The kiosk layout (`Density::Compact`) stays unchanged.

## Acceptance criteria
- [ ] Matches the mockup at the same window size, both themes.
- [ ] A view renamed in the title shows the new name in the list.

## Verification
- `cargo clippy --all-targets --all-features`, `cargo fmt`, `cargo test --all-features`.
- UI: screenshots per `crates/core/AGENTS.md` next to the mockup: list and detail, Add view dialog.

## Notes
