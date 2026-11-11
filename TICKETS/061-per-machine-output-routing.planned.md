# 061 — Per-machine universe to output routing

| | |
|---|---|
| Epic | E9 Console UI |
| Depends on | none |
| ABI / showfile | check showfile impact; routing itself goes in machine config, not the showfile |

**Touches:** `crates/engine/src/config.rs:49` (`dmx_out_devices`), `crates/engine/src/state.rs:147` (`ArtNetReceiver`), `crates/engine/src/dmx/`, `crates/engine/src/hub.rs`

## Problem
Output devices are a fixed pair, and Art-Net receivers take contiguous universe ranges. Agreed with the maintainer: routing is saved per machine, Art-Net receivers may have gaps, and one universe can go to several outputs.

## Scope
- Explicit universe-to-output mapping in machine config, with migration from `dmx_out_devices` and from the receiver range and `sends_all_universes`.
- One universe to many outputs; non-contiguous universes per Art-Net receiver.
- Host commands to read and apply routing as one atomic change.
- Report per-output state and which universes reach it.

## Out of scope
- The routing page (062).

## Acceptance criteria
- [ ] An existing config migrates with the same output.
- [ ] One universe verifiably reaches two outputs, and a receiver accepts universes 2, 3 and 9.
- [ ] Routing is unchanged when another showfile loads.

## Verification
- `cargo clippy --all-targets --all-features`, `cargo fmt`, `cargo test --all-features`.
- Live check with DMX or Art-Net capture.

## Notes
Decide while implementing how to keep the "All universes" receiver mode. It is missing from the mockup.
