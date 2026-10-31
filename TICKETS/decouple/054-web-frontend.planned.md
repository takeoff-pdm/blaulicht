# 054 — Web frontend

| | |
|---|---|
| Epic | E10 Decouple |
| Depends on | 048 (and the maintainer's decision recorded there) |
| ABI / showfile | none |

**Touches:** decided in 048

## Problem
The goal of the epic: a web version running alongside the native UI on the device.

## Scope
- Build the option chosen in 048, served by the engine's HTTP server on the configured port, with at least
  Programmer (selection, attributes, palettes) working end to end.

## Out of scope
- Feature parity with the native UI.

## Acceptance criteria
- [ ] A browser on another device and the native UI control the same rig at the same time, each with its own
      selection.

## Verification
- Screenshots of both frontends side by side after each changed something.

## Notes
