# 085 — Visualizer and Scene Graph in the studio shell

| | |
|---|---|
| Epic | E13 UI port |
| Depends on | 069 |
| ABI / showfile | none expected; check while implementing |

**Touches:** `app/pages/visualizer/`, `app/pages/scene_graph.rs`

## Problem
These two pages only need to match the shell, not the mockup exactly. Mockup: `design/studio`; gaps listed in `design/studio/UI_GAP.md`.

## Scope
- Page bars 40px with the shared controls (segmented, lit buttons, dropdown); tokens and status bar as everywhere.
- Keep existing canvases and editing behaviour.

## Out of scope
- Visualizer redesign (024, 025, 027).
- The kiosk layout (`Density::Compact`) stays unchanged.

## Acceptance criteria
- [ ] Matches the mockup at the same window size, both themes.
- [ ] Both pages use the shared bar and no ComboBox.

## Verification
- `cargo clippy --all-targets --all-features`, `cargo fmt`, `cargo test --all-features`.
- UI: screenshots per `crates/core/AGENTS.md` next to the mockup: both pages.

## Notes
