# 051 — Visualizer onto EngineClient; stage geometry becomes show state

| | |
|---|---|
| Epic | E10 Decouple |
| Depends on | 042 |
| ABI / showfile | showfile *shape* unchanged (the `stage` section stays where it is); the owner changes from UI to engine |

**Touches:** `crates/core/src/app/pages/visualizer/{ui,state,data}.rs`, `crates/core/src/stage.rs`, `crates/core/src/state.rs`, `crates/core/src/dmx/mod.rs`, `crates/core/src/config.rs`

## Problem
- The stage (room, trusses, models, fixture attachments) is show data, but it lives in
  `visualizer_ui_state.stage` (`visualizer/state.rs:147`).
- The UI saves it (`ui.rs:607`) and derives engine fixture positions from it every frame
  (`reconcile_truss_attachments`, `visualizer/ui.rs:756–768`).
- GLB import writes files next to the showfile on the frontend's machine (`visualizer/ui.rs:1105–1120`).
- `visualizer/ui.rs` writes engine state directly at 6 sites.

## Scope
- `StageScene` moves into engine show state (in `AppState` next to `dmx_engine`, or inside the core wrapper).
  Stage edits become `HostCommand`s, and attachment reconciliation runs in the engine when the stage or the patch
  changes.
- Model import: the frontend sends the file bytes in a command, and the engine stores them next to the showfile.
- The camera, selection highlight and editor state stay view state in the frontend.
- The showfile `stage` section is written by whoever builds the showfile. Until 052, the UI builds it by reading
  the stage through `with_show`.
- All other visualizer reads go through `with_show`. The GL render target and three-d resources stay
  frontend-local.

## Out of scope
- 024–027 (visualizer features).

## Acceptance criteria
- [x] No production code under `app/pages/visualizer/` writes engine state.
- [x] Editing trusses and attachments, and importing a model, works, and the result survives save and reload.

## Verification
- `cargo clippy --workspace --all-targets --all-features`, `cargo fmt`, `cargo test --workspace --all-features`.
- blenv: Visualizer screenshots before and after a truss edit; save, reload, screenshot.

## Notes

### What changed
- **Stage is engine show state.** `dmx::EngineState` got a fourth field, `StageState` (new
  `dmx/stage_commands.rs`): the `StageScene` behind an `Arc` (so `clone_for_ui` / `clone_for_session` copy a
  pointer), a `revision` counter, the showfile path the model assets sit next to, and the engine-only
  "where I last put each attached fixture" map. Readers: `show.stage()`, `show.stage_revision()`,
  `show.stage_showfile()`. Writers (engine side only): `load_stage(stage, showfile)` (sanitizes, places
  attached fixtures), `relocate_stage(stage, showfile)` (Save As), `reconcile_stage()`.
- **Loading:** `config::read_showfile_logic` calls `dmx.load_stage(showfile.stage, Some(file))` right after
  `load_showfile` (startup in `main.rs` and in-app Load both go through it); `close_showfile` loads an empty
  stage with no path. `LoadedShowfileState` lost its `stage` field; the UI only calls
  `VisualizerUiState::reset_stage_view()` (was `load_stage`) to drop selection, undo, caches.
- **Saving (until 052):** `BlaulichtApp::serialized_showfile` reads `self.client.with_show(|s| s.stage().clone())`;
  the showfile `stage` section has the same shape (checked in the saved JSON). Save As
  (`pages/system/showfile.rs`) reads the stage through the client, copies the assets, then writes the relocated
  stage and new path with a direct `dmx_engine.write().relocate_stage(..)`, because `save_showfile()` reads the
  stage back in the same frame (a command would arrive a tick later). That page already writes engine state
  directly in load/close; 052 removes all three.
