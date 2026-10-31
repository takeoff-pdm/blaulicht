# 048 — Web frontend

| | |
|---|---|
| Epic | E10 Decouple |
| Depends on | 047 |
| ABI / showfile | none |

**Touches:** to be decided; either a wasm target of `crates/ui_egui` or `crates/core/web` (Svelte)

## Problem
The goal of the epic: a web version running alongside the native UI on the device.

## Scope
Decision first, written in Notes and confirmed with the maintainer before building:
- **A. egui on wasm:** compile `blaulicht-ui-egui` for `wasm32` with `RemoteClient`. Almost no new UI code, and
  it looks the same as the device. Risks: the visualizer's GL callback on WebGL2, file dialogs, plugin UI pop-out
  windows, and a large wasm bundle.
- **B. Svelte:** revive `crates/core/web`, which hasn't changed since 2026-06. A separate UI that fits a different
  use case (phone remote: faders, scenes, selection), but it means a second UI codebase to maintain.

Then build the chosen option, served by the engine's HTTP server, with at least Programmer working end to end.

## Out of scope
- Feature parity with the native UI in the first iteration.

## Acceptance criteria
- [ ] A browser on another device and the native UI control the same rig at the same time, each with its own
      selection.

## Verification
- Screenshot of both frontends side by side after each changed something.

## Notes
