# 010 — Animation template editor and Palettes page become Setup sub-pages

| | |
|---|---|
| Epic | E2 Setup page |
| Depends on | 006, 012 |
| ABI / showfile | **ABI bump** to 18 (`AppSubPage` gained two variants); `AppPage` variants kept and hidden from the navbar |

**Touches:** `crates/core/src/app/pages/animations.rs`, `crates/core/src/app/pages/palettes.rs`, `crates/shared/src/page/mod.rs`, `crates/core/src/app/components/navbar.rs`

## Problem
Original review notes (2026-09-27):

> also its questionable whether we need a separate page for the animations?
> i think not really, but would be nice to have presets.

## Scope
- Mount the animation template editor (`animations.rs`) as Setup › Animations and the palette editor (`palettes.rs`) as Setup › Palettes.
- Remove the Animations and Palettes entries from the navbar. The day-to-day workflow (apply presets or palettes) lives in the Programmer (011, 012, 015).
- `NavigatePage(Animations|Palettes)` from plugins keeps working by redirecting to the new sub-page.

## Out of scope
- Tree UI for palettes (018).

## Key code
- `animations_ui` — `crates/core/src/app/pages/animations.rs`
- `palettes_ui` — `crates/core/src/app/pages/palettes.rs`

## Acceptance criteria
- [x] The navbar has no Animations/Palettes entries, and both editors work inside Setup.
- [x] Old `NavigatePage` events land on the right sub-page.

## Verification
- `cargo clippy --all-targets --all-features` and `cargo fmt` are clean.
- `cargo test --all-features` passes, and new logic has co-located `#[cfg(test)]` tests.
- If the ABI was bumped: `PLUGIN_ABI_VERSION` in `crates/shared/src/abi.rs` is incremented and `cd crates/plugins && make build_noopt` was run. The Makefile reports success even on failure, so grep its output for `^error`.
- Follow **UI Verification** in `crates/core/AGENTS.md` (check that `swaylock` is not running, launch from the repo root, navigate via blctl, capture with grim). Use the headless Xvfb recipe when clicks are needed or another session is running (see memory notes). Take before and after screenshots, keep them in the scratchpad, and never commit them.
- Screenshot Setup › Animations and Setup › Palettes.

## Notes
<!-- Implementation notes, decisions, and follow-ups found while working. Keep this updated before renaming to .done.md. -->
### What changed
- **Pages and ABI:** `AppSubPage` gained `Setup_Animations` and `Setup_Palettes`, appended (indices 10 and 11) and
  listed after Scenes. `PLUGIN_ABI_VERSION` is now **18**. Ticket 017 runs in parallel and is not expected to bump;
  if another ticket takes 18 first, the integrator renumbers.
- **The old pages stay:** `AppPage::Animations` and `AppPage::Palettes` keep their indices (7, 8) and JSON names.
  The new `AppPage::moved_to()` maps them to their Setup tab, and `AppPage::navbar_pages()` is every page without a
  `moved_to`.
  - The navbar draws `navbar_pages()` only (9 buttons instead of 11).
  - `Navbar::navigate_to` (used by `NavigatePage` from plugins and blctl, and by `Navbar::new`) opens the Setup tab
    for a moved page. No extra event is sent back to plugins; they already saw the `NavigatePage` they sent.
  - `page_content_based_on_tab` still renders the bare editor for the old pages, in case a caller passes one.
  - No plugin source changes were needed: names are unchanged, `console` and `midi_all` still send
    `NavigatePage(Animations)` (and `midi_all` also `Palettes`), which now redirect.
- **Files** (`pages/setup/`):
  - `animations.rs`: Setup › Animations, the template editor moved out of `pages/animations.rs`: `AnimationUI`,
    the Create / Presets (`render_presets_dialog`) / Delete dialogs, `insert_animation_template`,
    `animation_pagination` and `setup_animations_ui` (formerly `animations_ui`). It shows 4 templates per page
    instead of 5, because the tab bar takes the fifth one's room at 800x480.
  - `palettes.rs`: Setup › Palettes, moved from `pages/palettes.rs`. `setup_palettes_ui` draws the toolbar
    (`palette_toolbar`: Create Palette) and then **`setup_palette_list(ui, render_context, &[(u8, Palette)])`**, the
    list 018 replaces with a tree. The heading "Palettes" was dropped, since the tab says it. The kind label moved
    to `palette_kind_label` (tested) so a tree row can reuse it.
  - `mod.rs`: two `mod`/`pub use` lines, two doc lines and two match arms.
  - What stayed in `pages/`: the spec editor form `AnimationEditState` and `live_audio_for_preview` are shared with
    the Programmer's Scene Animations dialog, so `pages/animations.rs` was renamed to
    `pages/animation_spec_editor.rs` (git sees the rename) and keeps only those.
  - Not moved: the palette-assignment dialog (`render_palette_assignment_dialog`, `pages/programmer.rs`), the
    overlay pickers (`render_overlay_picker_dialog` in `pages/view.rs`, `render_view_perf_overlay_picker_dialog` in
    `pages/view_perf.rs`) and `scene_overview` (`components/fixtures_shared.rs`).
