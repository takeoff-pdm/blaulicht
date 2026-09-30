# 022 — Performance page (ViewPerformance + SceneGraph)

| | |
|---|---|
| Epic | E6 Performance |
| Depends on | 021, 005 |
| ABI / showfile | TBD by 021 |

**Touches:** `crates/core/src/app/pages/view_perf.rs`, `crates/core/src/app/pages/scene_graph.rs`, `crates/shared/src/page/mod.rs`, (refined by 021)

## Problem
Original review notes (2026-09-27):

> Merging graph and view performance into one 'Performance page'?
> Graph is a bit random.

## Scope
- **Placeholder – ticket 021 replaces this scope.** Baseline:
- A Performance page with sub-pages (e.g. Overlays/Banks and Graph) replacing the ViewPerformance and SceneGraph nav entries.
- Graph readability: a deterministic auto-layout (`auto_layout_graph`), node labels, and highlighting the active node/transition at runtime.

## Out of scope
- Grand master redesign (023).

## Key code
- `view_perf_ui` — `crates/core/src/app/pages/view_perf.rs:217`
- `render_scene_graph`, `auto_layout_graph` — `crates/core/src/app/pages/scene_graph.rs`

## Acceptance criteria
- [ ] As defined by 021.

## Verification
- `cargo clippy --all-targets --all-features` and `cargo fmt` are clean.
- `cargo test --all-features` passes, and new logic has co-located `#[cfg(test)]` tests.
- Follow **UI Verification** in `crates/core/AGENTS.md` (check that `swaylock` is not running, launch from the repo root, navigate via blctl, capture with grim). Use the headless Xvfb recipe when clicks are needed or another session is running (see memory notes). Take before and after screenshots, keep them in the scratchpad, and never commit them.
- Screenshot every Performance sub-page in both layouts.

## Notes
<!-- Implementation notes, decisions, and follow-ups found while working. Keep this updated before renaming to .done.md. -->
