# 058 — Desktop studio components and list style

| | |
|---|---|
| Epic | E9 Console UI |
| Depends on | none (builds on 056) |
| ABI / showfile | none |

**Touches:** `crates/ui_egui/src/app/ui_tokens.rs`, `app/components/` (fader, knob, tree rows, dialog, toggles, toolbar)

## Problem
The studio mockups use one set of tokens and controls, but the egui desktop layout does not have them yet. Every page ticket needs these first.

## Scope
- Add tokens from `design/studio/studio.css`: recessed list, folder band, hairline, changed amber, graph data, readable red text, dimmed selection.
- List and tree rows: folder band with icon and tree line, hairlines, single-line truncation with a tooltip, faint IDs, number-only counts.
- Selection: active row is solid blue with a left edge; included rows have a thin outline and no fill.
- One fader, one knob, one horizontal fader, one switch and one segmented control, applying the control table in `RULES.md`.
- One page bar: view tabs left, page actions right, 30px high.
- Modal dialogs: focus stays inside, one at a time, Cancel is default, Esc closes.
- Minimum target 24px, 28px for controls used during a show; minimum text 11px; 4.5:1 contrast.

## Out of scope
- Page layouts (036, 037, 038, 062). The kiosk: all of this applies only at `Density::Compact`.

## Acceptance criteria
- [x] A showcase or the first ported page shows every component in both themes.
- [x] A 40-character name truncates on one line and never overlaps the next row.
- [x] Tab cannot leave an open dialog, and a second dialog cannot open.
- [x] Kiosk screenshots are unchanged.

## Verification
- `cargo clippy --all-targets --all-features`, `cargo fmt`, `cargo test --all-features`.
- UI: run with `--desktop` via `scripts/blenv` and compare screenshots against the mockup, in dark and light theme, at 1280x800 and 2560x1440. Confirm the 800x480 kiosk is unchanged.

## Notes
- The Out of scope line says "applies only at `Density::Compact`"; in this codebase Compact *is* the desktop density
  (Touch is the kiosk), so everything here is desktop-only, as intended. The kiosk never calls these components.
- Code: tokens in `app/ui_tokens.rs` (`Studio`, `STUDIO_DARK`/`STUDIO_LIGHT`, `studio(ctx)`, `TARGET_MIN` 24,
  `TARGET_SHOW` 28, `TEXT_MIN` 11, `PAGE_BAR_HEIGHT` 30, `contrast_ratio`); components in
  `app/components/studio.rs` (`ListRow`, `folder_band`, `list_frame`, `fader`, `knob` in two sizes, `hfader`,
  `switch`, `segmented`/`view_tabs`, `page_bar`, `mini_button`, `open_modal`/`modal`). New module instead of
  restyling `fader.rs`/`knob.rs`/`switch.rs`/`tree.rs`, because those are shared with the kiosk.
- Showcase: `app/studio_showcase.rs`, desktop only, replaces the page when toggled with Ctrl+Shift+F12 (or start with
  `BLAULICHT_STUDIO_SHOWCASE=1`). It has a Dark/Light switch in its page bar.
- Verified with blenv `--desktop` (1500x950), dark and light: rows, folder bands (count only when collapsed), tree line,
  hairlines, 40+ character names cut on one line with a tooltip, active vs included rows, fader/knob/hfader drags,
  switch, segmented, folder toggle. Dialog: Cancel focused on open, 7x Tab stays inside the dialog, a second dialog is
  refused while one is open, Esc cancels, danger dialog has a red primary.
- Kiosk: 800x480 screenshots of Programmer and Setup are byte-identical to decouple; System differs only in its live
  loop graph. Ctrl+Shift+F12 does nothing in the kiosk.
- Contrast 4.5:1 is a unit test over the tokens (`studio_text_meets_4_5_contrast`). Muted text on `ctl` is 4.2:1 in
  dark, so muted text must not sit on a button; the test documents that.
- Checks ran inside the blenv container with CARGO_BUILD_JOBS=4: `cargo test -p blaulicht-ui-egui --all-features`
  (130 passed), `cargo clippy --all-targets --all-features` (no errors, no warnings in the new files), rustfmt on the
  touched files. `cargo fmt --all --check` still reports older drift in other crates (e.g. `audio_features`), not
  touched here.
- Not verified: 1280x800 and 2560x1440 window sizes (only 1500x950 was captured); keyboard Enter on the focused
  primary button; tooltips on truncated names (code path only, not screenshotted). The existing pages do not use the
  new components yet; adopting them is up to the page tickets (036-038, 062, 069+).
Reference: open the named pages in `design/studio/` in a browser. `design/studio/RULES.md` holds the agreed rules. The mockups are the design source, but their JavaScript is throwaway.
