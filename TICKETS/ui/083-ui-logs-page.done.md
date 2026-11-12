# 083 — Logs page

| | |
|---|---|
| Epic | E13 UI port |
| Depends on | 071 |
| ABI / showfile | none expected; check while implementing |

**Touches:** `app/components/log.rs` and the Logs page

## Problem
`pages.html#logs`. Mockup: `design/studio`; gaps listed in `design/studio/UI_GAP.md`.

## Scope
- Title "Logs" at the left of the filter bar; no separate pane title; no sample-lines tag.
- Search, level filter as a multi-select dropdown with icons and counts (label "All levels" / list / "No levels"), source dropdown, Follow, Copy, Clear.
- Source tags equal width (fit the longest), centred.

## Out of scope
- Nothing beyond the scope.
- The kiosk layout (`Density::Compact`) stays unchanged.

## Acceptance criteria
- [x] Matches the mockup at the same window size, both themes.
- [x] Hiding Debug in the dropdown hides debug lines immediately.

## Verification
- `cargo clippy --all-targets --all-features`, `cargo fmt`, `cargo test --all-features`.
- UI: screenshots per `crates/core/AGENTS.md` next to the mockup: Logs with the level dropdown open.

## Notes
- Desktop path is `LogWindow::draw_desktop` (`app/components/log.rs`) with the pieces in
  `app/components/log/desktop.rs`; `draw` returns early on the desktop density. The kiosk branch
  is the old code unchanged (only the always-true desktop level check was dropped from it).
- Bar (30px): "Logs" title, search with magnifier ("Filter messages", matches highlighted in the
  rows), level multi-select dropdown (funnel, icons, counts; "All levels" / list / "No levels",
  min 128px and growing with the label), source dropdown, then Follow (blue-dim while on), Copy
  (visible lines to the clipboard) and Clear flush right. No pane title, no status line, no
  sample-lines tag. Table header TIME/LEVEL/SOURCE/MESSAGE, rows per `.pg-log` (ms in a fainter
  tone, level icon + label, warn/error left edge and error tint), footer "N of M lines", warning
  and error counts, Following/Paused.
- Source tags all share one width: the longest buffered source (at least 7ch) + 12px, centred.
- Follow now only changes on a user scroll (scrolling up pauses, back to the end resumes) or the
  button, so new lines no longer switch it off.
- Verified with kt (`scripts/kt`, 1500x950, desktop, dark and light): unchecking Debug hid the
  debug line at once, search, Follow -> Paused, Clear -> "No log lines match the filter.". Kiosk
  shot at 800x480 shows the old touch Logs page; its code path is untouched.
- Not verified: the clipboard content of Copy (kt has no clipboard); the mockup with the level
  dropdown open (headless Chrome did not open it; the dropdown itself is the 071 component) and
  the mockup in the light theme (the page ignored the forced theme), so light was checked against
  the studio light tokens only. A second kt instance of another session was running, so kt ran
  with a config copy on port 1284 (the inspector's 9099 bind failed, harmless).
- Checks: `cargo test -p blaulicht-ui-egui --all-features` passes on the host (165 tests);
  rustfmt (stable toolchain's binary) on the changed files. `cargo clippy` could not run on the
  host: the nightly toolchain has no clippy component and the stable clippy-driver can't build the
  nightly crates. The workspace-wide `cargo test --all-features` was not run (only ui_egui).

- Review fixes (2026-11-12, kt 1500x950 against `pages.html#logs` / `#light`):
  - Edge to edge: `draw_desktop` paints into the content rect expanded by the shell's 8px margin
    (as `setup/duo.rs`), so the 30px bar starts at y=0 next to the sidebar, the footer sits on the
    status bar and there is no right gutter.
  - Row selection (`tr.sl`): rows sense clicks; a click selects (blue-dim fill replacing stripe and
    error tint, strong time; strong level and message on Info rows, level colours kept as in the
    CSS), a second click deselects. Selection is by a per-window sequence number on `LogEntry`, so
    it survives new lines.
  - Rows are 23px like the mockup (2px padding around the 18px level line plus the 1px border).
    Source tags are outline only (1px line, no fill); the dark look was the stripe behind them.
  - Light theme checked in kt via System > General > LIGHT (the switch exists there) against the
    `pages.html#light` mockup; then switched back to DARK.
  - Checks on the host with CARGO_BUILD_JOBS=4 after adding nightly `rustfmt`/`clippy` via rustup:
    `cargo fmt` (only the log files kept; it also reformatted unrelated files, reverted),
    `cargo clippy --all-targets --all-features` (no errors, no warnings in the log files;
    pre-existing warnings elsewhere), workspace-wide `cargo test --all-features` passes.
  - Kiosk: not re-shot; the review changes are all inside `draw_desktop`/`desktop.rs`, and the
    only shared change (the `seq` field on `LogEntry`) is not drawn, so the Compact path is the same.

Review fix (2026-11-12): log rows drew through the sticky header and under the footer. The
body clip is now pinned to the body rect before the ScrollArea (which widens its clip by
`clip_rect_margin`), so rows cut at the header's bottom (y=52) and the footer's top (y=902).
Checked in kt at 1500x950 with Follow at the end and Paused at the top. Kiosk path untouched
(change is inside `draw_desktop` only). fmt clean for touched files, clippy and tests pass
(host, CARGO_BUILD_JOBS=4); `cargo fmt --check` still reports older diffs in other crates.
