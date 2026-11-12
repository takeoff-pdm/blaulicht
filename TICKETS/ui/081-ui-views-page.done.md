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
- Differences left: scenes outside any folder are listed first without a band (the mockup has none).
- Review fixes: a view added from the desktop Add view dialog now starts empty, as in the mockup. The dialog sends the new `HostCommand::CreateEmptyView { name }` (no ABI or showfile change; covered in the engine views test). The kiosk still sends `CreateView`, which copies the playing stack. The shared studio modal now has the `.dlg footer` band (head fill, 8/10 padding, 4px gap), and its confirm button is the primary blue unless it is a danger button (Delete view stays red). Checked with blenv at 1500x950: 'Chorus' added while OFF was playing shows count 0. The kiosk View page (800x480) looks the same, and its code path is untouched. fmt, clippy and tests were run inside blenv: all tests pass, and clippy shows no new warnings.
- Checks: `cargo fmt` (only the touched files kept), `cargo clippy --all-targets --all-features` (no warnings in touched files) and `cargo test --all-features` (all pass) run inside the blenv container.

- Rebased onto decouple after 075-080 landed (2026-11-12): the earlier ui/081 commits were replayed; conflicts in `hub.rs` (kept `IdentifyFixture` and `RenameView`, appended in that order), `studio.rs` (`ListRow` keeps both `note` and `reserve`; `KnobSize::Small` stays 32px from 075, `Row` stays 40px) and `TODO.md`.
- Re-verified with kt (egui_kittest, `scripts/kt start --layout desktop --width 1500 --height 950`, SPARTACUS_SOLI copy), no blenv: page matches `pages.html#views` at 1500x950 (dark); renaming "OFF" to "Main show" with Enter shows in the list; Esc restores; Add view prefills "New view", a duplicate name is refused with a toast and the dialog stays open; "Chorus" is added empty (count 0); Add overlay dropdown adds WHITE; delete asks first, and the icon is hidden on the last view. The light theme was not rechecked after the rebase. The kiosk was not rechecked with a screenshot either: the Compact gate in `view_ui` and the kiosk code path are unchanged since the pixel-identical comparison above.
- Checks after the rebase, on the host: `cargo test --all-features` (nightly) all pass; rustfmt (stable) is clean on the touched files; clippy (stable toolchain in nightly mode, because the nightly toolchain has no clippy) shows no warnings in changed lines.
