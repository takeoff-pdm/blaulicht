# 080 — Setup pages: title bars and detail panes

| | |
|---|---|
| Epic | E13 UI port |
| Depends on | 069, 073 |
| ABI / showfile | none expected; check while implementing |

**Touches:** `app/pages/setup/*.rs`

## Problem
Small agreed changes on the other Setup pages (`setup.html`). Mockup: `design/studio`; gaps listed in `design/studio/UI_GAP.md`.

## Scope
- 40px top bars with prefixes on Groups, Scenes, Animations, Palettes.
- Scenes detail: no master intensity or speed, no Folder field.
- Groups detail: no Folder field.

## Out of scope
- Nothing beyond the scope.
- The kiosk layout (`Density::Compact`) stays unchanged.

## Acceptance criteria
- [ ] Matches the mockup at the same window size, both themes.
- [ ] Every Setup page's top bars line up across columns.

## Verification
- `cargo clippy --all-targets --all-features`, `cargo fmt`, `cargo test --all-features`.
- UI: screenshots per `crates/core/AGENTS.md` next to the mockup: each Setup page.

## Notes
