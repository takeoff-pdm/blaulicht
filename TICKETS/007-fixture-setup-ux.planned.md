# 007 — Fixture setup UX: add-fixture affordance, default names, bulk rename

| | |
|---|---|
| Epic | E2 Setup page |
| Depends on | 006 |
| ABI / showfile | none expected (uses existing rename events; add a batch rename event only if needed, then ABI bump) |

**Touches:** `crates/core/src/app/pages/setup/fixtures.rs (after 006)`, `crates/shared (fixture naming helper, if any)`

## Problem
Original review notes (2026-09-27):

> fixture setup add fixture should just be gray, but still clickable when no group is selected
> also in fixture setup / group setup, allow bulk rename / use better default names for fixtures

## Scope
- "Add Fixture" button: draw it gray when no group is selected, but keep it clickable. Clicking shows the existing "No Group Selected" hint (today it looks enabled and only then pops up, `fixtures_setup.rs:940-958`).
- Better default names in `add_fixture_batch_to_group`: `<fixture type> <n>` numbered within the group, instead of the current default.
- Bulk rename for a group's fixtures: a pattern with `{n}` and `{type}` placeholders, start number and step, with a live preview list before applying. Apply it as one `Transaction`.

## Out of scope
- Identify wizard (008).

## Key code
- `render_add_fixture_dialog`, `add_fixture_batch_to_group`, `can_create` — `fixtures_setup.rs:433-700` (moved by 006)

## Acceptance criteria
- [ ] Add button is visibly gray with no group selected, and clicking it explains why.
- [ ] Adding 4 fixtures creates sensibly numbered names.
- [ ] Bulk rename previews and applies in one step, and undo/redo (if any) treats it as one change.

## Verification
- `cargo clippy --all-targets --all-features` and `cargo fmt` are clean.
- `cargo test --all-features` passes, and new logic has co-located `#[cfg(test)]` tests.
- If the ABI was bumped: `PLUGIN_ABI_VERSION` in `crates/shared/src/abi.rs` is incremented and `cd crates/plugins && make build_noopt` was run. The Makefile reports success even on failure, so grep its output for `^error`.
- Follow **UI Verification** in `crates/core/AGENTS.md` (check that `swaylock` is not running, launch from the repo root, navigate via blctl, capture with grim). Use the headless Xvfb recipe when clicks are needed or another session is running (see memory notes). Take before and after screenshots, keep them in the scratchpad, and never commit them.
- Screenshot the gray button, the bulk-rename preview, and the result.

## Notes
<!-- Implementation notes, decisions, and follow-ups found while working. Keep this updated before renaming to .done.md. -->
