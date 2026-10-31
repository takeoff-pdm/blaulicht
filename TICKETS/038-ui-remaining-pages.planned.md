# 038 — Remaining pages in the Console style

| | |
|---|---|
| Epic | E9 Console UI |
| Depends on | 033, 034, 035 |
| ABI / showfile | none |

**Touches:** `crates/ui_egui/src/app/pages/system/*`, `audio.rs`, `view.rs`, `view_perf.rs`, `scene_graph.rs`, `visualizer/ui.rs`, `crates/ui_egui/src/app/plugin_ui.rs` (chrome only), `components/log.rs`

## Problem
After 036 and 037 the other pages still use the old buttons, literals and dialogs. The design system **Blaulicht Console** (https://claude.ai/artifact/ChF4HkbnuyT8bycizagoja) applies to every page.

## Scope
- Move System tabs, Audio, View, View Performance, Scene graph, Logs and the visualizer's controls to PageHeader, ListRow, Button variants and the overlay component.
- Remove remaining colour and size literals from these pages; leave the visualizer's 3D rendering colours alone (they are data).
- Plugin UI windows: frame and title in the Console style; plugin content is untouched.
- If this is too big for one session, split it per page as `TICKETS/README.md` describes.

## Out of scope
- Visualizer rendering, plugin-drawn content.

## Key code
- Page files — `crates/ui_egui/src/app/pages/`
- Plugin windows — `crates/ui_egui/src/app/plugin_ui.rs`

## Acceptance criteria
- [ ] No page uses `ButtonSize` or the old `Dialog` frame colours.
- [ ] `rg 'Color32::from_rgb|vec2\(' crates/ui_egui/src/app/pages` shows only data colours and intentional layout values, each with a comment.

## Verification
- `cargo clippy --workspace --all-targets --all-features`, `cargo fmt --all --check` and `cargo test --workspace --all-features` are clean (run them in the blenv container).
- Follow the `blaulicht-env` skill: run the app in blenv (`scripts/blenv run` and `run --desktop`), drive it with real clicks, and look at every screenshot yourself. Take before and after shots; keep them in `.blenv/shots/` and never commit them.
- Compare against the design system's component previews and rules (Blaulicht Console, linked under Problem).
- Before and after screenshots of each converted page at 800 × 480 and on desktop.

## Notes
<!-- Implementation notes, decisions, and follow-ups found while working. Keep this updated before renaming to .done.md. -->
