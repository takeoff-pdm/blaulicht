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
- [ ] Matches the mockup at the same window size, both themes.
- [ ] Hiding Debug in the dropdown hides debug lines immediately.

## Verification
- `cargo clippy --all-targets --all-features`, `cargo fmt`, `cargo test --all-features`.
- UI: screenshots per `crates/core/AGENTS.md` next to the mockup: Logs with the level dropdown open.

## Notes
