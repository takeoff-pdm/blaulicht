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
- [ ] Matches the mockup at the same window size, both themes.
- [ ] General has no empty column at 1400x900.

## Verification
- `cargo clippy --all-targets --all-features`, `cargo fmt`, `cargo test --all-features`.
- UI: screenshots per `crates/core/AGENTS.md` next to the mockup: General at 1400 and 900 wide, Screens.

## Notes
