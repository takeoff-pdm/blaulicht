# 048 — Web frontend spike: egui on wasm or Svelte

| | |
|---|---|
| Epic | E10 Decouple |
| Depends on | 047 |
| ABI / showfile | none |

**Touches:** `crates/ui_egui` (cfg gates only), a scratch wasm crate or build target

## Problem
The epic's goal includes a web version running next to the native UI. There are two options:
- **A. egui on wasm:** compile `blaulicht-ui-egui` with `RemoteClient`. Obstacles:
  - The visualizer shaders are `#version 330` (`visualizer/shaders/*.glsl`), and WebGL2 needs `300 es`.
  - The visualizer needs a 24-bit depth buffer and 4× multisampling (`main.rs:209–210`).
  - There are 31 `Instant::now` calls in `app/`, which panic on `wasm32-unknown-unknown` without `web-time`.
  - Threads are spawned at `ui.rs:354` and `:408`.
  - `std::fs` and `sysinfo` are used (`system/showfile.rs:13`), as are `cpal` (`app/event.rs:8`),
    `fdg_sim` (`scene_graph.rs`) and egui-file-dialog.
- **B. Svelte:** `crates/core/web` has 74 `.svelte` files and was last changed 2026-06-23. It speaks the dead
  actix JSON topic protocol (`web/src/lib/websocket.ts`), so it would effectively be a rewrite.

## Scope
- Try to build `blaulicht-ui-egui` for `wasm32-unknown-unknown` with the visualizer, file dialog and
  pop-outs turned off, and list what fails and roughly how much work each item is.
- Write a recommendation (A, B or a slim A without the visualizer) with an estimate into Notes. The maintainer
  decides before 054 starts.

## Acceptance criteria
- [ ] Notes contain the build result, a blocker list with estimates and a recommendation.

## Notes
