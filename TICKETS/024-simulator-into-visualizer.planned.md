# 024 — Move the 2D DMX simulator into the Visualizer

| | |
|---|---|
| Epic | E7 Visualizer |
| Depends on | 005 |
| ABI / showfile | ABI bump only if new sub-page variants are added to shared (they are, via 005's mechanism) |

**Touches:** `crates/core/src/app/components/fixtures_shared.rs`, `crates/core/src/app/pages/visualizer/`, `crates/core/src/app/pages/setup/fixtures.rs`, `crates/shared/src/page/sub.rs`

## Problem
Original review notes (2026-09-27):

> fixture setup owns the simulator? - why?
> i think this should be part of the visualizer.

## Scope
- Move `render_dmx_simulation_dialog`, `simulate_dmx` and `render_mapped_ui` (`fixtures_shared.rs:289+`, state `dmx_simulator`) from a Setup dialog into a **Visualizer › DMX** sub-page. The 3D view becomes **Visualizer › 3D**.
- Remove the "Simulate" button from setup.

## Out of scope
- Visualizer bugs (025).

## Key code
- `render_dmx_simulation_dialog` — `crates/core/src/app/components/fixtures_shared.rs:289`
- `visualizer_ui` — `crates/core/src/app/pages/visualizer/ui.rs`

## Acceptance criteria
- [ ] The simulator is reachable only in the Visualizer and works as before (universe switch, live values).

## Verification
- `cargo clippy --all-targets --all-features` and `cargo fmt` are clean.
- `cargo test --all-features` passes, and new logic has co-located `#[cfg(test)]` tests.
- If the ABI was bumped: `PLUGIN_ABI_VERSION` in `crates/shared/src/abi.rs` is incremented and `cd crates/plugins && make build_noopt` was run. The Makefile reports success even on failure, so grep its output for `^error`.
- Follow **UI Verification** in `crates/core/AGENTS.md` (check that `swaylock` is not running, launch from the repo root, navigate via blctl, capture with grim). Use the headless Xvfb recipe when clicks are needed or another session is running (see memory notes). Take before and after screenshots, keep them in the scratchpad, and never commit them.
- Screenshot both Visualizer sub-pages.

## Notes
<!-- Implementation notes, decisions, and follow-ups found while working. Keep this updated before renaming to .done.md. -->
