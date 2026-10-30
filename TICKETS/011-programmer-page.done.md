# 011 — Programmer page: rename, clear selection, scene changes with undo-all

| | |
|---|---|
| Epic | E3 Programmer |
| Depends on | 006 |
| ABI / showfile | Possibly **ABI bump** if a new `ControlEvent` (e.g. `ClearSceneChanges`) is added (append at the end) |

**Touches:** `crates/core/src/app/pages/fixtures_perf.rs`, `crates/core/src/app/components/fixtures_shared.rs`, `crates/shared/src/page/mod.rs (label only)`, engine event handling for undo-all

## Problem
Original review notes (2026-09-27):

> also fixtures performance should rather be 'programmer'
> also selection in the programmer should have a 'clear' shortcut
> also easy view of the scene changes, and one button to undo all
> Removing changes from a scene (root backlog: "most important")

## Scope
- Rename the FixturesPerformance label to **Programmer**. Keep the `AppPage` variant index stable, and rename the Rust identifier only if it doesn't break bincode.
- Add a **Clear selection** button and a keyboard shortcut (e.g. `Esc` or `C`, when no text field has focus) that sends `RemoveAllSelection`.
- Add a **Scene changes** panel for the focused scene. It lists the changed fixtures and properties (reusing `render_scene_changeset_dialog` / `scene_changes`) with per-row remove and one **Undo all** button (with confirmation) that clears the scene's sink changes. Also bring over the scene animations dialog that 006 removed from setup.

## Out of scope
- Selection lock (013).
- Animation presets (012).

## Key code
- `fixtures_ui` — `crates/core/src/app/pages/fixtures_perf.rs:142`
- `render_scene_changeset_dialog`, `scene_changes` — `fixtures_shared.rs` / `fixtures_perf.rs`
- `EngineSelection::clear`, `RemoveAllSelection` — `crates/shared/src/state/engine.rs:26-60`

## Acceptance criteria
- [x] The navbar says Programmer. (The kiosk navbar shows icons only; the page label `AppPage::short()` and the desktop dock tab say "Programmer". See Notes.)
- [x] The shortcut clears the selection.
- [x] Undo all empties the focused scene's changes and nothing else (check other scenes with `blctl.py state`).

## Verification
- `cargo clippy --all-targets --all-features` and `cargo fmt` are clean.
- `cargo test --all-features` passes, and new logic has co-located `#[cfg(test)]` tests.
- If the ABI was bumped: `PLUGIN_ABI_VERSION` in `crates/shared/src/abi.rs` is incremented and `cd crates/plugins && make build_noopt` was run. The Makefile reports success even on failure, so grep its output for `^error`.
- Follow **UI Verification** in `crates/core/AGENTS.md` (check that `swaylock` is not running, launch from the repo root, navigate via blctl, capture with grim). Use the headless Xvfb recipe when clicks are needed or another session is running (see memory notes). Take before and after screenshots, keep them in the scratchpad, and never commit them.
- Screenshot the Programmer with the scene-changes panel populated, then after Undo all.

## Notes
<!-- Implementation notes, decisions, and follow-ups found while working. Keep this updated before renaming to .done.md. -->
### What changed
- **Rename:** `AppPage::FixturesPerformance` is now `AppPage::Programmer`, at the same index (6), with
  `#[serde(alias = "FixturesPerformance")]`. Old showfile dock layouts, `midi_all` mappings and blctl
  `NavigatePage":"FixturesPerformance"` still work. `short()` and the dock tab title are "Programmer".
  - The kiosk navbar shows icons only; `short()` is not drawn anywhere. "Programmer" is visible in the desktop dock
    tab, the `console` page labels and the `midi_all` page names.
  - Code: `pages/fixtures_perf.rs` became `pages/programmer.rs`, `fixtures_ui` became `programmer_ui`, and
    `FixturePerfUi` / `fixture_perf_ui` became `ProgrammerUi` / `programmer_ui_state`.
  - `blenv nav` and the docs (AGENTS.md, blaulicht-env skill) use `Programmer`.
