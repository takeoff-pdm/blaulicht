# 009 — View preparation into Setup › Views, with a Test button

| | |
|---|---|
| Epic | E2 Setup page |
| Depends on | 006, 020 |
| ABI / showfile | none beyond what 020 adds |

**Touches:** `crates/core/src/app/pages/view.rs -> crates/core/src/app/pages/setup/views.rs`, `crates/shared/src/page/mod.rs`

## Problem
Original review notes (2026-09-27):

> view preparation looks a bit ugly as well, and should be integrated into the setup meta page.
> also there shouldn't be a single 'apply', but rather also a 'test' button

## Scope
- Move the View page (`view.rs`, 496 LOC) into the Setup › Views sub-page and remove the `View` nav entry.
- Restyle it: consistent spacing and row layout with the other Setup sub-pages, and a clear header with the view name and pagination.
- Add **Test** next to **Apply**. Test saves the current bank (020), applies the view, and shows a "Testing view – Revert" bar. Revert restores the bank, and leaving the page reverts as well. Apply is unchanged.

## Out of scope
- Performance-side views (022).

## Key code
- `view_ui`, apply button (~line 407) — `crates/core/src/app/pages/view.rs`
- `View::apply_events` — `crates/shared/src/view.rs`
- Bank save/restore events — ticket 020

## Acceptance criteria
- [ ] Views are reachable only through Setup › Views.
- [ ] Test followed by Revert restores the previous overlay set exactly (`blctl.py state` before and after is equal).

## Verification
- `cargo clippy --all-targets --all-features` and `cargo fmt` are clean.
- `cargo test --all-features` passes, and new logic has co-located `#[cfg(test)]` tests.
- Follow **UI Verification** in `crates/core/AGENTS.md` (check that `swaylock` is not running, launch from the repo root, navigate via blctl, capture with grim). Use the headless Xvfb recipe when clicks are needed or another session is running (see memory notes). Take before and after screenshots, keep them in the scratchpad, and never commit them.
- Screenshot Setup › Views idle, during Test, and after Revert.

## Notes
<!-- Implementation notes, decisions, and follow-ups found while working. Keep this updated before renaming to .done.md. -->