- **New `HostCommand`s** (`hub.rs`, all `edits_show() == true`, handled in `apply_host` →
  `stage_commands.rs`):
  - `AddStageObject(StageObject)` → `CreatedId::StageObject(u64)` (new `CreatedId` variant).
  - `RemoveStageObjects(Vec<u64>)` → removes with connections and attachments (fixtures stay put); refused
    "Stage object not found" (nothing removed).
  - `PatchStage(Box<StagePatch>)` → one editor edit, applied whole on a copy then committed: `room`, `objects`
    (replace existing objects' contents), `disconnect` / `connect`, `detach` / `attach` (an attach replaces the
    fixture's previous attachment), `fixtures` (hand placements, `FixturePlacement { group, fixture, position,
    rotation }`; non-finite components keep the old value). Refused "Stage object not found", "Fixture not
    found", "Truss not found" (no truss or branch), "Trusses cannot be connected" (the sanitizer rejected a
    requested connection: occupied endpoint, profile mismatch, ...). `StagePatch::between(base, edited)`
    (`stage/mod.rs`) computes it.
  - `RestoreStage { stage: Box<StageScene>, fixtures: Vec<FixturePlacement> }` → undo/redo only.
  - `ImportStageModel { file_name, bytes: ModelBytes }` → stores `<showfile stem>.assets/<sha256>.glb` next to
    the engine's showfile (same location/naming as before), dedups by hash, adds an `ImportedModel` object
    named after the file → `CreatedId::StageObject`. Refused "Save the showfile before importing models" or the
    install error. `ModelBytes(Arc<[u8]>)`: the bus clones a pointer per connection, and its `Debug` (used by the
    bus `Display`/trace) prints only the size.
  - `RemoveStageAssets(Vec<u64>)` → removes models and the objects showing them ("Remove from Stage" in the
    model-load error popup); refused "Model not found".
- **Reconciliation in the engine.** `EngineState::reconcile_stage` is the old per-frame
  `reconcile_truss_attachments` moved: put every attached fixture at its mount; drop an attachment whose fixture
  is gone or was moved by something else since the engine last placed it (same 0.001 tolerance). It runs after
  every stage command, after `load_stage`/`RestoreStage` (with the placement map cleared, like the old
  `attachment_world_cache.clear()`), and in `dispatch` after every successful host command with
  `HostCommand::changes_patch()` (`DeleteGroup`, `AddFixtures`, `UpdateFixture`, `DeleteFixture`,
  `SwapFixtures`, `MoveFixtureToGroup`). So a Setup patch edit is reconciled at once, not only while the
  Visualizer page happens to be drawn (which was the old, implicit behaviour). No `ControlEvent` changes fixture
  positions or the patch (checked), so nothing else needs it.
- **Model import split** (`stage_assets.rs`): `read_glb_for_import(path)` (frontend worker thread: name, size,
  header, full `load_prepared_model` parse, returns name + bytes) and `install_glb(name, bytes, showfile)`
  (engine: name/size/header check, hash, atomic write). `import_glb` is gone.
- **Geometry moved engine-side:** `app/pages/visualizer/math.rs` → `stage/math.rs` (now `pub`; the visualizer's
  `math.rs` re-exports it), and the pure truss geometry (branches, endpoints, bounds, `world_*`, mounts) from
  `visualizer/truss.rs` → `stage/geometry.rs`, plus `attachment_pose(stage, attachment)`. `stage.rs` became
  `stage/mod.rs`. Mesh generation for the renderer stays in `visualizer/truss.rs`.
- **Visualizer (frontend):**
  - `VisualizerUiState.stage` is now the editor's **working copy**, with `stage_base` (last sent/received),
    `stage_revision`, `stage_reply`, `fixture_drafts` / `fixture_drafts_sent`. `sync_visualizer_stage()` (start of
    each frame) replaces it with the engine's stage when the revision changed, except while a drag runs
    (`editor.drag_before`) or the reply to its own last stage command is outstanding; after any reply it re-copies
    (so a refusal reverts the working copy) and clears the fixture drafts.
  - The existing editing code (move/rotate/scale, snap, truss endpoint snapping, connected-truss rotation,
    inspector) still mutates the working copy; at the old undo points `commit_visualizer_stage()` sends
    `StagePatch::between(base, working)` plus changed fixture drafts as one `PatchStage`. Drags send once, on
    release; inspector fields send on each change (absolute values, so nothing is lost to the one-tick delay).
  - Fixture moves write `fixture_drafts` (display overrides) instead of the engine; drawing uses
    `data::fixture_pose(stage, drafts, key, fixture)`: draft → mount on the working copy's truss → engine
    position. So attached fixtures follow a truss during a drag (verified), before the engine has it.
  - Add / Duplicate send `AddStageObject` (the reply selects the new id); Delete sends `RemoveStageObjects`
    (removed locally at once); import sends `ImportStageModel` from the worker's bytes (reply selects and records
    the undo step); "Remove from Stage" sends `RemoveStageAssets`. Refusals use 044's `send_command` popup.
  - Reads: `collect_fixtures(show, universes, stage, drafts, model_cache)` runs inside one `with_show`; the 10
    universe buffers are copied first with `with_dmx_universe` (no nested `with_*`). Selection list, top view,
    inspectors and the undo snapshot read through `with_show`. The showfile path for model loading, the import
    dialog's start folder and the Import button's enabled state come from `show.stage_showfile()` instead of
    `data.config`. Nothing under `app/pages/visualizer/` touches `self.data` any more.
  - Camera, selection, editor state, undo stack, GL targets, three-d resources and the model cache stay local.

### Decisions and deviations
- **Undo = whole-stage replace** (`RestoreStage` with the snapshot's stage and every fixture's pose), as the
  ticket allows: the engine changes the stage only by command (reconcile can drop an attachment, but only in
  response to a command or load), never per tick, so replacing it loses no engine-side update. The undo stack
  stays UI-side and stores the same snapshots as before. Normal edits are granular (`PatchStage` carries only
  what changed, so a concurrent edit of another object survives).
- **Working copy + diff** (like `SceneGraphUI::sync` in 044) instead of rewriting the editor's geometry code as
  engine commands: the snapping / connected-truss logic stays where it was, operating on the copy.
- **Swap and move keep attachments** (`swap_stage_fixtures` / `move_stage_fixture` rekey the attachment and the
  placement map). Before, a swap or move gave the attachment key to another (or no) fixture, so the next
  per-frame reconcile detached it (if the Visualizer was open). Reordering ids no longer drops truss
  attachments; deleting still does.
- **The engine doesn't re-parse imported models**: it checks name, size and glTF header, hashes and writes; the
  full parse (seconds for DamagedHelmet in a debug build) runs on the frontend's worker thread before sending
  and again in the frontend's loader. Hashing and writing a model (a few MB) happens on the engine thread during
  the command, a one-off hiccup while editing.
- **Room edits** commit only on change (the old code assigned the sliders into the stage every frame); the room
  still has no undo, as before.
- **Remote frontends:** models are loaded by path relative to `stage_showfile()`, so a remote frontend can't show
  imported models yet (needs an asset stream; 047/054).
- Outside Touches: `hub.rs`, `dmx/{session,management}.rs` (fourth tuple field), `stage_assets.rs`,
  `pages/system/showfile.rs`, `app/ui.rs`, `crates/core/AGENTS.md` (module description).

### Verification
- `cargo test --workspace --all-features`: all pass; blaulicht-core 321 → 332. New: `dmx::stage_commands::tests`
  (two-client harness, commands from remote and desk): `stage_objects_are_added_edited_and_removed_by_id`,
  `an_attached_fixture_hangs_on_its_truss_and_follows_it` (incl. the three patch refusals with unchanged
  revision, detach leaves the fixture in place), `moving_an_attached_fixture_elsewhere_takes_it_off_its_truss`
  (Setup `UpdateFixture` keeping vs. changing the position; hand placement with a NaN component),
  `patch_changes_from_setup_keep_attachments_in_step` (swap, move to group with `CreatedId::Fixture`, delete
  fixture, delete group, add fixtures), `trusses_connect_only_where_they_fit`,
  `undo_restores_the_whole_stage_and_the_fixtures`,
  `a_model_is_stored_next_to_the_showfile_and_removed_with_its_objects` (Box.glb; no-showfile and bad-file
  refusals), `a_loaded_stage_hangs_its_fixtures_and_drops_missing_ones` (also: UI snapshot shares the stage
  `Arc`); `stage::tests::a_patch_holds_only_what_changed`;
  `stage_assets::tests::an_installed_model_lands_next_to_the_showfile_once`;
  `config::tests::a_loaded_showfile_hands_its_stage_to_the_engine` (load and close).
- `cargo clippy --workspace --all-targets --all-features`: blaulicht-core lib 131, lib test 149 (after 044: 134 /
  152). Two new `should_implement_trait` hits from making `Vec3::add/sub` public got an `allow` with a reason.
  `cargo build`: 83 lib warnings (was 84).
- `cargo fmt`: touched files clean; the usual unrelated hunks reverted.
- **blenv** (`--desktop`, copies of `Kuze_Theater.json`, which has no stage; screenshots read):
  - Edit › Add › Truss › Square › Straight: "Truss 1" appears and is selected (reply). Inspector Y → 4.00 m.
  - Selection list: Linse LL + shift Truss 1 → Attach: inspector `pos (0, 4, 0)`, `rotation.x 180` (engine
    reconcile). Inspector X → 3: Linse LL moves to x 3 with it (blctl state).
  - Top view, drag the attached truss: mid-drag the fixture is drawn on the moving truss while the engine still
    has the old position; on release the truss snaps (4.00, -3.25) and the engine fixture follows. Undo → back
    to (0, -3.5) for truss and fixture; Redo → (4, -3.25).
  - Import GLB: dialog opens in the engine's showfile folder; DamagedHelmet.glb (3.7 MB) → stored as
    `/state/showfiles/after.assets/<sha256>.glb`, object "DamagedHelmet.glb" selected; moved/scaled ×3 → the
    helmet renders. Room width → 14 m.
  - Autosave ran on these edits (`edits_show`); Save Showfile: the JSON's `stage` has room 14, truss, model
    asset, attachment (same shape as before).
  - Restart with the saved file: Visualizer shows room 14 m, helmet, truss with Linse LL at (4, 4, -3.25). Close
    Showfile → empty default stage; Load Showfile (in-app) → everything back.
  - Compared against a before build (`e309d0f`): the same Select-tool drag of a fresh truss ends identically
    (snapped to (-0.00, -3.50), still selected).
  - Logs: only the usual DMX/MIDI/XSETTINGS/XRandR/thread-priority lines.
  - Observed, pre-existing (same in the before build): a drag on a thin truss in the top view only grabs it if
    the pointer is still on it when egui decides it's a drag (press-and-hold works); the inspector row re-wraps
    while X changes width, shifting the canvas a few pixels during a drag; the panel's scrolled content can
    overlap the dock tab bar.

### For 052, 053, 047/054
- 052: `serialized_showfile` and Save As are the last UI reads/writes of the stage outside the visualizer
  (`relocate_stage` direct write in `pages/system/showfile.rs`); when the engine builds the showfile, read
  `EngineState::stage()` there and drop that write. `StageState.showfile` duplicates `config.last_open_showfile`
  for the engine; merge them when config moves into the engine.
- 053: `crate::stage` (scene, math, geometry) and `stage_assets::{install_glb, resolve_asset,
  copy_assets_for_save_as}` are engine code; `stage_assets::{load_prepared_model, read_glb_for_import}` and the
  three-d types are frontend-side (the file mixes both and depends on three-d).
- 047/054: `HostCommand::ImportStageModel` carries raw bytes (`ModelBytes`); a websocket transport needs a binary
  frame for it. Remote visualizers also need the model files (asset fetch by content hash).
