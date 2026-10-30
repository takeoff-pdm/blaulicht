# 016 — Folder hierarchy data model for groups, scenes and palettes

| | |
|---|---|
| Epic | E4 Hierarchy (folders) |
| Depends on | none |
| ABI / showfile | **ABI bump** (serialized shared types change) and **showfile shape** change (new fields need `#[serde(default)]`) |

**Touches:** `crates/shared/src/fixture/state.rs`, `crates/shared/src/scene.rs`, `crates/shared/src/palette.rs`, `crates/shared/src/state/engine.rs`, `crates/shared/src/abi.rs`, engine event handling, save.rs / showfile

## Problem
Original review notes (2026-09-27):

> fixture groups should be hierarchical
> same for scenes
> like everything, we need hierarchies

## Scope
- Add `folders: BTreeMap<u16, Folder { name, parent: Option<u16>, kind: FolderKind(Groups|Scenes|Palettes) }>` to `EngineState` and `folder: Option<u16>` on `FixtureGroup`, `Scene` and `Palette` (all `#[serde(default)]`).
- Add `ControlEvent`s, appended at the end, for create, rename, delete (children move to the parent) and move-item-to-folder, with cycle prevention.
- Selecting a groups folder selects all groups under it recursively: a helper plus a `SelectFolder` event that expands to group ids.
- Only data and engine changes here. The UI is 017 and 018.

## Out of scope
- Nested groups with value cascading.
- Any UI.

## Key code
- `FixtureGroup`, `EngineGroups` — `crates/shared/src/fixture/state.rs:28`
- `Scene` — `crates/shared/src/scene.rs:96`
- `Palette` — `crates/shared/src/palette.rs`
- `EngineState` — `crates/shared/src/state/engine.rs`

## Acceptance criteria
- [x] Unit tests cover create/move/delete, cycle rejection and recursive folder selection.
- [x] An existing showfile (e.g. a scratch copy of `Kuze_Theater.json`) still loads with all groups and scenes. Check with `blctl.py state` that it is **not** a default show (a shape mismatch silently boots a default show).
- [x] ABI bumped and plugins rebuilt.

## Verification
- `cargo clippy --all-targets --all-features` and `cargo fmt` are clean.
- `cargo test --all-features` passes, and new logic has co-located `#[cfg(test)]` tests.
- If the ABI was bumped: `PLUGIN_ABI_VERSION` in `crates/shared/src/abi.rs` is incremented and `cd crates/plugins && make build_noopt` was run. The Makefile reports success even on failure, so grep its output for `^error`.
- Showfile compatibility: load a **scratch copy** of an existing showfile and confirm with `python3 scripts/blctl.py state` that it is not a silently booted default show.

## Notes
<!-- Implementation notes, decisions, and follow-ups found while working. Keep this updated before renaming to .done.md. -->

### Design (what 017/018 build on)
All logic lives in `crates/shared/src/folder.rs`, with its tests next to it. The engine arms in
`crates/core/src/dmx/mod.rs` only forward to it.

- Types:
  - `pub type FolderId = u16`.
  - `enum FolderKind { Groups, Scenes, Palettes }`.
  - `struct Folder { name: String, parent: Option<FolderId>, kind: FolderKind }`.
  - `enum FolderItem { Group(u8), Scene(u8), Palette(u8), Folder(FolderId) }`.
- Data: `EngineState.folders: BTreeMap<FolderId, Folder>`, plus `folder: Option<FolderId>` on
  `FixtureGroup`, `Scene` and `Palette`. All are `#[serde(default)]`, as are their save mirrors
  (`SaveEngineState.folders`, `SaveFixtureGroup.folder`, `SavedScene.folder`). `None` means root.
  A folder's parent must be a folder of the same kind; one tree per kind.
- Events, appended after `AnimationPluginUi`:
  - `CreateFolder { kind, parent, name }` gets the lowest free id (`next_folder_id`). The UI can
    predict that id, because the event returns no value.
  - `RenameFolder(id, name)`.
  - `DeleteFolder(id)` deletes only the folder. Its subfolders and items of its kind get the
    deleted folder's `parent`, which is the root if it had none. No group, scene or palette is
    deleted.
  - `MoveToFolder { item: FolderItem, folder: Option<FolderId> }`. It's rejected if the target
    holds another kind, or if a folder would move into itself or a descendant
    (`is_ancestor_or_self`). The ephemeral BLANK scene can't be moved.
  - `SelectFolder(id)` resolves through `EngineState::folder_group_ids(id) -> BTreeSet<u8>`: every
    group whose folder is in `folder_subtree(id)` (the folder itself plus all its descendants).
    These ids are added to `selection.group_ids`, like one `SelectGroup` per group. The fixture
    filter is cleared. It errors on a missing folder, a non-groups folder, an empty folder, or
    when everything is already selected.
- Methods on `EngineState` (all `Result<_, &'static str>`, used directly as the engine error):
  - `create_folder`, `rename_folder`, `delete_folder`, `move_to_folder`, `select_folder`.
  - `item_folder(FolderItem) -> Option<Option<FolderId>>`.
  - `folder_group_ids`.
  - `repair_folders() -> usize`.
- Free helpers: `next_folder_id`, `is_ancestor_or_self`, `folder_subtree`. They're cycle-safe
  (visited sets), so a broken file can't hang them.
