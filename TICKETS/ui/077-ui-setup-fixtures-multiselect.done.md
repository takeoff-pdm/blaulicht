# 077 — Setup Fixtures: multi-select and Move to group

| | |
|---|---|
| Epic | E13 UI port |
| Depends on | 069, 071, 073 |
| ABI / showfile | none expected; check while implementing |

**Touches:** `app/pages/setup/fixtures.rs`

## Problem
The mockup selects several fixtures and moves them between groups (`setup.html` fixtures, `treedrag.js`). Mockup: `design/studio`; gaps listed in `design/studio/UI_GAP.md`.

## Scope
- Fixture table: click selects one, Ctrl/Cmd+click toggles, Shift+click range.
- Several selected: pane title "Fixtures: N selected", type or "Mixed", universe, start channel, channels, position and rotation frozen as `--`; Identify and Initialize stay.
- Move to group: header button opens the group dropdown (071); dragging fixture rows onto a group in the tree moves them (a selected row carries the selection). Notification, no undo.
- Delete names the count; separated from Move by a divider.

## Out of scope
- Identify (078), position view (079).
- The kiosk layout (`Density::Compact`) stays unchanged.

## Acceptance criteria
- [x] Matches the mockup at the same window size, both themes.
- [x] Three fixtures moved by drag land in the target group and the patch is unchanged.

## Verification
- `cargo clippy --all-targets --all-features`, `cargo fmt`, `cargo test --all-features`.
- UI: screenshots per `crates/core/AGENTS.md` next to the mockup: three selected, drag onto a group.

## Notes
- All in `app/pages/setup/fixtures/desktop.rs` (desktop shell only); the kiosk `setup_fixtures_ui` is untouched.
  The selection (`FixtureSelection`) lives in egui temp memory for the group it was made in; without one it is
  the fixture in the editor, so the Groups tab's "Edit fixtures" and the first-group pick still work.
  `click_select` follows `setup.html`: click selects one, Ctrl/Cmd toggles (the editor follows the last
  selected), Shift adds the range from the editor's fixture (replacing the selection unless Ctrl/Cmd is held).
- Fixture pane title: "Fixture: <name>", "Fixtures: N selected", or "Fixture" with nothing selected. Header
  actions for any selection: Move to group (the 071 dropdown, other groups under folder bands with counts,
  "Filter groups"), a `.hsep` divider, Delete -> "Confirm delete" / "Delete N fixtures" (red text) + Cancel.
  The title is hidden during the confirm step because the buttons need the whole 300 px bar (the mockup wraps
  there). The desktop single editor loses its own Move to group (id_selection_dialog) and Delete (dialog) buttons.
- Several selected: count, type or "Mixed", Universe / Start channel / Position / Rotation as disabled `--`
  fields at 50 %, Channels `--`, the `.msg.o` note, Identify and Initialize (Initialize sends `InitFixtures`
  for the whole selection and stays lit 10 s).
- Move = one `MoveFixtureToGroup` per fixture (the engine keeps universe and address); the selection clears and
  "Moved X to Y" / "Moved N fixtures to Y" is notified after the last reply. No undo.
- Drag: rows are click-and-drag; a drag started on a selected row carries the selection, on another row only
  that one. Carried rows at 40 %, the ghost ("3 fixtures") like 073's, the target group row outlined in blue,
  grabbing cursor, Esc cancels, dropping elsewhere or on the own group does nothing.
- Verified in blenv `--desktop` 1500x950 (scratch copy of `SPARTACUS_SOLI.json`) with real xdotool clicks:
  Shift+click range (3 selected) next to the mockup shot in dark and light; Ctrl+click toggle; Move to group
  dropdown -> FOG moved MAC250E #1-#3 ("Moved 3 fixtures to FOG"); drag of MAC250E #4-#6 onto DIMMER (outline,
  ghost) landed them with their patch unchanged (454-471, 472-489, 490-507, universe 0); Esc mid-drag cancels;
  Delete -> "Delete 3 fixtures" deletes all three. Kiosk Setup (800x480) is byte-identical (`cmp`) between
  decouple and ui/077.
