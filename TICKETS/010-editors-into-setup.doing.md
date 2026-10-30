# 010 — Animation template editor and Palettes page become Setup sub-pages

| | |
|---|---|
| Epic | E2 Setup page |
| Depends on | 006, 012 |
| ABI / showfile | Possibly **ABI bump** if `AppPage` variants are removed or renamed (prefer keeping variants and hiding them from the navbar) |

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
- [ ] The navbar has no Animations/Palettes entries, and both editors work inside Setup.
- [ ] Old `NavigatePage` events land on the right sub-page.

## Verification
- `cargo clippy --all-targets --all-features` and `cargo fmt` are clean.
- `cargo test --all-features` passes, and new logic has co-located `#[cfg(test)]` tests.
- If the ABI was bumped: `PLUGIN_ABI_VERSION` in `crates/shared/src/abi.rs` is incremented and `cd crates/plugins && make build_noopt` was run. The Makefile reports success even on failure, so grep its output for `^error`.
- Follow **UI Verification** in `crates/core/AGENTS.md` (check that `swaylock` is not running, launch from the repo root, navigate via blctl, capture with grim). Use the headless Xvfb recipe when clicks are needed or another session is running (see memory notes). Take before and after screenshots, keep them in the scratchpad, and never commit them.
- Screenshot Setup › Animations and Setup › Palettes.

## Notes
<!-- Implementation notes, decisions, and follow-ups found while working. Keep this updated before renaming to .done.md. -->
