# 031 — Folder follow-ups (ids, undo, order, names) and the unloadable Kuze showfile

| | |
|---|---|
| Epic | E4 Hierarchy (folders) |
| Depends on | 016 |
| ABI / showfile | **ABI bump** (`CreateFolder` changes shape, `Folder` gains a field) and **showfile shape** change (new field needs `#[serde(default)]`) |

**Touches:** `crates/shared/src/folder.rs`, `crates/shared/src/event.rs`, `crates/shared/src/abi.rs`, `crates/core/src/dmx/mod.rs`, `crates/core/src/dmx/state.rs`, `Kuze_Theater*.json`

## Problem
Ticket 016 left these open (see its Notes):

> Folder events return no rollback event (`(err, None)`), so they don't take part in undo-by-rollback
> There is no explicit sort order inside a folder.
> Folder names are not checked: empty and duplicate names are allowed.
> `Kuze_Theater.json` in the repo root no longer loads (old animation value shape); only the `*.migrated.json` copies do.

There is also a related gap. `CreateFolder` doesn't tell the sender which id the new folder got, so the UI has to predict it with `next_folder_id`. That prediction is racy if two senders, such as the UI and a plugin, create folders at the same time.

The 017/018 tree UIs need stable ids, undo and a deterministic order. This ticket should land before them, so their UI doesn't get built around the gaps.

## Scope
1. **Explicit ids.** Change `CreateFolder` to carry the id: `CreateFolder { id: FolderId, kind, parent, name }`.
   - The sender picks the id with `folder::next_folder_id`.
   - The engine rejects an id that is already taken.
   - This also makes `CreateFolder` usable as the rollback of `DeleteFolder` (item 2).
2. **Undo.** Every folder event returns a rollback event, like the selection and override arms in `dmx/mod.rs`:
   - `CreateFolder` → `DeleteFolder(id)`.
   - `RenameFolder` → `RenameFolder(id, old_name)`.
   - `MoveToFolder` → `MoveToFolder` back to the previous folder (`item_folder`).
   - `DeleteFolder` → a `Transaction` that recreates the folder with the same id, then moves every reparented child folder and item back into it.
   - `SelectFolder` → only the group ids it actually added, deselected again (one `DeSelectGroup` each, wrapped in a `Transaction`).
3. **Sort order.** Add `order: u16` to `Folder` with `#[serde(default)]`, and a `ReorderFolder(FolderId, u16)` event appended at the end of `ControlEvent`.
   - Siblings sort by `(order, name)`.
   - Ordering items (groups, scenes, palettes) inside a folder is out of scope. Write in Notes whether it's needed.
4. **Name rules.** `create_folder` and `rename_folder` trim the name and reject an empty one. They also reject a name that a sibling of the same kind already uses (same parent), compared case-insensitively.
   - `repair_folders` doesn't rename existing duplicates in old files. It only logs them.
5. **Kuze showfile.**
   - First check on plain v3 (before 016) whether `Kuze_Theater.json` loads, and note the result. It fails at line 451 (`"amplitude_min": 0`, the old pre-`FixtureValue` shape).
   - Then do one of the following, and record which in Notes:
     - **(a)** Add a serde compatibility shim, so the old shape still loads. Use this only if real shows in that shape still exist.
     - **(b)** Replace `Kuze_Theater.json` with its migrated version and remove the stale copies (`Kuze_Theater.migrated.json`, `Kuze_migrated.json`), if they're the same show.
   - Ask the maintainer before deleting a showfile.

## Out of scope
- Any tree UI (017, 018).
- Ordering items inside a folder (see Scope 3).
- The broken absolute path dependency in `crates/plugins/pioneer_pro_dj_link/Cargo.toml` (separate Inbox item).

