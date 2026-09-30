# 025 — Visualizer bug triage (produces tickets)

| | |
|---|---|
| Epic | E7 Visualizer |
| Depends on | none |
| ABI / showfile | none |

**Touches:** no code (writes new TICKETS/*.planned.md)

## Problem
Original review notes (2026-09-27):

> visualizer is also a bit buggy

## Scope
- Run the app in the headless X recipe (real clicks are needed for the 3D editor) with a real showfile copy. Exercise camera, truss placement, model import, fixture inspector, undo/redo and snapping in both window layouts.
- For every bug, record the steps, expected vs actual behavior, a screenshot and the suspected code location (`crates/core/src/app/pages/visualizer/`).
- Write one new `.planned.md` ticket per bug or cluster of related bugs, sized for one session, using the next free numbers, and add them to the TODO.md index. Ask the user to confirm the list.

## Out of scope
- Fixing bugs in this ticket (unless trivial one-liners).

## Key code
- `crates/core/src/app/pages/visualizer/` (ui.rs 2.5k LOC, renderer.rs, gl.rs, state.rs, truss.rs)

## Acceptance criteria
- [ ] New bug tickets exist and are indexed. Each one has repro steps.

## Verification
- `cargo clippy --all-targets --all-features` and `cargo fmt` are clean.
- `cargo test --all-features` passes, and new logic has co-located `#[cfg(test)]` tests.
- Screenshots live in the scratchpad and are referenced by description in the tickets.

## Notes
<!-- Implementation notes, decisions, and follow-ups found while working. Keep this updated before renaming to .done.md. -->
