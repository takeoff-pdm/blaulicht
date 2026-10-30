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
- [ ] For every folder event, applying it and then its rollback leaves `EngineState` unchanged. Unit tests cover this, including deleting a folder that has nested child folders and items of its kind.
- [ ] `CreateFolder` with a taken id, an empty name or a duplicate sibling name is rejected without changing the state.
- [ ] Siblings sort by `(order, name)`. A showfile without `order` still loads.
- [ ] `Kuze_Theater.json` loads with its real groups and scenes (`blctl state`), or has been replaced by a migrated file that does. The choice is recorded in Notes.
- [ ] ABI bumped and plugins rebuilt.

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
