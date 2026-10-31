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
- [x] No production code under `app/pages/setup/`, `fixtures_shared.rs` or `animation_spec_editor.rs` touches
      `self.data.state` or `AppState`.
- [x] Adding fixtures in a batch (including a refused batch), renaming, deleting, folders, groups, scenes and
      animations all work as before.

## Verification
- `cargo clippy --workspace --all-targets --all-features`, `cargo fmt`, `cargo test --workspace --all-features`.
- Showfile: save, reload, `blctl state`.
- blenv: screenshots of each sub-page after an edit; one refused batch add.

## Notes

### What changed
- **New `HostCommand`s** (`hub.rs`; engine side in the new `dmx/setup_commands.rs`, dispatched from
  `DmxEngine::apply_host` with the sender's session swapped in). Every one names what it acts on by id. All
  return `true` from `edits_show()` except `InitFixtures`.
  - Groups: `CreateGroup { name, folder }` (compound: creates, then moves into `folder`; a folder it can't enter
    leaves it at the top level with a warning, as before) → `Ok { created: Group(id) }` / `Refused("Group limit
    reached")`. `RenameGroup { group, name }` (trimmed) → `Refused("Name is empty" | "Group not found")`.
    `DeleteGroup(id)` → `Refused("Group not found")`.
  - Scenes: `CreateScene { name, folder }` → `Scene(id)` / "Scene limit reached". `CloneScene { scene, name }`
    (copies that scene, not the focus; lands in its folder) → `Scene(id)` / "Scene not found" (also BLANK) /
    "Scene limit reached". `RenameScene { scene, name }` → "Name is empty" / "Scene not found".
    `DeleteScene(id)` → "Scene not found" / "At least one scene must remain"; sessions and the live preview are
    repaired by the existing `EngineState::delete_scene`.
  - Fixtures: `AddFixtures { group, fixtures }` (all or none) and `UpdateFixture { group, fixture, value }` are
    refused with the `FixtureBatchError` text. `DeleteFixture { group, fixture }`, `SwapFixtures { group, a, b }` →
    "Fixture not found". `MoveFixtureToGroup { group, fixture, target }` → `Fixture { group, fixture }` (the new
    id) / "Could not move fixture". `InitFixtures(Vec<(u8, u8)>)` → "Fixture not found" if any is missing (then
    none runs).
  - Animation templates: `CreateAnimationTemplate(spec)` → `AnimationTemplate(id)` / "Animation limit reached";
    `UpdateAnimationTemplate { id, spec }` → "Animation not found"; `DeleteAnimationTemplate(id)` → always `Ok`
    (as `delete_animation`, it also purges scene references of a missing template).
  - `CreatedId::{Group(u8), Scene(u8), Fixture { group, fixture }}` are new.
  - `HostCommand` no longer derives `PartialEq`/`Eq`: `Fixture` and `AnimationSpec` don't implement them, and
    nothing compared commands.
- **Fixture init:** `AppState::pending_fixture_inits` / `request_fixture_init` / `take_fixture_init_requests` are
  gone. `DmxEngine` keeps the queue (`pending_fixture_inits: RefCell<Vec<_>>`, `apply_host` takes `&self`) and
  `run_fixture_inits` drains it after rendering, as before.
- **Shared:** `blaulicht_shared::EngineState::new_scene` returns the new id (`Option<u8>`, as does the core
  wrapper); `clone_scene(name)` (focus-based) was replaced by `clone_scene_of(source, name) -> Option<u8>`.
  Methods only, no serialized type changed, so no ABI bump.
- **Pages.**
  - `setup_ui` takes one snapshot per frame with `show.clone_for_session(session)` (it deep-cloned the whole
    engine state before), and Palettes does the same instead of `clone_for_ui` on a read lock.
  - Animations reads templates, palettes and the session's selected fixtures in one `with_session`; its two
    `clone_animation_instance_state` calls send `HostCommand::CloneAnimationInstanceState`; the template edit sends
    `UpdateAnimationTemplate` (after the clone back, both on the bus in order). A selected template that
    disappears (deleted elsewhere) now shows "No Animation Selected" instead of panicking on `unwrap()`.
  - `group_selection` (`fixtures_shared.rs`) takes the page snapshot instead of its own read guard.
  - `setup/mod.rs` has `send_setup_command(cmd, on_ok)` (wraps `reply_handlers.send_then`; refusals show as a 5 s
    popup with OK, like the batch error before) and `setup_reply_pending()`. `BlaulichtApp::setup_reply` holds
    the last Setup command's seq.
  - Dialogs that close only on success close in the reply: batch add, group rename, scene rename, scene delete,
    animation Create and Presets. Their OK does nothing while the previous Setup command still waits for its
    reply, so a double tap doesn't send twice.
  - Dialogs that closed unconditionally still do (group create/delete, scene create/clone, fixture delete,
    template delete); a refusal now shows as a popup instead of being silent.
  - Follow-ups in replies: fixture delete selects the group's first fixture, Move Up/Down keeps the moved fixture
    selected, Move Group follows the fixture to `CreatedId::Fixture` and loads it into the editor, Initialize shows
    "Initializing …" once the engine accepted it.
  - Scene clone/rename/delete dialogs store the scene they were opened for (`scene_dialog_target`) and send its id.
- **Animation editor keepalive:** already done in 050 (`KeepAnimationEditorAlive`, resent every 100 ms);
  `animation_spec_editor.rs` has no direct state access left, so nothing changed there. The ticket's
  `EditAnimationInstance { id }` is that command. `plugin/tick.rs` needed no change.

### Decisions and deviations
- **Compound create commands** (`CreateGroup`/`CreateScene { folder }`) instead of create + `MoveToFolder` from a
  reply handler, so the move is atomic and needs no second round trip. They still go through `send_then` and
  report the new id; nothing in the desk uses the id yet (the dialogs didn't select the new item before either).
- **Scene delete floor:** the dialog now counts user scenes (`user_scenes().count() > 1`) instead of
  `scenes.len() > 1`, which counted BLANK and offered Confirm for the last user scene (the engine then silently
  refused). Verified: one user scene shows "At least one scene must remain."
- **Animation Create:** the name stays in the field until the reply (it was taken immediately and put back on
  failure); same result.
- **Remote senders:** `DeleteGroup`/`DeleteFixture` clear the *sender's* selection and stack (that is what the
  desk saw before) and repair every other session, including the desk's.
- `palettes.rs` was not in Touches but is under `app/pages/setup/`; its one read moved so the first acceptance
  criterion holds.

### Verification
- `cargo test --workspace --all-features`: all pass; blaulicht-core 301 → 309. New `dmx::setup_commands::tests`
  (two-client harness, commands sent by the remote): `a_new_group_lands_in_its_folder_and_reports_its_id` (incl. a
  missing folder), `groups_are_renamed_and_deleted_by_id` (trim, empty, missing, desk selection repaired, remote's
  cleared), `scene_commands_act_on_the_named_scene_not_the_focus` (create in folder, clone of a non-focused scene
  copies its master alpha, rename, deleting the desk's focus moves the desk only, BLANK refusals),
  `the_last_user_scene_stays`, `a_fixture_batch_is_added_whole_or_refused` (overlap refused with nothing added,
  scene states patched, missing group), `fixtures_are_updated_swapped_moved_and_deleted_by_id` (incl. overlap
  refusal and `CreatedId::Fixture`), `initializing_fixtures_queues_their_setup_run` (incl. refusal),
  `animation_templates_are_created_updated_and_deleted` (incl. limit). Test helpers: `TestEngine::engine()`,
  `DmxEngine::initializing_fixtures()` (`cfg(test)`).
- `cargo clippy --workspace --all-targets --all-features`: blaulicht-core lib 140, lib test 159 (baseline 141 /
  160; none in new code). `cargo build`: 84 core lib warnings (unchanged).
- `cargo fmt`: touched files clean; the usual unrelated hunks reverted.
- **blenv** (kiosk 800x480, copy of `Kuze_Theater.json`, screenshots read):
  - Fixtures: Spitzlicht › PAR L, Move Down → it becomes #1 and stays selected; rename to "PAR Links" + Save →
    list updates; Initialize → popup, log "Initialized fixture `PAR Links`"; Add Fixture at address 1 → refusal
    popup "New Fixture uses universe 0, channels 1-18, which overlap Linse LL", dialog stays open; address 400 →
    added (4 FX), dialog closes; Move Group → Saal: editor follows (Group #2 • Fixture #1); Delete → gone,
    selection on #0.
  - Groups: New Folder "Stage", Add Group with the folder selected → "My Group" lands in it; Rename → "Buehne",
    dialog closes; Delete → gone, "No group selected".
  - Scenes: Delete with one user scene → "At least one scene must remain."; folder "Acts", Scene + → "My Scene" in
    it; Clone → "Clone" next to it in the folder; Rename → "Act One"; delete Clone → focus moves to Empty Scene.
  - Animations: Create "Test Anim" → listed as #6; waveform Sin → Triangle + Apply (inspector state shows
    `base: Triangle`); Presets › Kick Flash → added, dialog closes; Delete → gone.
  - Palettes renders ("No palettes defined", the show has none).
  - Unsaved dot appears after the edits. Save Showfile, copy the saved file out, reload it in a fresh run: the
    saved JSON and `blctl state` contain "PAR Links", "Act One", folders "Stage"/"Acts", "Test Anim" with
    Triangle; the Scenes tab shows Acts › Act One and Empty Scene.
  - Desktop layout, Setup › Animations, new Wasm template with Chaos engine: the plugin editor UI appears and
    stays live, Warp speed drags to 255 (keepalive + editor ops work).
  - Logs: only the usual DMX/MIDI/XSETTINGS/XRandR/thread-priority lines, plus two egui "Widget rect changed id
    between passes" warnings right after deleting tree rows (not compared against a before build).
- **Observed, unchanged:** after Apply on a Chaos engine template, switching to another template and back shows
  Warp speed at its default 110 again. A before build (`bb8eb63`) does exactly the same, so it is not from this
  change (likely the plugin's state persistence; the clones are queued in the same order as before). Also, after
  deleting a fixture the editor fields keep the deleted fixture's values until another fixture is tapped (the
  old code only changed `setup_fixture_id` too).

### For 044, 045, 051, 052
- `send_setup_command` is the Setup-local shape of "send, popup on refusal, run `on_ok`"; copy it for other pages
  or lift it into `app/` if a second page wants it.
- Host commands that create things return a `CreatedId`; compound commands (create + place) avoid
  create-then-move round trips.
- Nothing in Setup reads `AppState` any more; `AppState::pending_fixture_inits` is gone.
