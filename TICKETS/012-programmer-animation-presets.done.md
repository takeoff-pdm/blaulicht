# 012 — Apply animation presets from the Programmer

| | |
|---|---|
| Epic | E3 Programmer |
| Depends on | 011 |
| ABI / showfile | none expected (reuse existing preset / animation events) |

**Touches:** `crates/core/src/app/pages/fixtures_perf.rs`, `crates/core/src/app/pages/animations.rs (read-only reuse)`

## Problem
Original review notes (2026-09-27):

> i think not really, but would be nice to have presets.

## Scope
- Add an **Animations** panel or dialog in the Programmer. It lists `AnimationPreset`s (and templates) as buttons, and applying one attaches it to the current selection in the focused scene, the same way `render_add_animations_dialog` does today.
- Show the animations active on the current selection with remove buttons.
- Allow saving the animation on the current selection as a new preset.

## Out of scope
- Editing templates (stays in Setup, 010).

## Key code
- `AnimationPreset`, `AnimationTemplate` — `crates/shared/src/state/engine.rs:391+`
- `render_add_animations_dialog`, `render_scene_animations_dialog` — `crates/core/src/app/pages/fixtures_perf.rs`

## Acceptance criteria
- [x] A preset can be applied to a selection in at most 2 taps. (Toolbar **Animations**, then the preset.)
- [x] The active animations list matches `blctl.py state` for the scene.

## Verification
- `cargo clippy --all-targets --all-features` and `cargo fmt` are clean.
- `cargo test --all-features` passes, and new logic has co-located `#[cfg(test)]` tests.
- Follow **UI Verification** in `crates/core/AGENTS.md` (check that `swaylock` is not running, launch from the repo root, navigate via blctl, capture with grim). Use the headless Xvfb recipe when clicks are needed or another session is running (see memory notes). Take before and after screenshots, keep them in the scratchpad, and never commit them.
- Screenshot the presets panel and the selection with an animation running.

## Notes
<!-- Implementation notes, decisions, and follow-ups found while working. Keep this updated before renaming to .done.md. -->
The Key code names `fixtures_perf.rs`; since 011 that file is `pages/programmer.rs`.

### What changed
- **Toolbar:** "Add Animations" became **Animations (n)**, n being the number of animations on the selection in
  the focused scene. It is still 7 buttons (`TOOLBAR_BUTTONS` unchanged).
- **Animations dialog** (`render_animation_presets_dialog`, new module `pages/programmer_animations.rs`, state
  `ProgrammerUi::animation_presets: AnimationPresetsUi`). It is 740 px wide and fits at 800x480.
  - **Right side:** "Saved" templates, then the starter presets (`AnimationPreset`) by category, in rows of 4. One
    tap applies one to the current selection in the focused scene. A second tap removes it. A button is
    highlighted while it is on the selection.
  - **Left side, "On selection (n)":** the animations on the current selection. Each row shows the name,
    `#template · property` and "paused" if paused, an **X** that sends `RemoveAnimation`, and **Save**.
  - **Save** opens a nested form (`render_save_animation_preset_dialog`) with a free name prefilled
    (`unique_template_name`, e.g. "Kick Flash 2"). Saving stores the animation's current `spec_cloned` as a new
    `AnimationTemplate` (`save_as_template`). For a WASM animation, the plugin state is copied from the instance to the
    template, as the Animations page does. An empty or taken name is refused.
  - **Disabled buttons:** with nothing selected, and for a property bound to a palette on the selection (the engine
    rejects those). `Esc` closes the form first, then the dialog.
