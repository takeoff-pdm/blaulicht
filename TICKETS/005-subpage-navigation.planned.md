# 005 — Generic sub-page navigation for all pages

| | |
|---|---|
| Epic | E1 Foundations |
| Depends on | none |
| ABI / showfile | **ABI bump** (serialized shared types change); no showfile change |

**Touches:** `crates/shared/src/page/mod.rs`, `crates/shared/src/page/sub.rs`, `crates/shared/src/abi.rs`, `crates/core/src/app/components/horizontal_nav.rs`, `crates/core/src/app/components/navbar.rs`, `crates/core/src/app/event.rs`, `crates/core/src/app/page.rs`, `scripts/blctl.py (docs only)`

## Problem
Original review notes (2026-09-27):

> maybe sub-pages for scenes and groups
> also allow sub-page navigation

## Scope
- Generalize `AppSubPage` / `AppPage::list_subpages()` (`shared/src/page/sub.rs`) so any page can declare sub-pages. Today only System does. Keep bincode indices of existing variants stable.
- Make `horizontal_nav` the standard tab bar for sub-pages and keep the current sub-page per page in the navbar state.
- **Append** `MainUiEvent::NavigateSubPage(AppSubPage)` at the end of the enum. Navigating to a sub-page also switches to its parent page.
- Only the plumbing belongs here: add no new pages. Tickets 006, 022 and 024 use it.
- Document the event in `crates/core/AGENTS.md` (the Navigate step) with a blctl example.

## Out of scope
- Creating the Setup / Performance pages.

## Key code
- `AppPage`, `MainUiEvent` — `crates/shared/src/page/mod.rs`
- `AppSubPage` — `crates/shared/src/page/sub.rs`
- `horizontal_nav` — `crates/core/src/app/components/horizontal_nav.rs`
- `NavigatePage` handling — `crates/core/src/app/event.rs:39`

## Acceptance criteria
- [ ] System's tabs still work and are now driven by the generic mechanism.
- [ ] `blctl.py event '{"MainUi":{"NavigateSubPage":"<System sub-page>"}}'` switches to that tab.
- [ ] ABI bumped; all plugins rebuilt without errors.

## Verification
- `cargo clippy --all-targets --all-features` and `cargo fmt` are clean.
- `cargo test --all-features` passes, and new logic has co-located `#[cfg(test)]` tests.
- If the ABI was bumped: `PLUGIN_ABI_VERSION` in `crates/shared/src/abi.rs` is incremented and `cd crates/plugins && make build_noopt` was run. The Makefile reports success even on failure, so grep its output for `^error`.
- Showfile compatibility: load a **scratch copy** of an existing showfile and confirm with `python3 scripts/blctl.py state` that it is not a silently booted default show.
- Follow **UI Verification** in `crates/core/AGENTS.md` (check that `swaylock` is not running, launch from the repo root, navigate via blctl, capture with grim). Use the headless Xvfb recipe when clicks are needed or another session is running (see memory notes). Take before and after screenshots, keep them in the scratchpad, and never commit them.
- Screenshot System with two different sub-pages selected via blctl.

## Notes
<!-- Implementation notes, decisions, and follow-ups found while working. Keep this updated before renaming to .done.md. -->
