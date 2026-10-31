# 043 — Setup pages onto EngineClient

| | |
|---|---|
| Epic | E10 Decouple |
| Depends on | 042 |
| ABI / showfile | none (new operations go into `HostCommand`) |

**Touches:** `crates/core/src/app/pages/setup/{mod,fixtures,groups,scenes,animations}.rs`, `crates/core/src/app/components/fixtures_shared.rs`, `crates/core/src/app/pages/animation_spec_editor.rs`, `crates/core/src/dmx/mod.rs`, `crates/core/src/plugin/tick.rs`, `crates/core/src/state.rs`

## Problem
Setup pages write engine state directly:
- `fixtures.rs`, 6 sites: `:134`, `:673`, `:969`, `:996`, `:1012`, `:1062`.
- `scenes.rs`, 4 sites; `groups.rs`, 3 sites (`:296` is a read-modify-write); `animations.rs`, 3 sites.
- Hidden writes through helper methods:
  - `request_fixture_init` (`fixtures.rs:1093`, `pending_fixture_inits`).
  - `clone_animation_instance_state` (`setup/animations.rs:485`, `:523`).
  - `animation_spec_editor.rs:306`, which renews a 250 ms lease in `wasm_animation_editor_requests` every
    frame.

Several flows need a reply:
- Batch add shows a popup and closes the dialog only on success (`fixtures.rs:965–985`).
- Some flows need the id of what was just created: create a group then move it to a folder (`groups.rs:296–307`),
  diffing scene keys (`scenes.rs:299`), `animations.rs:111`.
- Rename closes only on success (`groups.rs:360`, `scenes.rs:367`).
- Scene rename and delete use `current_scene_focus` (`scenes.rs:365`, `:403`).

## Scope
- All reads go through `with_show`, and all writes are commands (existing `ControlEvent`s where they fit,
  otherwise `HostCommand`s).
- Create-then-use flows use either `CommandResult::Ok { created }` or compound commands
  (`CreateGroup { name, folder }`). Bulk edits use `Transaction` (`dmx/mod.rs:753`) or one `HostCommand`, so they
  apply atomically.
- Commands that act on a scene carry its id explicitly, instead of relying on focus.
- `pending_fixture_inits` becomes a `HostCommand::InitFixtures`.
- The animation editor lease becomes a subscription command (`HostCommand::EditAnimationInstance { id }`,
  renewed at most every 100 ms, expiring as today).
- Read guards held while drawing (`fixtures_shared.rs:351`, `:488`, `scenes.rs:384`) become scoped
  `with_show` blocks.

## Out of scope
- Setup redesign (037). There is no Setup undo today; don't add one.

## Acceptance criteria
- [ ] No production code under `app/pages/setup/`, `fixtures_shared.rs` or `animation_spec_editor.rs` touches
      `self.data.state` or `AppState`.
- [ ] Adding fixtures in a batch (including a refused batch), renaming, deleting, folders, groups, scenes and
      animations all work as before.

## Verification
- `cargo clippy --workspace --all-targets --all-features`, `cargo fmt`, `cargo test --workspace --all-features`.
- Showfile: save, reload, `blctl state`.
- blenv: screenshots of each sub-page after an edit; one refused batch add.

## Notes
