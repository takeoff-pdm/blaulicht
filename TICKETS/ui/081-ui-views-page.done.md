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
- [x] Matches the mockup at the same window size, both themes.
- [x] A view renamed in the title shows the new name in the list.

## Verification
- `cargo clippy --all-targets --all-features`, `cargo fmt`, `cargo test --all-features`.
- UI: screenshots per `crates/core/AGENTS.md` next to the mockup: list and detail, Add view dialog.

## Notes
- Desktop page in `app/pages/view/desktop.rs`, entered from `view_ui` only when `Density::of(ctx).is_compact()` (the desktop); the kiosk path is untouched. Kiosk View at 800x480 compared with a decouple build: pixel-identical.
- The nav label already read "Views"; no other "Scene Stacks" string was left in the desktop UI.
- New `HostCommand::RenameView { view, name }` (appended at the end of the enum, edits the show; refused with "Name is empty" / "View not found"), with an engine test. No ABI or showfile change.
- Duplicate names are refused in the UI (Add view and rename) with an error toast; the engine does not refuse them.
- New `KnobSize::Row` (40px knob, unlit scale, muted name) for the 72px rows; `ListRow::reserve` keeps room for the delete icon. Remove and the delete icon turn `#b80f0f` under the pointer.
- Verified in blenv (1500x950, SPARTACUS_SOLI) with real clicks/keys: rename by Enter shows in the list, Esc restores, Add view prefill "New view" selected, duplicate "OFF" refused and the dialog stays open, Add overlay dropdown adds a scene, Remove, folder band collapse, knob drag (100 -> 83), delete asks first and the icon is hidden on the last view. Light theme compared with the mockup's light theme.
- Differences left: a new view still captures the playing overlay stack (engine `CreateView` behaviour the kiosk relies on), while the mockup's starts empty. The Add view dialog uses the shared studio modal (no footer band, Add view not blue as in the mockup). Scenes outside any folder are listed first without a band (the mockup has none).
- Checks: `cargo fmt` (only the touched files kept), `cargo clippy --all-targets --all-features` (no warnings in touched files) and `cargo test --all-features` (all pass) run inside the blenv container.

