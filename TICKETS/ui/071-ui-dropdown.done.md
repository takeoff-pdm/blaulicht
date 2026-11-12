# 071 — One dropdown component

| | |
|---|---|
| Epic | E13 UI port |
| Depends on | 058, 069 |
| ABI / showfile | none expected; check while implementing |

**Touches:** new `app/components/dropdown.rs`, every `ComboBox` call site

## Problem
The mockup uses one popover for every choice list (`chrome.js` `BL.dropdown`); egui uses ComboBox. Mockup: `design/studio`; gaps listed in `design/studio/UI_GAP.md`.

## Scope
- Popover on the pane surface with a 1px line, opens below (or above if no room), max 480px tall.
- Filter field from 8 options; arrow keys, Home/End, Enter picks, Esc or outside click closes.
- Groups as folder bands (069); optional meta text right (counts, ids).
- Multi-select variant: checkboxes, an All row, Alt-click selects only that row, list stays open.
- Replace every ComboBox on desktop pages with it. Only the Programmer scene picker dims the page.

## Out of scope
- Kiosk widgets.
- The kiosk layout (`Density::Compact`) stays unchanged.

## Acceptance criteria
- [x] Matches the mockup at the same window size, both themes.
- [x] No `ComboBox` left in desktop page code.
- [x] Keyboard-only use works: open, filter, pick, close.

## Verification
- `cargo clippy --all-targets --all-features`, `cargo fmt`, `cargo test --all-features`.
- UI: screenshots per `crates/core/AGENTS.md` next to the mockup: single-select with filter, multi-select (Logs levels).

## Notes
- Component: `crates/ui_egui/src/app/components/dropdown.rs`, the egui port of `BL.dropdown`. `Popover` (pane fill,
  1px line outside, below the anchor or above when below has less than min(content, 240) and less than above, max
  480px, min width max(anchor, 180), up to 360), filter strip from 8 options (`.ddq`: magnifier, 24px well, blue line
  on focus), 26px rows (`act` head fill, single-select current blue-dim with 2px blue edge, leaf indent 22 under 24px
  folder bands, mono 11px meta right), multi-select checks with an All row (more than 2 options, unfiltered) and a
  hairline under it, "Nothing matches." when empty, optional `dim` scrim. Helpers: `button` (`.ddbtn`, blue line
  while open or focused), `select`, `select_value`, `action` (fixed label, e.g. "Add scene") and `multi_select`.
  One list is open at a time (`open`/`close`/`toggle`, state in ctx temp data).
- Keys: Enter/Space/Alt+Down on the focused anchor opens it (the opening key is ignored by the list in its first
  frame); arrows wrap, Home/End only without a filter (in the field they move the cursor, like the mockup), Enter
  picks (Space too without a filter), Alt+Enter or Alt-click picks only that row in a multi list, Esc closes and
  gives focus back to the anchor, Tab or a press outside closes. Keys are consumed before the filter field sees them.
- Replaced on desktop: Animation editor plugin picker, Scene Graph node speed, Add scene (filter "Filter scenes",
  `#id` meta) and edge condition, Visualizer truss profile/part, plugin UI `Op::ComboBox`, and the Logs level
  filter (multi-select with Debug/Info/Warn/Error icons and counts, "All levels"/"No levels"/joined label) and
  source filter ("All sources" + sources). The Logs page layout itself stays for 083.
- Kiosk: every site branches on `dropdown::is_desktop` (`Density::Compact`); the kiosk path calls
  `dropdown::kiosk_combo`, which builds the same `ComboBox::from_label` / `from_id_salt` as before, so no page file
  names `ComboBox` any more. Kiosk Logs keeps Filter Level / Filter Source dialogs (`shown_levels` stays all true
  there).
- Verified in blenv `run --desktop` (1500x950) against `pages.html#logs` (levels dropdown open) and the Add overlay /
  Move to group popovers, dark and light (System > Light): Logs levels (click toggles, list stays open, Alt+Enter
  only, All row, counts, filled warn/error icons), Logs source (filter typed "in", Down + Enter picks, Esc
  refocuses the anchor, Enter reopens), Scene Graph Add scene (meta ids, pick adds the scene), Speed (current row
  marked, outside click closes).
- Kiosk (800x480): Scene Graph empty, with a node selected, Speed combo open and Add Scene combo open are
  byte-identical PNGs against decouple 3a464b1; kiosk Logs checked by eye (timestamps differ).
- Checks inside the blenv container, scoped to `-p blaulicht-ui-egui`: rustfmt on the touched files (a
  pre-existing diff in `fixtures_shared.rs` left alone), `cargo clippy --all-targets --all-features` has no warning
  in the new or changed code (all remaining ones pre-exist), `cargo test --all-features` 139 passed (3 new
  dropdown tests: one band per group, filter by label/meta drops empty groups, All row rules).
- Not verified: folder bands inside the popover and the open-above placement on a real page (no desktop page has a
  grouped list or a dropdown near the bottom yet; 077 Move to group will be the first); the `dim` scrim (Programmer
  scene picker, 075). The Visualizer truss and plugin-UI combos were not opened in the app (no truss / no plugin
  with a ComboBox op in the default setup); they use the same `select` as the verified ones. Esc inside a modal may
  also reach the modal's own Esc handling if that runs earlier in the frame.

- Review fix: Alt+Enter toggled the active row instead of picking only it, because the key loop tried
  `Modifiers::NONE` first and egui's `consume_key` matches modifiers logically (NONE also matches Alt). The loop
  now tries `Modifiers::ALT` first. Re-verified in blenv --desktop on Logs levels: all checked, hover Warn,
  alt+Return -> only Warn checked, label "Warn"; then Return on Error -> "Warn, Error" (plain toggle still works).
  Alt-click (reads `modifiers.alt`) is still untested in blenv: `blenv click` has no modifier option.
  Checks: fmt (blenv; no diff in dropdown.rs, pre-existing drift in other crates), clippy --all-targets
  --all-features -p blaulicht-ui-egui (blenv, clean), cargo test --all-features -p blaulicht-ui-egui (host,
  CARGO_BUILD_JOBS=4, 139 passed). Kiosk untouched: the change is key handling inside the desktop-only dropdown.