## Key code
- Folder logic and tests: `crates/shared/src/folder.rs` (`next_folder_id`, `create_folder`, `repair_folders`).
- Folder events: `crates/shared/src/event.rs`, `CreateFolder` through `SelectFolder`, appended after `AnimationPluginUi`.
- Engine arms (currently `(err, None)`): `crates/core/src/dmx/mod.rs`, from the `// Folders:` comment.
- Rollback pattern to copy: the `SelectGroup` / `SetChannelOverride` arms in `crates/core/src/dmx/mod.rs` (~line 775–840).
- `Transaction` handling: `crates/core/src/dmx/mod.rs` (~line 752).
- Showfile loading and `repair_folders` call: `dmx::state::EngineState::load_showfile`.

## Acceptance criteria
- [x] For every folder event, applying it and then its rollback leaves `EngineState` unchanged. Unit tests cover this, including deleting a folder that has nested child folders and items of its kind.
- [x] `CreateFolder` with a taken id, an empty name or a duplicate sibling name is rejected without changing the state.
- [x] Siblings sort by `(order, name)`. A showfile without `order` still loads.
- [x] `Kuze_Theater.json` loads with its real groups and scenes (`blctl state`), or has been replaced by a migrated file that does. The choice is recorded in Notes. Option (b), applied after maintainer approval (see Notes).
- [x] ABI bumped and plugins rebuilt.

## Verification
- `cargo clippy --all-targets --all-features` and `cargo fmt` are clean.
- `cargo test --all-features` passes, and new logic has co-located `#[cfg(test)]` tests.
- If the ABI was bumped:
  - `PLUGIN_ABI_VERSION` in `crates/shared/src/abi.rs` is incremented.
  - `cd crates/plugins && make build_noopt` was run. The Makefile reports success even on failure, so grep its output for `^error`. `pioneer_pro_dj_link` is known to fail; see Out of scope.
- Showfile compatibility: load a **scratch copy** of an existing showfile, including one saved by 016 with folders, and confirm with `python3 scripts/blctl.py state` that it isn't a default show that booted silently.
- End to end in blenv (`scripts/blenv run --showfile …`, then `blenv blctl event …`):
  - create, rename, move, reorder and delete folders, and check the result with `blctl state`;
  - check that an undo restores the earlier state.

## Notes
<!-- Implementation notes, decisions, and follow-ups found while working. Keep this updated before renaming to .done.md. -->

### API for 017/018
All logic is still in `crates/shared/src/folder.rs`; the engine has one arm for all folder events in
`crates/core/src/dmx/mod.rs` that forwards to `EngineState::apply_folder_event`.

- Events (`ControlEvent`):
  - `CreateFolder { id: FolderId, kind: FolderKind, parent: Option<FolderId>, name: String }`. The
    sender picks `id` with `folder::next_folder_id(&state.folders)`. JSON:
    `{"CreateFolder":{"id":4,"kind":"Groups","parent":null,"name":"Front"}}`.
  - `ReorderFolder(FolderId, u16)`, appended at the end of `ControlEvent`. JSON: `{"ReorderFolder":[4,2]}`.
  - `RenameFolder`, `DeleteFolder`, `MoveToFolder`, `SelectFolder` keep their 016 shapes.
- `Folder` gained `order: u16` (`#[serde(default)]`). New folders get 0.
- Rollbacks. The engine returns `(None, Some(rollback))` for every successful folder event, and the
  tick loop sends it on the bus with originator `DmxEngine` (visible in `blctl tap`). A failed folder
  event returns `(Some(err), None)` and changes nothing. The same rollback can be computed before
  sending, read-only, with `EngineState::folder_rollback(&event)`, so a UI can push it on its undo
  stack from its read lock.
  - `CreateFolder { id, .. }` -> `DeleteFolder(id)`.
  - `RenameFolder(id, _)` -> `RenameFolder(id, old_name)`.
  - `ReorderFolder(id, _)` -> `ReorderFolder(id, old_order)`.
  - `MoveToFolder { item, .. }` -> `MoveToFolder { item, folder: <previous folder> }`.
  - `DeleteFolder(id)` -> `Transaction[CreateFolder { same id, kind, parent, name },
    ReorderFolder(id, order) (only if order != 0), MoveToFolder { child, Some(id) } for every child
    folder (by id) and then every item of its kind (groups / scenes / palettes by id)]`.
    Special case: if a child folder has the deleted folder's name, it sits next to the recreated
    folder until it is moved back, so the folder is recreated as `"<name> (n)"` (first free n) and a
    final `RenameFolder(id, name)` restores the name.
  - `SelectFolder(id)` -> `Transaction[DeSelectGroup(g) for each group it actually added (ascending),
    then LimitSelectionToFixtureInCurrentGroup(f) for each fixture of the filter it cleared]`. The
    filter part goes beyond the ticket text: without it the rollback would not restore the state
    when a single group with a fixture filter was selected.