- **Clear selection:** a Clear button sits in the toolbar before UnLmt and is disabled while nothing is selected.
  `Esc` clears the selection too. Both send `RemoveAllSelection`.
  - `Esc` does nothing while a text field has focus, while any dialog is open (dialogs close on `Esc`; checked
    via egui's top modal layer) or while nothing is selected. The key is consumed, so a Programmer shown in two
    panes sends the event once.
  - The toolbar has 7 buttons now (`TOOLBAR_BUTTONS`). They share a width (`toolbar_button_width`, 60 to 90 px)
    so all of them fit at 800x480. Narrow dynamic panes keep 90 px and wrap.
- **Scene changes:**
  - **Panel** (`scene_changes_panel`, right column under the masters): the focused scene's changes, sorted by
    group, fixture and property. Each row shows the property and "group · fixture" with an X button that sends
    `RemoveChange` for that row. Above the list: **All (n)** and **Undo all**. The list fills the rest of the
    column and keeps about one animation card of room when the selection has animations.
  - **Dialog** (`render_scene_changes_dialog`, opened by All): all changes grouped by fixture, with one
    "X Property" button per change, plus Undo all and Close. It replaces `render_scene_changeset_dialog`, which was
    removed together with `current_scene_changeset_dialog_open`. The new state lives in `ProgrammerUi`
    (`scene_changes_dialog_open`, `undo_all_changes_confirm_open`).
  - **Undo all** opens a confirmation (`render_undo_all_changes_dialog`) that names the scene and the count.
    Confirming sends `undo_all_changes_event`: one `Transaction` with one `RemoveChange` per property, covering
    every fixture that changed it. Like the per-row remove, it acts on the focused scene only.
  - When the changes dialog is open, the confirmation is rendered inside it. That gives it the higher modal order.
    Rendered beside it, the confirmation ended up behind the dialog once the dialog had been clicked.
- **Scene Animations dialog:** the existing button and dialog on this page are unchanged and verified to open.
- **ABI: not bumped (stays 16).** No `ControlEvent` was added; Undo all composes `Transaction` + `RemoveChange`.
  The rename keeps the bincode index. `crates/shared/src/folder.rs` was not touched.
  - **Plugins should still be rebuilt after the merge.** A plugin parses blctl/JSON with its own copy of the
    shared crate. The stale inspector bundle answered `NavigatePage":"Programmer"` with "unknown variant
    `Programmer`" until `blenv build plugins` rebuilt it. The old name keeps working with stale bundles.

### For later tickets
- **017:** the Programmer's group and fixture selection is `BlaulichtApp::fixture_selection` in
  `components/fixtures_shared.rs`, called from `programmer_ui` in `pages/programmer.rs`. 017's Key code names
  `group_selection`, but that is the Setup › Fixtures picker.
- **012:** the animation buttons (Scene Animations, Add Animations) are in the Programmer toolbar in
  `programmer_ui`. Adding a toolbar button means raising `TOOLBAR_BUTTONS`.
- **013:** the planned lock toggle is another toolbar button (`TOOLBAR_BUTTONS` = 8 gives about 70 px each at
  800 px). The selection-changing events it must block now include the Clear button and `Esc`, both
  `RemoveAllSelection`.

### Verification
- `cargo clippy --workspace --all-targets --all-features` (blenv container): no errors. The only warnings in
  `programmer.rs` are the two old `for (fix_id, _) in &group.fixtures` loops in Lmt All. The new code adds none.
- `cargo fmt --all --check`: only the known old diff in `pages/animations.rs`.
- `cargo test --workspace --all-features`: 300 passed. New tests:
  - `app::pages::programmer::tests::undo_all_empties_the_focused_scene_and_keeps_the_others` runs the event
    through `DmxEngine::apply` with two scenes;
  - `undo_all_groups_the_changes_by_property`, `undo_all_without_changes_sends_nothing`,
    `changes_are_sorted_by_group_fixture_and_property`,
    `escape_clears_only_a_non_empty_selection_outside_text_fields_and_dialogs`,
    `toolbar_buttons_shrink_to_fit_but_not_below_the_minimum`;
  - `page::sub::tests::programmer_keeps_the_fixtures_performance_index_and_name`;
  - `config::tests::dock_tabs_saved_as_fixtures_performance_load_as_programmer`.
- **Plugins:** no ABI bump, so `make build_noopt` was not run. `blenv build plugins` (the default set) and
  `blenv build plugins console midi_all` (the plugins with source changes) built without errors.
- **App (blenv, a copy of `Kuze_Theater.migrated.json`):** the show loaded with its groups and scene, not a
  default show. All checks used real clicks, drags and keys:
  - before/after shots of the page at 800x480 and in `--desktop`;
  - a group was selected and Strobe/Tilt dragged: the count went from 79 to 81;
  - the X on a row removed it (81 to 80); a chip in the dialog removed one change;
  - `Esc` cleared the selection at 800x480 and in `--desktop`. With the Scene Animations dialog open, `Esc`
    kept the selection; with the confirmation open, `Esc` closed only the confirmation;
  - the Clear button cleared the selection;
  - Undo all with confirmation on scene 0, after cloning it to scene 1 in Setup › Scenes.
    `blctl state` (Debug dump, parsed per scene) before: scene 0 has 81 changes, scene 1 has 79. After: scene 0
    has 0, and scene 1 still has 79 with an identical set (same hash);
  - inline Undo all on scene 1, then Cancel: still 79.
  - In `--desktop` the saved `"FixturesPerformance"` dock pane came back as "Programmer", and the changes dialog
    renders.
- Screenshots are in `.blenv/shots/` (not committed).

### Not covered / follow-ups
- Removing a change (per row or Undo all) uses the existing `RemoveChange`: it sets the property to 0 in the
  scene's fixture state; it does not restore an earlier value. Added to the Inbox.
- Undo all only clears the changeset (fixture properties). Animations, palette assignments and the scene masters
  are left alone; animations are handled in the Scene Animations dialog.
- The showfile JSON holds 141 changeset entries, but the app shows 79 after loading. Entries for fixtures that no
  longer exist are pruned on load (`retain_fixture_keys`); this is not new behaviour.
