# 078 — Identify panel and Initialize for a selection

| | |
|---|---|
| Epic | E13 UI port |
| Depends on | 077 |
| ABI / showfile | none expected; check while implementing |

**Touches:** `app/pages/setup/fixtures.rs`; engine command for identify if missing

## Problem
Identify does not exist in egui; Initialize works on one fixture (`fxplan.js` Identify). Supersedes the dialog idea in 008. Mockup: `design/studio`; gaps listed in `design/studio/UI_GAP.md`.

## Scope
- Identify and Initialize in their own row under Patch, left-aligned, lit buttons (069).
- Identify drops down a panel from the button: fixture name and "2 of 3", Previous/Next and arrow keys, Transfer to selection (stop and select only that fixture), Stop/Esc/outside click. Only the button blinks.
- Steps through the selection, or through the group when one fixture is selected. The identified fixture's row is red and wins over blue.
- Initialize sends start-up values to every selected fixture.
- Offline: not sent.

## Out of scope
- Nothing beyond the scope.
- The kiosk layout (`Density::Compact`) stays unchanged.

## Acceptance criteria
- [x] Matches the mockup at the same window size, both themes.
- [x] Stepping moves the flashing fixture on the rig and the red row together.

## Verification
- `cargo clippy --all-targets --all-features`, `cargo fmt`, `cargo test --all-features`.
- UI: screenshots per `crates/core/AGENTS.md` next to the mockup: panel open with a red row.

## Notes
- Engine: new `HostCommand::IdentifyFixture(Option<(group, fixture)>)`, appended at the end of the enum
  (bincode indices stay). The engine overlays the fixture full white, 250 ms on / 250 ms off, after the
  render and the Initialize runs (over the grand master, so it is findable during a blackout); each step
  starts lit. One fixture at a time; `None` stops. It expires after `IDENTIFY_TIMEOUT` (3 s) unless repeated,
  and the desk repeats it every second while the panel is open, so a client that goes away can't leave a
  fixture flashing. Not saved (`edits_show` false). No ABI or showfile change.
- Desktop UI (`setup/fixtures/desktop.rs`, desktop density only): `desktop_identify_row` puts Identify and
  Initialize in one left-aligned row of lit buttons (069) under the patch fields, in the single editor
  (after Universe; Initialize moved out of the Save row) and the multi editor. The panel (`.idpop`, 270 px,
  4 px under the button) shows a steady red dot, name, "2 of 6", Previous/Next, Transfer to selection, Stop
  and the "Through …" hint; only the button's dot blinks. ←/→ step, Esc stops (not while a text field has
  focus), a click outside the panel, its button and the fixture rows stops (empty table space stops too), as does leaving the page or
  group. Several selected: steps through them from the first; one selected: through the group from it.
  The identified row is `rgb(184 15 15 / .45)` with white text and a 3 px red edge, over the blue selection.
  Transfer to selection stops and selects only that fixture. Initialize sends `InitFixtures` for every
  selected fixture. Offline, Identify and Initialize send nothing and show "… not sent: engine …".
  New helpers in `studio.rs`: `mini_button_w`, `text_width` made public.
- Verified in blenv `--desktop` 1500x950 with SPARTACUS_SOLI, MAC250E, against setup.html (headless Chrome
  over CDP) in dark and light: panel open with the red row; → moved the red row from #2 to #3 and, in the
  Simulate DMX grid, the flashing 255 moved from 418-435 to 436-453 at the same time; Shift+click of 3 rows
  gave "2 of 3", red over blue, "Through the selected fixtures"; Transfer to selection; outside click; Esc;
  Initialize lit with its notice. Kiosk Setup (800x480) screenshot byte-identical (`cmp`) with decouple.
- Checks in the blenv container (CARGO_BUILD_JOBS=4): `rustfmt --check` on the touched files (workspace
  `cargo fmt` would reformat untouched files, left alone), full `cargo clippy --all-targets --all-features`
  (nothing in touched code), `cargo test --all-features` all pass, new tests
  `identify_flashes_one_fixture_until_stopped`, `identify_flashes_its_fixture_white_over_the_show`,
  `identify_steps_through_the_selection_or_the_group`.
- Not verified: the offline path in the running app (no way to drop the engine link in blenv here; same
  `engine_link` check as Blackout); a real rig and the 3D visualizer (leaving the page stops Identify, so
  the flash was checked in the Simulate grid). The table does not scroll the identified row into view
  (`scrollIntoView` in the mockup); with the mockup's group sizes the rows always fit.
- Out of scope, noted: the single-fixture editor still lacks the mockup's Patch heading, Channels row,
  conflict message and position plan; that's the editor port, not this ticket.
- Review fix (arrow keys): ←/→ did nothing, because the gate was `egui_wants_keyboard_input()`, which is
  true whenever any widget has focus, and Next takes focus when the panel opens. Now the panel owns ←/→
  while nothing or one of its own buttons has focus (a text field elsewhere keeps its arrows), and it sets
  a focus-lock filter on its focused button so egui doesn't move the focus with the arrows. Re-verified in
  blenv `--desktop` with SPARTACUS_SOLI, MAC250E, #2 selected: Right ×2 went from 2 of 6 to 4 of 6 with the
  red row following; after a click inside the panel, Left ×3 went #4 to #1; after clicking Previous (#1 to
  #6, wrap), Right ×2 went to #2; Esc still stops. Kiosk Setup (800x480) screenshot still byte-identical
  (`cmp`) with the earlier kiosk shots. In the container (CARGO_BUILD_JOBS=4): rustfmt --check on
  desktop.rs, clippy --all-targets --all-features (nothing in desktop.rs), cargo test --all-features pass.
- Review fix (outside click): the exemption was the whole table area, so a click on empty space below the
  rows kept Identify running. Now only the visible fixture row rects (clipped to the scroll view) are
  exempt, like `tr[data-fx]` in `fxplan.js`. Re-verified in blenv `--desktop` with SPARTACUS_SOLI,
  MAC250E #2: a click on row #3 kept the panel open (2 of 6, #2 red); a click at (800,600) below the rows
  stopped it (no red row, panel closed). Light theme compared side by side with setup.html in light
  (headless Chrome over CDP, Identify open on #2): red row with the red edge, lit Identify button, panel
  colours, "2 of 6", Next focus outline and hint match; the mockup's editor above the row (Patch, Channels,
  conflict line) is still the out-of-scope editor port. Kiosk Setup (800x480) still byte-identical (`cmp`)
  with the earlier kiosk shots. In the container (CARGO_BUILD_JOBS=4): rustfmt --check on desktop.rs,
  clippy --all-targets --all-features (nothing in desktop.rs), cargo test --all-features pass.