- Sort rule: siblings sort by `(order, name)`; the name is compared trimmed and lowercased, and the id
  breaks remaining ties (duplicate names in old files), so the order is total. Helpers:
  `folder::sorted_children(&folders, kind, parent) -> Vec<FolderId>` and
  `folder::sorted_folder_ids(&folders, kind) -> Vec<FolderId>` (every folder of a kind, siblings in
  sort order; feed it to the 030 tree widget, which keeps input order per parent). Items inside a
  folder have no order (see below).
- Name rules (`create_folder`, `rename_folder`, and `move_to_folder` for folders): the name is
  trimmed; errors are `"Folder name is empty"` and `"A sibling folder already has this name"` (same
  kind, same parent, compared trimmed and case-insensitively; a folder never clashes with itself, so
  a case-only rename works). Other errors: `"Folder id already taken"`, `"Folder not found"`,
  `"Folder holds a different kind of item"`, `"Item not found"`,
  `"Folder cannot move into itself or a subfolder"`, `"Not a groups folder"`,
  `"Folder contains no groups"`, `"Already selected"`, `"Not a folder event"`.
  `EngineState::check_folder_name(kind, parent, name, folder) -> Result<String, &str>` validates
  input before sending; `folder::normalize_folder_name` and `folder::folder_names_equal` are public.
- `repair_folders` doesn't rename anything new. It logs each empty name and each set of equally
  named siblings (`EngineState::folder_name_problems`) with `tracing::warn!`.

### Decisions
- `MoveToFolder` for a folder also checks the sibling-name rule at the target. Without it a move would
  bypass the rule that create and rename keep, and 017/018 drag-and-drop would produce duplicates.
- `DeleteFolder` is never rejected for names. Its children move up and may end up next to an equally
  named sibling (e.g. `A/X` and `X`, delete `A`). That is allowed, like duplicates in old files.
- Rollbacks are exact for states that follow the name rules, and the unit tests cover that. In an
  old file with duplicate or empty names, a rollback that has to recreate or move a folder into such
  a clash is rejected by the name rules as a whole (the engine's `Transaction` restores its
  snapshot), so nothing half-applies.
- Rollbacks on success are new for the engine: the older arms only return a corrective event on
  error. Nothing in the repo consumes the `DmxEngine` events except the app's dirty flag and the
  inspector tap, and the engine ignores `DmxEngine`-originated events, so the broadcast rollback is
  never applied by itself.
- Applying a plain rollback (`RenameFolder`, `ReorderFolder`, `MoveToFolder`, `DeleteFolder`) again
  returns its own rollback, which is the original event (a redo; the redo of `CreateFolder` arrives as
  `Transaction[CreateFolder]`). The engine's `Transaction` arm returns `None` on success, so the
  `Transaction` rollbacks of `DeleteFolder` and `SelectFolder` return no redo. A UI that wants redo
  should keep the original event rather than rely on the engine's answer.
- Ordering items (groups, scenes, palettes) inside a folder: not needed for 017/018. Groups, scenes
  and palettes already have stable numeric ids that the current pages sort by, and the tree widget
  shows leaves in input order. If manual item order is wanted later, it needs an `order` on
  `FixtureGroup` / `Scene` / `Palette` (another ABI bump), not on `Folder`.
- ABI: `PLUGIN_ABI_VERSION` is **17**. v3 is at 15 and ticket 006 runs in parallel with 16; the
  integrator renumbers at merge time if needed.

