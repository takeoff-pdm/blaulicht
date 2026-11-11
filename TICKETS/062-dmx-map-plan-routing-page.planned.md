# 062 — DMX and Art-Net page: Map, Plan and Routing

| | |
|---|---|
| Epic | E9 Console UI |
| Depends on | 058, 059, 061 |
| ABI / showfile | none |

**Touches:** `app/pages/system/` (DMX and Art-Net tabs), `crates/shared/src/page/sub.rs`

## Problem
The two output tabs hide the rig. The patch map found a real overlap in the show within minutes.

## Scope
- One page with Map, Plan and Routing view tabs in the page bar, merging the DMX and Art-Net sub-pages.
- Map: universes by number, fixture blocks with live output, group, type and output colouring, overlaps, find-free, inspector.
- Plan: sortable table with ranges as `x-y`, Group left of Fixture, narrow Ch column, Outputs written as `A 2 D 1`.
- Routing: matrix with Pending, Apply and Discard, and a status bar count until resolved.
- Patch stays read-only here.

## Out of scope
- Engine routing (061). Patch editing (037).

## Acceptance criteria
- [ ] A known overlap (DPACK #10 and New Fixture #1) is flagged at the right channel.
- [ ] Problem links open the affected universe.
- [ ] Pending routing stays visible after leaving the tab.

## Verification
- `cargo clippy --all-targets --all-features`, `cargo fmt`, `cargo test --all-features`.
- UI: run with `--desktop` via `scripts/blenv` and compare screenshots against the mockup, in dark and light theme, at 1280x800 and 2560x1440. Confirm the 800x480 kiosk is unchanged.

## Notes
Mockup: `system.html#dmx`, with research in `patch-references.html`. Live per-channel output may need new engine data. Channel display (0-based or 1-based) is still undecided.