- Checks inside the blenv container (CARGO_BUILD_JOBS=4): `rustfmt` on the touched file (workspace `cargo fmt`
  reformats untouched files on decouple), `cargo clippy -p blaulicht-ui-egui --all-targets --all-features`
  (no warnings in the touched file) and `cargo test -p blaulicht-ui-egui --all-features` (147 passed, new
  `selection_tests`). The full-workspace clippy/test was not run; ui_egui is the only crate changed.
- Not done / not verified: Identify is drawn but does nothing yet (078). Type names stay `model_string()`
  ("MartinMac250E", mockup "Martin MAC 250E"), as in the table before. Dragging groups between folders inside
  this page's group tree is not added (not in Scope). Not verified: a drag in a scrolled table or tree, and
  light-theme drag visuals (dark only).
- Review fixes: picking a group in the tree (and the first group picked at startup) now clears the selection,
  as setup.html does (`st.fx = null; st.fxs.clear()`): the pane shows "Fixture" and the "Select a fixture…"
  hint instead of opening the group's first fixture (`pick_setup_group` in desktop.rs). The Patch/Position
  headings of the editors use font weight Medium in `--muted`, as `.frm h4` (500, computed rgb(156,158,164)
  in the mockup). Re-verified with blenv desktop screenshots (startup HX-BASE, DIMMER pick, click, Shift+click
  of 3) against setup.html driven over CDP; kiosk Setup screenshot unchanged (desktop.rs is only drawn in
  desktop density). Kept as is: drag visuals (40% dimmed carried rows, blue outline on the target group)
  follow treedrag.js's intent rather than studio.css, which only styles `.row.dragsrc`/`.row.folder.dropin`;
  Type stays `model_string()`; dim Ch values on unselected rows are table styling from before this ticket.
  Full-workspace `cargo clippy --all-targets --all-features` (no errors, nothing in the touched file) and
  `cargo test --all-features` (all pass) in the blenv container; `cargo fmt --check` lists only files this
  ticket does not touch (same 92 diffs on the unchanged branch).
- Second review fixes: (1) the Fixture pane body is now `--list` like every `.body` in studio.css (it was
  `--pane`); the header bar keeps its own colour. In light the Identify/Initialize `.mini` buttons (`--ctl`
  #eeeef0) now sit on the #eeeeef body with no visible fill, as in setup.html#light; dark gives body
  rgb(23,24,26) and buttons rgb(58,59,63), the mockup's computed values (checked over CDP). (2) Fixture table:
  Type and Ch use `--muted` on unselected rows (`.rows td.dim`), `--strong` on selected ones. (3) Type names
  follow the mockup's `pretty` map via `type_label` ("Martin MAC 250E", "Dimmer", "LiteCraft AT10", ...;
  unknown models fall back to `model_string()`), in the table, the multi-select Type field and the Add fixture
  Type row; the filter matches both spellings. Re-verified in blenv `--desktop` 1500x950, MAC250E with
  Shift+click of 3 rows, dark and light, against setup.html driven over CDP. Kiosk Setup (800x480)
  byte-identical (`cmp`) with decouple. Checks in the blenv container (CARGO_BUILD_JOBS=4): `rustfmt --check`
  on the touched file, full `cargo clippy --all-targets --all-features` (nothing in the touched file),
  `cargo test --all-features` (all pass, new `type_label_follows_the_mockup_names`).
- Out of scope, noted: the sidebar labels/grouping (SYSTEM/SETUP/PERFORM, DMX and Art-Net apart, "View
  Performance") differ from the mockup ("DMX & Art-Net", "MIDI & Serial", "Live", "Perform"); that is shell
  work for another ticket.