### Kuze showfile (scope 5): recommend (b), not done, needs maintainer approval
Nothing was deleted or overwritten; the acceptance box stays unticked until the maintainer decides.

Evidence (from git and the files, no old build):
- `Kuze_Theater.json`, `Kuze_Theater.migrated.json` and `Kuze_migrated.json` were all added in the
  same commit, `95f04c2` (2026-08-18, message `.`). The two migrated files are byte-identical.
- `amplitude_min` became `FixtureValue` in `5eed96e feat: Palettes` (2026-06-26); before that it was
  `u16`. `5eed96e` is an ancestor of `95f04c2`, and at `95f04c2` (and on v3 before 016) the field is
  `FixtureValue` with no compatibility deserializer. So `Kuze_Theater.json` could not load on plain v3
  either, nor on the commit that added it; it is the pre-migration source kept next to its migrated
  output. I did not build an old commit to confirm this.
- On this branch, `blenv run --showfile` with a copy of `Kuze_Theater.json` logs
  `Failed to read showfile …: expected value at line 451 column 38` and boots a default show.
- A semantic diff (old file vs migrated file, with `{"Literal": n}` unwrapped) finds exactly two
  shape changes and nothing else: `amplitude_min`/`amplitude_max` of the 6 animations (bare number ->
  `{"Literal": n}`), and in all 26 scene fixture states `color {h,s,v}` + `orientation {pan,tilt}` ->
  `color_h/color_s/color_v/pan/tilt` (plus `alpha`, `strobe_speed`, `focus` wrapped in `Literal`).
  Same groups (Frontlicht, Spitzlicht, Saal, Par Publikum, Fluter, Par Truss, Par Front), same
  scene (`Empty Scene`), same patch, UI and plugin state. So it is the same show.
- The colour conversion is lossy and not obvious (`h: 196.94` (float) -> `Literal(196)`,
  `s: 1.0` -> `255`), so a shim (a) would have to re-implement an old migration for two shapes,
  `FixtureValue` numbers and the old fixture-state layout. `17.01.json` is the only other repo file
  in that shape (bare `amplitude_min`, 476 `orientation` entries), and it has a migrated sibling,
  `17.01_migrated.json`. I didn't diff that pair. No showfile in the repo needs the shim.
- `Kuze_Theater.migrated.json` loads on this branch with all 7 groups and `Empty Scene` (see
  Verification).

Recommendation (b): replace `Kuze_Theater.json` with `Kuze_Theater.migrated.json` (`git mv`) and
delete `Kuze_migrated.json`. Consider the same for `17.01.json`. Needs the maintainer's OK.

### Verification
Everything ran in the blenv container `blenv-agent-aea950689efc0dfab-d72ae592`, never on the host.
- `cargo test --workspace --all-features`: exit 0 (shared 69 passed, core 200 passed, the rest
  pass or are empty). New tests:
  - `crates/shared/src/folder.rs`: 19 tests. Among them, `every_folder_event_round_trips_through_its_rollback`
    covers create, rename, reorder, moves of every item kind and of folders, delete of a nested folder
    with child folders, items and a non-zero order, delete of a root, scenes and palettes folder, and
    `SelectFolder` with an existing fixture filter. Others: `delete_rollback_survives_a_child_with_the_folder_name`,
    `create_uses_the_given_id_and_checks_id_parent_and_name`, `rename_trims_and_checks_the_name`,
    `siblings_sort_by_order_then_name`, `failed_events_have_no_rollback_and_change_nothing`,
    `repair_folders_only_reports_bad_names`, `showfile_folders_without_order_load`.
  - `crates/shared/src/state/save.rs`: `folders_saved_before_folder_order_still_load`.
  - `crates/core/src/dmx/mod.rs`: `folder_events_and_their_rollbacks_through_the_engine` runs the
    events and their rollbacks through the real `DmxEngine::apply` (`Transaction`, `DeSelectGroup`,
    `LimitSelectionToFixtureInCurrentGroup` arms included), and the rejections (taken id, empty and
    duplicate names, cycle) with `(Some(err), None)` and an unchanged state.