- **Events (no ABI bump, stays 17):**
  - Applying sends `Transaction[AddAnimation(id), PlayAnimation(id)]` (`toggle_animation_event`). A new
    `ActiveAnimation` starts paused, so plain `AddAnimation` (the old dialog's behaviour) left an applied preset
    doing nothing until Play was tapped. If the add is rejected, the transaction rolls back as a whole.
  - Removing sends `RemoveAnimation(id)`.
- **Presets vs templates:** `AddAnimation` takes a template id, and an `AnimationPreset` is not a template. So a
  preset tap first looks for the template named after the preset (`preset_template_id`, lowest id) and creates one
  from `AnimationSpec::preset` if there is none (`ensure_preset_template`). It writes directly to
  `animation_templates`, as `insert_animation_template` on the Animations page does. Templates named after a preset
  are not listed under "Saved"; their preset button stands for them. Consequences:
  - If a user edits the "Kick Flash" template, the preset button applies the edited version.
  - If the template is renamed, the next tap creates a fresh "Kick Flash".
  - Templates without a name are labelled `#id`.
- **Removed:** the old Add Animation dialog (`render_add_animations_dialog`, `BlaulichtApp::add_animations_dialog_open`,
  `add_selected_animation`). It needed 3 taps and offered templates only.
- **Fix (own commit):** the engine keys a scene's animations by the *sorted* selection. The Programmer's animation
  cards and the scene changes panel looked them up by the unsorted `get_selection()`. That misses for a limited
  selection whose `fixtures_in_group` HashSet iterates out of order (seen live: `{2, 0}`). Both now use the sorted
  selection (`selection_animations`, `selection_animation_count`).

### For 010
- **Stays in the Programmer:** applying, removing and saving (above), the Scene Animations dialog
  (`render_scene_animations_dialog`, which edits an applied animation's `spec_cloned` via `LoadSpecIntoAnimation`)
  and the cards under the scene changes.
- **Editor-only (moves to Setup in 010):** creating and editing templates, starter presets as *template creation*
  (`render_presets_dialog`), delete and rename. They all live in `pages/animations.rs`; this ticket only read it.
  The Programmer dialog calls no function from `animations.rs`. It does use `AnimationSpec::preset` and writes
  `animation_templates` itself, so 010 can move the editor without touching it.

### Verification
- `cargo clippy --workspace --all-targets --all-features` (blenv container): no errors. The only warnings in
  `programmer.rs` are the two old Lmt All loops; the new module has none.
- `cargo fmt --all --check`: only the known old diff in `pages/animations.rs`.
- `cargo test --workspace --all-features`: 319 passed. New tests in `app::pages::programmer_animations::tests`:
  - `applying_and_removing_a_preset_goes_through_the_engine` runs the events through `DmxEngine::apply` on a
    limited selection. The animation appears under the sorted selection in the focused scene only, runs, and the
    second tap removes it;
  - `a_preset_reuses_the_template_named_after_it`, `a_preset_without_template_gets_one_at_the_lowest_free_id`,
    `no_template_is_created_when_all_ids_are_taken`, `saving_keeps_the_spec_and_sets_the_trimmed_name`,
    `suggested_names_do_not_collide`, `saved_templates_leave_out_preset_named_ones`,
    `unnamed_templates_are_labelled_by_id`, `a_tap_toggles_the_animation`.
- **App (blenv, copies of `Kuze_Theater.json`; the show loaded with its groups, scene "Empty Scene" and 6
  templates):** real clicks, keys and typing. After each step, `blctl state` (Debug dump, parsed per scene) was
  compared with the dialog:
  - At 800x480, with Fluter selected: Animations, then Kick Flash, is **2 taps**. Template #6 "Kick Flash" was
    created and applied without "paused". The list showed 3 animations, the same as the engine (#0, #5, #6 on
    `[(5,0)..(5,3)]`).
  - The X on "Color" removed it: 2 in the dialog and in the engine.
  - Save on Kick Flash, typed "Fluter Kick": template #7 appeared under Saved and in the engine. It was applied to
    a Fluter + Frontlicht selection (8 fixtures, its own key in the engine) and removed with a second tap.
  - A limited selection (Fluter LL + RR) with Hue Drift: the engine had `fixtures_in_group: {2, 0}` and key
    `[(5,0),(5,2)]`. The dialog list and the page card ("Hue Drift", Pause = running) both showed it.
  - In `--desktop` (Programmer dock pane): Bass Pump applied, and the list matched the engine (3). `Esc` closed the
    dialog. With an empty selection all buttons were disabled, with the hint "Select a group or fixtures first".
  - Screenshots are in `.blenv/shots/` (not committed).

### Not covered / follow-ups
- Disabling buttons for properties bound to a palette (`frozen_properties`) was not exercised: the test show has
  no palette bindings on a selection. The engine rejects such an add anyway.
- Engine rejections (e.g. a palette-bound property) are not shown in the UI. This is not new.
- At 800x480 the Programmer's right column (scene changes, then animation cards) does not scroll. Only the first
  card is fully visible when the selection has two or more animations. The dialog's list has them all. Added to
  the Inbox.
