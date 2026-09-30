# 021 — Design: merge ViewPerformance and SceneGraph into one Performance page

| | |
|---|---|
| Epic | E6 Performance |
| Depends on | 020 |
| ABI / showfile | none |

**Touches:** TICKETS/022-*.md (writes the spec), no code

## Problem
Original review notes (2026-09-27):

> Graph is a bit random.
> Merging graph and view performance into one 'Performance page'?
> User clarification: the biggest issue is that the graph's purpose / UX is unclear; the fix would be merging the graph and the view performance. Layout / readability is secondary.

## Scope
- Read `scene_graph.rs`, `shared/src/scene_graph.rs` (SceneGraph, TransitionCondition, GraphRuntime, SceneOverride), `view_perf.rs` and 020's bank stack. Write down what the graph does at showtime today.
- Propose one mental model that ties views, overlay scenes, banks and graph transitions together. For example: graph nodes are views or banks and edges are transitions triggered by beat, time or MIDI. Say what the performer sees and touches on the 800x480 screen.
- Produce ASCII mockups for integrated and desktop mode, the list of sub-pages, and the engine changes needed.
- Rewrite ticket 022 (and split it into 022a/b… if it exceeds one session) with concrete scope and acceptance criteria. Ask the user to review before 022 starts.

## Out of scope
- Implementation.

## Key code
- `crates/core/src/app/pages/scene_graph.rs`
- `crates/shared/src/scene_graph.rs`
- `crates/core/src/app/pages/view_perf.rs`

## Acceptance criteria
- [ ] 022 contains an approved, concrete spec.

## Verification
- `cargo clippy --all-targets --all-features` and `cargo fmt` are clean.
- `cargo test --all-features` passes, and new logic has co-located `#[cfg(test)]` tests.
- Screenshots of the current ViewPerformance and SceneGraph pages attached to the design notes (scratchpad only, not committed).

## Notes
<!-- Implementation notes, decisions, and follow-ups found while working. Keep this updated before renaming to .done.md. -->