- **Desktop dock** (`external_screen.rs`): new docks and the "add missing pages" pass use `navbar_pages()`, so no
  Animations/Palettes panes are created. A saved layout is migrated on load (`migrate_moved_pages`):
  - an Animations or Palettes tab becomes a Setup tab in the same place and render mode if the layout has no Setup
    tab yet;
  - otherwise it is dropped, and a leaf that is left empty gets a Setup tab (so a split pane stays where it was);
  - other tabs, including legacy bare names such as `"FixturesSetup"`, are not touched, and the leaf's active tab
    follows its tab when that tab survives.
  - The Setup panes share one tab selection (it lives in the navbar state), so two Setup panes show the same tab.
- **Fix (own commit):** the sub-page tab bar (`components/horizontal_nav.rs`) used `horizontal_wrapped`. In a
  desktop dock pane of about 720 px, Setup's fifth tab wrapped into a second row below the clip rect and could not
  be clicked. Rows are now explicit chunks of the column count `subpage_tab_bar_height` reserves, and widths are
  whole pixels.
- **fmt:** the old `cargo fmt --check` diff in `pages/animations.rs` is fixed, in its own commit before the move.
  `cargo fmt --all --check` is now clean.
- **Docs:** `crates/core/AGENTS.md`, `.claude/skills/blaulicht-env/SKILL.md` and `tools/blenv/blenv.py` (comment) say
  that `Animations`/`Palettes` open the Setup tabs, and list `Setup_Animations`/`Setup_Palettes`. `blenv nav
  Animations|Palettes` still works and redirects.

### Verification
All cargo runs were in the worktree's blenv container (`docker exec -w /work -e CARGO_BUILD_JOBS=5 …`).
- `cargo clippy --workspace --all-targets --all-features`: no errors. The warnings in touched files are all in moved
  or untouched code (the navbar's deprecated `SidePanel` and `clone` on `Copy`, `HSVColor` clones and a unit
  `let` in the palette dialogs, the spec editor's 8-argument fn, `external_screen.rs` deprecations).
- `cargo fmt --all --check`: clean.
- `cargo test --workspace --all-features`: 327 passed. New tests:
  - `page::sub::tests::animations_and_palettes_moved_into_setup` and `the_navbar_leaves_out_moved_pages`, plus the
    new indices in `existing_bincode_indices_are_stable`;
  - `components::navbar::tests::moved_pages_open_their_setup_tab`;
  - `external_screen::tests::new_docks_leave_out_moved_pages`, `saved_moved_page_tabs_are_dropped_when_setup_is_open`
    (the Kuze layout shape), `a_saved_moved_page_becomes_setup_in_its_place` and
    `a_leaf_with_only_moved_pages_keeps_a_setup_tab`;
  - `pages::setup::palettes::tests::kind_labels_name_single_properties`.
- `make build_noopt` (container, log in `.blenv/plugins.log`): the only `^error` is the known `pioneer_pro_dj_link`
  `prolink` path dependency. `blctl ping` reports `abi: 18`, and all 5 blenv plugins loaded.
- **App (blenv, copies of `Kuze_Theater.json`, real clicks; screenshots in `.blenv/shots/`, not committed):**
  - Before (v3 build): the Animations page (empty and with template 0 selected), the Palettes page, Setup › Fixtures
    at 800x480, and the desktop dock with Animations and Palettes tabs.
  - After, 800x480: the navbar shows 9 entries, without Animations and Palettes.
    `blctl event '{"MainUi":{"NavigatePage":"Animations"}}'` landed on Setup › Animations, and the `Palettes` one
    on Setup › Palettes.
  - Template editing: on template 0, ticking "Reverse after N iterations" and Apply set
    `reverse_after_n_iterations: Some(..)` in the engine. Unticking and Apply set it back to `None`.
  - Presets → Kick Flash created template #6, which was then selected, deleted and confirmed. The engine went back
    to ids 0–5.
  - Palettes: Create Palette → Create made `[0] New Palette (Color)` (engine: one palette). Edit opened prefilled and
    was cancelled. Delete → Confirm removed it (engine: no palettes).
  - `--desktop` with the original layout (Animations in the first leaf, `"FixturesSetup"` in its own leaf): the
    Animations and Palettes tabs are gone, and the Setup pane is unchanged. After switching it to Dynamic, its 5
    tabs are visible (after the tab bar fix). Clicking Palettes works, and `NavigatePage(Animations)` selects
    Setup › Animations in that pane.
  - `--desktop` with a modified copy (Animations in the first leaf, a Dynamic Palettes-only leaf where Setup was):
    Animations became a "Setup" tab in its place in the first leaf. The Palettes leaf became a second Setup pane,
    still Dynamic. Clicking its Palettes tab showed Setup › Palettes in both Setup panes.

### Not covered / follow-ups
- The template details form does not scroll, so the audio-modulation form runs under the tab bar at 800x480. It
  already overflowed the old page, but now overflows by 40 px more. Added to the Inbox.
- The Starter Presets dialog (560x520) is cramped at 800x480. Unchanged code; added to the Inbox.
- When a loaded dock layout lacks pages, they are appended to the first leaf, which also changes that leaf's active
  tab. This already happened before; added to the Inbox.
- The active-tab handling for a migrated layout is covered by unit tests only. In the app it is overridden by the
  appended tabs (above).
- 018: replace `setup_palette_list` in `pages/setup/palettes.rs`. The toolbar (`palette_toolbar`) is where folder
  actions go.
