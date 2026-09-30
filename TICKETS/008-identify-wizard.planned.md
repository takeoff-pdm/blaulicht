# 008 — Fixture identify wizard

| | |
|---|---|
| Epic | E2 Setup page |
| Depends on | 006 |
| ABI / showfile | none expected (use existing DMX overrides / selection events to highlight) |

**Touches:** new: crates/core/src/app/pages/setup/identify.rs, `crates/core/src/app/pages/setup/fixtures.rs`

## Problem
Original review notes (2026-09-27):

> also allow an 'identify' wizard

## Scope
- A wizard started from Setup › Fixtures (per group or for all groups). It steps through fixtures one at a time and highlights the current one: full white, open shutter and a centered position via temporary DMX overrides or a temporary scene. All other fixtures are dimmed.
- For each fixture the user can rename it (text field focused) and press Next, Prev or Skip. Show progress as k/N.
- On exit (finish or cancel), remove every temporary override. Rename changes are applied as they are made.

## Out of scope
- Automatic camera-based identification.
- Changing DMX addresses.

## Key code
- DMX overrides — `EngineState.overrides`, `render_dmx_override_dialog` (`fixtures_setup.rs:336`)
- Fixture rename events used by the fixture editor

## Acceptance criteria
- [ ] Running the wizard over a group highlights each fixture in turn (check with `blctl.py state` or the DMX simulator).
- [ ] Cancel leaves no overrides behind (`blctl.py state overrides` is empty or unchanged).

## Verification
- `cargo clippy --all-targets --all-features` and `cargo fmt` are clean.
- `cargo test --all-features` passes, and new logic has co-located `#[cfg(test)]` tests.
- Follow **UI Verification** in `crates/core/AGENTS.md` (check that `swaylock` is not running, launch from the repo root, navigate via blctl, capture with grim). Use the headless Xvfb recipe when clicks are needed or another session is running (see memory notes). Take before and after screenshots, keep them in the scratchpad, and never commit them.
- Screenshot the wizard at step 1 and step 2. Confirm the DMX values change for the highlighted fixture only.

## Notes
<!-- Implementation notes, decisions, and follow-ups found while working. Keep this updated before renaming to .done.md. -->
