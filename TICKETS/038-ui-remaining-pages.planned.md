# 038 — Remaining desktop pages in the studio style

| | |
|---|---|
| Epic | E9 Console UI |
| Depends on | 058, 059 |
| ABI / showfile | none |

**Touches:** `app/pages/system/*`, `audio.rs`, `view.rs`, `view_perf.rs`, `scene_graph.rs`, `visualizer/ui.rs`, `app/plugin_ui.rs`, `app/components/desktop_nav.rs`, `navbar.rs`

## Problem
The other desktop pages should follow the agreed studio design.

## Scope
- General: Engine, About, Show, Problems with named actions, Patch, Application with Shut down set apart.
- Plugins: list with Reload on crash, and the plugin UI embedded with a Pop out button.
- MIDI & Serial, with Reconnect. Screens, Remote UI (turning access off warns about dropped clients), Logs, Audio.
- Views (renamed from Scene Stacks on 2026-11-11) and Perform on the original layouts, with Grand Master, Set to 100% and Blackout spaced apart. Removing an overlay takes two presses.
- Scene Graph and Visualizer controls. Leave 3D rendering and plugin-drawn content alone.
- Renames as in `RULES.md`, with the sidebar heading "Live". Check page names in `crates/shared/src/page/` still parse, and append rather than reorder any enum.
- Split per page if too big.

## Out of scope
- DMX and Art-Net (062). The kiosk.

## Acceptance criteria
- [ ] Each problem has an action that reaches the fix.
- [ ] A crashed plugin can be reloaded from General and Plugins.
- [ ] Remote UI off warns and asks first.

## Verification
- `cargo clippy --all-targets --all-features`, `cargo fmt`, `cargo test --all-features`.
- UI: run with `--desktop` via `scripts/blenv` and compare screenshots against the mockup, in dark and light theme, at 1280x800 and 2560x1440. Confirm the 800x480 kiosk is unchanged.

## Notes
Mockups: `system.html`, `pages.html`. The Visualizer mockup is a 2D stand-in, so judge it against the real page. Remaining dummy actions in the mockup keep their current app behaviour.
