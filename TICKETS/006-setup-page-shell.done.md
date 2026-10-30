# 006 — Setup page with Fixtures / Groups / Scenes sub-pages

| | |
|---|---|
| Epic | E2 Setup page |
| Depends on | 005 |
| ABI / showfile | Possibly **ABI bump** if `AppPage` gains a variant (append at the end; keep `FixturesSetup` index or repurpose it as `Setup`) |

**Touches:** `crates/shared/src/page/mod.rs`, `crates/core/src/app/pages/fixtures_setup.rs`, `crates/core/src/app/components/fixtures_shared.rs`, `crates/core/src/app/page.rs`, `crates/core/src/app/components/navbar.rs`, new: crates/core/src/app/pages/setup/

## Problem
Original review notes (2026-09-27):

> fixture setup should have nothing to do with scenes
> maybe sub-pages for scenes and groups
> also scene animations / scene changes do not have to belong in the fixtures setup, but stuff like clone scene, etc should go in there.

## Scope
- The `FixturesSetup` nav entry becomes **Setup**, with sub-pages **Fixtures**, **Groups** and **Scenes** (Views, Animations and Palettes are added later by 009 and 010).
- Split `fixtures_setup.rs` (1.4k LOC) into `pages/setup/{fixtures,groups,scenes}.rs`, mostly by moving code:
    - Groups: add, rename and delete group dialogs (`fixtures_setup.rs:117-265`).
    - Scenes: new, clone, rename and delete scene dialogs (`fixtures_shared.rs:328-575`).
    - Fixtures: group picker, fixture editor, add/delete/move fixture.
- The Fixtures sub-page shows no scene UI (no `scene_overview`, no scene buttons).
- Remove the scene **changeset** and **scene animations** buttons from setup. They move to the Programmer in 011, so until then keep them reachable from FixturesPerformance and note it in the ticket.
- Leave the "Show Overrides" and "Simulate" buttons in place for now (they move in 019 and 024).

## Out of scope
- Visual redesign, bulk rename, identify wizard (007, 008).
- Moving View prep (009).

## Key code
- `fixtures_ui_setup` — `crates/core/src/app/pages/fixtures_setup.rs:818`
- `group_selection`, `scene_overview`, scene dialogs — `crates/core/src/app/components/fixtures_shared.rs`
- `page_content_based_on_tab` — `crates/core/src/app/page.rs`

## Acceptance criteria
- [x] The navbar shows Setup, and each sub-page renders and works as before.
- [x] No scene controls on the Fixtures or Groups sub-pages.
- [x] Existing showfiles load unchanged.

## Verification
- `cargo clippy --all-targets --all-features` and `cargo fmt` are clean.
- `cargo test --all-features` passes, and new logic has co-located `#[cfg(test)]` tests.
- If the ABI was bumped: `PLUGIN_ABI_VERSION` in `crates/shared/src/abi.rs` is incremented and `cd crates/plugins && make build_noopt` was run. The Makefile reports success even on failure, so grep its output for `^error`.
- Follow **UI Verification** in `crates/core/AGENTS.md` (check that `swaylock` is not running, launch from the repo root, navigate via blctl, capture with grim). Use the headless Xvfb recipe when clicks are needed or another session is running (see memory notes). Take before and after screenshots, keep them in the scratchpad, and never commit them.
- Screenshot each Setup sub-page in the default 800x480 layout and in `--desktop-mode`.

## Notes
<!-- Implementation notes, decisions, and follow-ups found while working. Keep this updated before renaming to .done.md. -->
### What changed
- **Page/ABI:** `AppPage::FixturesSetup` was renamed to `AppPage::Setup`, keeping index 3. `#[serde(alias = "FixturesSetup")]`
  keeps old showfile dock layouts (`"tabs": ["FixturesSetup"]`), stored `midi_all` mappings and blctl
  `NavigatePage":"FixturesSetup"` working. The navbar short label is "Setup"; the dock tab title is "Setup".
  `AppSubPage` gained `Setup_Fixtures`, `Setup_Groups` and `Setup_Scenes`, appended at the end (indices 7–9).
  `PLUGIN_ABI_VERSION` is now **16**. Ticket 031 is being implemented in parallel and will use 17, so the integrator
  may renumber at merge time. The `console` and `midi_all` plugins were updated for the rename.
