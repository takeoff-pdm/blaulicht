# 043 — Setup pages onto EngineClient

| | |
|---|---|
| Epic | E10 Decouple |
| Depends on | 042 |
| ABI / showfile | none, unless a new `ControlEvent` variant is needed (append at the end) |

**Touches:** `crates/core/src/app/pages/setup/fixtures.rs`, `setup/groups.rs`, `setup/scenes.rs`, `setup/animations.rs`, palettes and views setup pages, `crates/core/src/dmx/mod.rs`

## Problem
After 042, the Setup pages still write engine state directly: `fixtures.rs` (5), `scenes.rs` (4),
`groups.rs` (3), `animations.rs` (3).

## Scope
- Move every Setup sub-page onto `EngineClient`: reads from the snapshot, writes as commands handled by the
  engine. No `.write()` / `.lock()` on `AppState` remains under `app/pages/setup/`.
- Bulk edits (rename many, add a fixture with patching) become one command each, not a burst of small ones, so
  they apply atomically.

## Out of scope
- Setup redesign (037). Don't run them at the same time.

## Key code
- `grep -nE 'state\.[a-z_]+\.(write|lock)\(\)' crates/core/src/app/pages/setup`
- `groups.rs:253` — an example of read-modify-write during drawing

## Acceptance criteria
- [ ] The grep above finds nothing.
- [ ] Adding, editing and removing fixtures, groups, scenes, animations, palettes and views works as before,
      including folders and undo.

## Verification
- `cargo clippy --all-targets --all-features`, `cargo fmt`, `cargo test --all-features`.
- Showfile shape: save, reload and check with `python3 scripts/blctl.py state`.
- UI: screenshots of each sub-page after an edit.

## Notes
