# 014 — Bug: pointer to a color palette shows a different color

| | |
|---|---|
| Epic | E3 Programmer |
| Depends on | none |
| ABI / showfile | **ABI bump only if** a serialized value representation changes; prefer a fix that doesn't |

**Touches:** `crates/shared/src/palette.rs`, `crates/core/src/app/pages/palettes.rs`, `possibly crates/shared/src/fixture/ (FixtureState color resolve)`

## Problem
Original review notes (2026-09-27):

> also palettes are a bit fucked?
> color gets skewed
> User clarification: when we use a pointer to another color palette, the pointer shows a different color than the original, even though no transformation was made.

## Scope
- Reproduce with a unit test: `Color(hsv)` palette A and `Pointer { target: A, property: None, ops: [] }` palette B. Resolving B must give exactly the same RGB as A, for several hues/saturations/values including edge cases (h=359.9, s=v=1.0, low s).
- Likely causes to check:
    - `extract_leaf_property` truncates with `as u16`: hue in whole degrees, S/V mapped to 0..255 and truncated (`palette.rs:~240`).
    - `FixtureState::apply_value` / `resolve` may expect a different range.
    - The swatch for direct palettes uses stored RGB while pointers go through HSV → u16 → FixtureState → RGB (`palette_resolved_rgb`, `palettes.rs:972`).
- Fix it so an op-less pointer is lossless (e.g. round instead of truncate and use the full u16 range, or resolve colors in float end to end), and check the same path the engine uses for real DMX output, not only the swatch.

## Out of scope
- Partial attribute apply (015).

## Key code
- `resolve_via_chain`, `apply_pointer_ops`, `extract_leaf_property` — `crates/shared/src/palette.rs:174-260`
- `palette_resolved_rgb` — `crates/core/src/app/pages/palettes.rs:972`

## Acceptance criteria
- [ ] The new unit tests pass (`cargo test -p blaulicht-shared`).
- [ ] Side-by-side swatches of A and B look identical.
- [ ] DMX output for a fixture using B equals output using A.

## Verification
- `cargo clippy --all-targets --all-features` and `cargo fmt` are clean.
- `cargo test --all-features` passes, and new logic has co-located `#[cfg(test)]` tests.
- If the ABI was bumped: `PLUGIN_ABI_VERSION` in `crates/shared/src/abi.rs` is incremented and `cd crates/plugins && make build_noopt` was run. The Makefile reports success even on failure, so grep its output for `^error`.
- Follow **UI Verification** in `crates/core/AGENTS.md` (check that `swaylock` is not running, launch from the repo root, navigate via blctl, capture with grim). Use the headless Xvfb recipe when clicks are needed or another session is running (see memory notes). Take before and after screenshots, keep them in the scratchpad, and never commit them.
- Screenshot the Palettes page with a color palette and a pointer to it side by side.

## Notes
<!-- Implementation notes, decisions, and follow-ups found while working. Keep this updated before renaming to .done.md. -->