- **Files:** `pages/fixtures_setup.rs` became `pages/setup/`:
  - `mod.rs`: `setup_ui` dispatches via `subpage_ui`. It also renders the Show Overrides and Simulate tool windows
    page-wide, so they stay open across tabs.
  - `fixtures.rs`: Setup › Fixtures. The toolbar has Add Fixture | Show Overrides | Simulate, then
    `group_selection` (groups + fixtures) and the fixture editor. `select_setup_fixture` is shared with Groups.
  - `groups.rs`: Setup › Groups. The toolbar has Add Group / Ren. Group / Delete Group. `setup_group_list` is the
    list on the left (**017 replaces it with the folder tree**); `setup_group_details` is on the right (name, id,
    fixtures, and an "Edit Fixtures" link to the Fixtures tab). It also has the add/rename/delete group dialogs.
  - `scenes.rs`: Setup › Scenes. It has `scene_overview` on the left (018 replaces it with a tree) and the toolbar
    Scene + / Clone Sc. / Ren. Sc. / Del. Sc., plus the new/clone/rename/delete scene dialogs, which moved out of
    `components/fixtures_shared.rs`.
- The selected group (`add_fixture_group`) is shared by the Fixtures and Groups tabs.
- **`group_tile()`** (`components/fixtures_shared.rs`): the group button, now shared by Setup › Groups,
  `group_selection` and `fixture_selection`. The look is unchanged.
- **Scene Changes / Scene Animations:** both buttons were removed from setup.
  - Scene Animations stays reachable on FixturesPerformance, through the button already there.
  - Scene changes are reachable on FixturesPerformance through its existing inline "Scene Changes" panel. No button
    was added there because the 800 px toolbar has no room.
  - The changeset dialog (`render_scene_changeset_dialog`) is therefore not called anywhere right now. It was kept
    for 011, which plans to reuse it.
- **Layout:** in the fixture editor, the action column spacing went from 12 to 6 px, so Delete stays above the tab
  bar at 800x480. The editor now scrolls when its area is shorter than the editor (small desktop dock panes).
- The "Add Fixture" popup now says where to add a group when none exists.

### Verification
- `cargo clippy --workspace --all-targets --all-features`: no errors and no new warnings. All warnings in touched
  files come from moved code, for example `screen_rect` deprecation, the layout test's deprecations, "items after a
  test module", and the collapsible `if` in the delete-scene dialog.
- `cargo fmt --all --check`: only the known old diff in `pages/animations.rs`.
- `cargo test --workspace --all-features`: 292 passed. New tests:
  - `page::sub::tests::setup_keeps_the_fixtures_setup_index_and_name`;
  - Setup cases in the existing sub-page tests;
  - `config::tests::dock_tabs_saved_as_fixtures_setup_load_as_setup`.
- `make build_noopt`: the only `^error` is the known `pioneer_pro_dj_link` absolute path dependency.
- **Build and app:** `blenv build all` succeeded and blctl ping reports ABI 16.
- **Showfile load:** a copy of `Kuze_Theater.migrated.json` loaded with its groups. In `--desktop`, its saved dock
  layout (`"FixturesSetup"`) came back as the "Setup" pane.
- **Screenshots** (blenv, `.blenv/shots/`, not committed):
  - before: 800x480 Setup and FixturesPerformance, and desktop;
  - after: each Setup tab at 800x480 and in `--desktop` with the pane switched to Dynamic.
- **Dialogs:**
  - Group dialogs: a group was added, renamed and deleted, which restored the original set.
  - Scene dialogs: a scene was created, the clone dialog cancelled and later confirmed, the scene renamed, and one
    scene deleted.
  - Fixture tools: Add Fixture was opened and cancelled; Show Overrides was opened and closed; Simulate was opened;
    an editor numberpad was opened and cancelled.
  - FixturesPerformance: Scene Animations opens.

### Follow-ups / not covered
- In `--desktop` a dock pane in the "Default (723 x 480)" render mode clips its bottom when the pane is shorter
  than 480 px, so the sub-page tab bar is not visible. This affects System since 005 too. Switching the pane to
  Dynamic (tab context menu) shows it. Added to the Inbox.
- `group_tile` (moved code) uses `name.truncate(10)`, which panics if byte 10 is not a char boundary, e.g. with
  umlauts. This was already the case before. Added to the Inbox.
- `current_scene_changeset_dialog_open` is never set now; 011 should wire it up or remove it.
