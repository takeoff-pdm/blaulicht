# 084 — System General and Screens

| | |
|---|---|
| Epic | E13 UI port |
| Depends on | 069, 074 |
| ABI / showfile | none expected; check while implementing |

**Touches:** `app/pages/system/*.rs`

## Problem
`system.html#general`, `#screens`. Mockup: `design/studio`; gaps listed in `design/studio/UI_GAP.md`.

## Scope
- General: two columns. Left: Show (file, state, location, modified, size, room; Contents and Fixture types side by side; Load, Save as, Save primary, divider, Close), then Problems filling the rest (one row per problem with its action button, one patch summary line with Open patch map).
- Right: Engine graphs (Debug, Reload), About, Application (Theme, Restart/Quit, Shut down separated with its explanation). Stacks below 1000px.
- Screens list: two-line rows; plugin-controlled screens show a link icon and the plugin name highlighted, others "Desk window"; size after it.

## Out of scope
- DMX page (062).
- The kiosk layout (`Density::Compact`) stays unchanged.

## Acceptance criteria
- [x] Matches the mockup at the same window size, both themes.
- [x] General has no empty column at 1400x900.

## Verification
- `cargo clippy --all-targets --all-features`, `cargo fmt`, `cargo test --all-features`.
- UI: screenshots per `crates/core/AGENTS.md` next to the mockup: General at 1400 and 900 wide, Screens.

## Notes

- Desktop only: `render_system_general_desktop` (new `system/general.rs`) and `render_screens_desktop`
  (`system/screens.rs`) run when `layout.is_desktop()`; the kiosk keeps `render_system_general` and its
  Screens list untouched (kt kiosk shots at 800x480 show the old General and Screens tabs). The
  status bar's patch-overlap warnings are also desktop only.
- General: columns 60/40, Problems and Application fill to the bottom; one scrolling column below
  820px page width (1000px window minus the sidebar). Problems come from the same health data as
  the issues cell; patch overlaps are now counted there too, so "N issues" equals the Problems count.
  Problem actions: Open universe / Show overlap / Open patch map open System › DMX; Reload plugin
  sends `ReloadPlugins` (there is no per-plugin reload); MIDI and Serial are separate tabs here, so
  the buttons read "Open MIDI" / "Open Serial".
- Review fixes (now shown, as in the mockup): Size reads "518 KB, format 4" (the file's
  `format_version` on disk, parsed once per path and mtime); Contents ends with "Scene changes"
  (sum of every scene's changeset; `clone_for_ui` now keeps the small changeset of unfocused
  scenes, which also fixes Setup › Scenes showing 0 changes for them); About has "Commit"
  ("<short hash>, <commit date>" from a new `crates/ui_egui/build.rs`, row hidden without git) and
  "Audio input" ("<device>, <rate> Hz": `TickSpeeds` gained `audio_sample_rate`, so
  `PROTOCOL_VERSION` is now 4). The mockup's "pipewire" is the audio host; the app shows the
  selected device name instead. Uptime is still the UI's, not the engine's.
- Intentional deviation from the mockup: the serial/MIDI problem action reads "Open MIDI" or
  "Open Serial" instead of "Open MIDI & Serial", because MIDI and Serial are separate sidebar
  tabs in the app. Not seen live: kt has no MIDI or serial device problem rows.
- Engine Loop/Plugins graphs keep 30 s of history (150 samples, separate from the status bar's
  30-sample sparks) and scale to three times the history's peak (at least 1 ms), so they draw a
  dense, low line across the full width. Checked in kt after ~35 s.
- `setup::duo` helpers became `pub(crate)` and `type_label` moved there (shared with Fixtures).
- Screens: master-detail like the mockup (list with Add, preview pane with "Desk window"/"Owned by
  plugin X" and Remove). Verified in kt with a temporary, uncommitted patch that skips drawing
  external viewports (kt draws an external screen over the whole harness). Plugin-owned rows (link
  icon, highlighted plugin name) were not seen live: no plugin opened a screen in kt.
- Verified with kt (not blenv): General at 1500x950 dark and light, 1400x900 (no empty column),
  900x900 (stacked, scrolled), Screens at 1500x950; clicked Open universe 0 (opens DMX), Debug
  (opens the debug window), Light/Dark, Add, row select, Remove. Not clicked: Load/Save as (native
  dialog), Restart/Quit/Shut down.
- Checks on the host with CARGO_BUILD_JOBS=4: `cargo fmt` (only touched files kept), `cargo clippy
  -p blaulicht-ui-egui -p blaulicht-engine --all-targets --all-features` (no warnings in changed
  code; existing ones elsewhere), `cargo test -p blaulicht-ui-egui -p blaulicht-engine
  --all-features` (169 + 279 passed). Workspace-wide clippy/test not run.
- Review fixes verified in kt (desktop 1500x950, SPARTACUS_SOLI.json copy), dark and light,
  against the mockup shot. Kiosk not re-shot: the fixes only touch the desktop General page and
  data (format/scene-change/audio fields, graph history); the Compact path draws no new rows.