- External input: `dmx::state::EngineState::load_showfile` calls `repair_folders()`. That moves
  folders with a missing, other-kind or cyclic parent, and items pointing at a missing or
  other-kind folder, to the root, and logs a warning.
- Other choices:
  - `clone_for_ui` keeps every scene's `folder`, so the tree UI can render scenes that aren't
    focused.
  - Cloning a scene (`EngineState::clone_scene`) keeps the source's folder.
  - Newly created groups, scenes and palettes land at the root.
- ABI: `PLUGIN_ABI_VERSION` 13 -> 14.

### Verification
Everything ran in the blenv container (`blenv-agent-a12055dfe99a3b54c-b7260de1`), never on the
host.
- The container's nightly toolchain had no clippy or rustfmt, so I added them with
  `rustup component add clippy rustfmt`. That change is container-local.
- `cargo clippy --workspace --all-targets --all-features` built with exit 0. No warning points at
  a line this ticket added. Many warnings on untouched code come from the newer nightly.
- `cargo fmt --all -- --check` is clean for every touched file. The one remaining diff,
  `crates/core/src/app/pages/animations.rs:856`, was already there and is untouched by this
  ticket. Touched files were formatted per crate edition: core is edition 2021, shared is 2024.
- `cargo test --workspace --all-features` passed with exit 0, including 11 `folder::tests` and 2
  new save tests: `folders_survive_a_round_trip` and `showfiles_without_folders_still_load`.
- Plugins: I ran `docker exec -w /work/crates/plugins … make build_noopt`, with the log in
  `.blenv/plugins.log`. `grep '^error'` found exactly one error:
  - `pioneer_pro_dj_link` fails with `no matching package named prolink`. Its `Cargo.toml` has a
    path dependency on `/home/mik/Downloads/prolink-test/prolink-rs`, which exists on no machine
    here.
  - This was already broken and is unrelated to the ABI bump. Its bundle was not rebuilt.
  - The other 20 plugins built into `crates/plugins/bundle_debug/`.
  - Then `scripts/blenv build core` built with exit 0. The app loaded inspector, julia,
    audio_reactor, chaos and sample_egui at ABI 14.
- Showfile check, running `scripts/blenv run --showfile <scratch copy>` and then
  `scripts/blenv blctl state`:
  - `Kuze_Theater.json` itself does **not** load. It fails with `expected value at line 451
    column 38`, and a default show boots. That line is `"amplitude_min": 0` in an animation, an
    old pre-`FixtureValue` shape in a section this ticket doesn't touch. I didn't rebuild v3 to
    confirm it fails there too. I used its migrated sibling instead.
  - `Kuze_Theater.migrated.json` loaded (`Loaded showfile …`). It has all 7 real groups
    (Frontlicht, Spitzlicht, Saal, Par Publikum, Fluter, Par Truss, Par Front), the scene
    `Empty Scene` plus BLANK, every `folder: None`, and `folders: {}`.
  - `17.01_migrated.json` loaded too, with 18 groups and the scenes Default Scene, My Scene, DEV,
    BS, STRB and BLANK.
  - Kuze's `state` comes back as the inspector's Debug fallback, because of tuple-keyed maps.
    That's expected; I parsed it from the dump.
- End to end through `blctl event`, on the loaded Kuze show:
  - I ran `CreateFolder` three times: Groups "Stage" became 0, Groups "Front" under 0 became 1,
    and Scenes "Act 1" became 2.
  - Then `MoveToFolder`: groups 0 and 7 went to 1, group 2 to 0, and scene 0 to 2.
  - Two moves were correctly rejected, and the state didn't change: `Folder(0)` into 1 (a cycle)
    and `Group(4)` into the scenes folder.
  - `RemoveAllSelection` followed by `SelectFolder(0)` gave `group_ids = {0, 2, 7}`. That's
    recursive.
  - `DeleteFolder(1)` moved groups 0 and 7 to folder 0. `RenameFolder(2, "Act One")` worked.
  - The app's autosave wrote `folders` and the per-item `folder` into the showfile copy.
    Reloading that file with `blenv run --showfile` restored the same tree.
- Not run: `cargo` on the host through `direnv exec .`. Host builds were avoided on purpose
  because the disk was tight; all cargo ran in the container. The blctl `event` reply is only
  `ok: true` (delivered), so I checked the rejections through the state, not the error text.

### Follow-ups and out-of-scope findings
- Folder events return no rollback event (`(err, None)`), so they don't take part in
  undo-by-rollback the way some selection and override events do. Add rollbacks if 017/018 need
  undo.
- There is no explicit sort order inside a folder. The UI has to order by id, or by name for
  folders. If manual ordering is wanted, it needs an `order` field, which means another ABI bump.
- Folder names are not checked: empty and duplicate names are allowed.
- `crates/plugins/pioneer_pro_dj_link/Cargo.toml` has an absolute path dependency, so it breaks
  `make build_noopt` everywhere except the original machine.
- `Kuze_Theater.json` in the repo root no longer loads (old animation value shape); only the
  `*.migrated.json` copies do.
- The blenv container image ships no clippy or rustfmt. Adding them to `tools/blenv/Dockerfile`
  would let agents run the Verification steps without `rustup component add`.

