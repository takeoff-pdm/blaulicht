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
- [ ] The navbar shows Setup, and each sub-page renders and works as before.
- [ ] No scene controls on the Fixtures or Groups sub-pages.
- [ ] Existing showfiles load unchanged.

## Verification
- `cargo clippy --all-targets --all-features` and `cargo fmt` are clean.
- `cargo test --all-features` passes, and new logic has co-located `#[cfg(test)]` tests.
- If the ABI was bumped: `PLUGIN_ABI_VERSION` in `crates/shared/src/abi.rs` is incremented and `cd crates/plugins && make build_noopt` was run. The Makefile reports success even on failure, so grep its output for `^error`.
- Follow **UI Verification** in `crates/core/AGENTS.md` (check that `swaylock` is not running, launch from the repo root, navigate via blctl, capture with grim). Use the headless Xvfb recipe when clicks are needed or another session is running (see memory notes). Take before and after screenshots, keep them in the scratchpad, and never commit them.
- Screenshot each Setup sub-page in the default 800x480 layout and in `--desktop-mode`.

## Notes
<!-- Implementation notes, decisions, and follow-ups found while working. Keep this updated before renaming to .done.md. -->