- `cargo clippy --workspace --all-targets --all-features`: exit 0. None of its warnings point at a
  line this ticket added; all are in untouched code.
- `cargo fmt --all -- --check`: only the known old diff in `crates/core/src/app/pages/animations.rs`.
- Plugins: `make build_noopt` in the container (log `.blenv/plugins.log`). `grep '^error'` finds only
  the known `pioneer_pro_dj_link` failure (`no matching package named prolink`). Then
  `scripts/blenv build all` built with exit 0, and the inspector answered `ping` with `"abi": 17`.
- End to end on a scratch copy of `Kuze_Theater.migrated.json` (`blenv run --showfile`; state checked
  with `blenv blctl state`, parsed from the Debug fallback; rollbacks read from `blenv blctl tap`,
  where the engine sends them with originator `DmxEngine`):
  - Built a tree with `CreateFolder` (explicit ids 0..4, groups and scenes), `MoveToFolder` (groups 0,
    2, 5, 7 and scene 0) and `ReorderFolder(3, 5)`. Each one answered with the expected rollback.
  - These were rejected with an unchanged state and no rollback: a taken id, a blank name, `" stage "`
    next to `Stage`, `RenameFolder(3, "FRONT")` next to `Front`, a folder moved into its child, and a
    group moved into a scenes folder.
  - Round trips, each with the rollback from the tap applied and the state compared to before:
    create, rename, two reorders, four moves (group, folder to root, folder between folders, scene),
    `DeleteFolder` of a nested folder (child folder plus 2 groups), of the root folder `Stage`
    (2 child folders plus a group) and of the scenes folder, and `SelectFolder(0)` with group 6 and
    fixture filter `{0}` selected. That last rollback was `Transaction[DeSelectGroup 0, 2, 5, 7,
    LimitSelectionToFixtureInCurrentGroup 0]`, and it restored the selection `{6}` and the filter
    `{0}`. Every rollback restored the state.
  - Autosave wrote `folders` with `order` (folder 3: 5) into the container's copy. Reloading that file
    restored the same tree.
  - 016-shaped file: the autosaved file with every `order` key removed (the exact 016 `Folder`
    shape) loaded with the same tree and `order` 0 everywhere. I did not run a 016 binary to write
    it.
  - A copy with two sibling folders named `Front` / `FRONT ` loaded unchanged and logged
    `WARN … sibling Groups folders [1, 3] share the name "front"; left as is`.
  - A copy of `Kuze_Theater.json` itself fails as described above.

### Follow-ups
- Kuze/17.01 cleanup (b) above, after maintainer approval.
- `crates/core/src/app/event.rs` `control_event_marks_showfile_dirty` treats `SelectFolder` as
  dirtying (it falls into `_ => true`), unlike `SelectGroup`. That's from 016. It's a one-line fix in
  `app/`, left out because this ticket stays out of UI files.
- The engine's `Transaction` arm drops its sub-events' rollbacks. If 011 or later wants a generic
  undo, it could return a reversed `Transaction` of them.

### Kuze follow-up (applied after maintainer approval, 2026-10-30)
- Compared before replacing:
  - `Kuze_Theater.migrated.json` and `Kuze_migrated.json` are byte-identical.
  - Old vs. migrated: same top-level keys, and the only differences are format changes (272 diffs, all in fixture-state and amplitude shapes).
  - All 208 fixture-state values match after the unit mapping (`h` truncated degrees, `s`/`v` × 255, the rest identical).
- Applied option (b): `git mv -f Kuze_Theater.migrated.json Kuze_Theater.json`, and removed `Kuze_migrated.json`.
- `17.01.json` was **not** replaced. `17.01_migrated.json` is a later, edited show, not just a migration:
  - its groups, fixtures, names and addresses differ;
  - it has palette assignments and audio-modulation animations that the old file lacks;
  - its scene changesets differ.

  `17.01.new.json` and `17.01.freq.json` are later revisions again. Which of these to keep is the maintainer's call.

