# 017 — Tree UI for group folders (Setup › Groups and Programmer)

| | |
|---|---|
| Epic | E4 Hierarchy (folders) |
| Depends on | 016, 006 |
| ABI / showfile | none |

**Touches:** `crates/core/src/app/pages/setup/groups.rs`, `crates/core/src/app/components/fixtures_shared.rs (group_selection)`, new: crates/core/src/app/components/tree.rs (reusable)

## Problem
Original review notes (2026-09-27):

> fixture groups should be hierarchical

## Scope
- Build a reusable collapsible tree widget (`components/tree.rs`) that fits both the 800x480 integrated layout and desktop mode. 018 reuses it.
- In Setup › Groups: create, rename and delete folders, and move groups into folders (drag or a "Move to…" dialog, since touch is primary).
- In the Programmer, group selection is shown as a tree. Tapping a folder selects all its groups (`SelectFolder`), and collapsed state is remembered per session.

## Out of scope
- Scenes and palettes (018).

## Key code
- `group_selection` — `crates/core/src/app/components/fixtures_shared.rs:577`

## Acceptance criteria
- [ ] Folders can be managed on the touch layout without a keyboard, except for naming.
- [ ] Selecting a folder selects the right groups (check with `blctl.py state`).

## Verification
- `cargo clippy --all-targets --all-features` and `cargo fmt` are clean.
- `cargo test --all-features` passes, and new logic has co-located `#[cfg(test)]` tests.
- Follow **UI Verification** in `crates/core/AGENTS.md` (check that `swaylock` is not running, launch from the repo root, navigate via blctl, capture with grim). Use the headless Xvfb recipe when clicks are needed or another session is running (see memory notes). Take before and after screenshots, keep them in the scratchpad, and never commit them.
- Screenshot Setup › Groups with nested folders, and the Programmer tree.

## Notes
<!-- Implementation notes, decisions, and follow-ups found while working. Keep this updated before renaming to .done.md. -->
