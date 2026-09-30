# 019 — DMX overrides: move to System › DMX, layout per mode, active toggle

| | |
|---|---|
| Epic | E5 DMX overrides |
| Depends on | none |
| ABI / showfile | **ABI bump** (serialized shared types change) and **showfile shape** change (new fields need `#[serde(default)]`) |

**Touches:** `crates/core/src/app/pages/fixtures_setup.rs (remove)`, `crates/core/src/app/pages/system/dmx.rs`, `crates/shared/src/state/engine.rs (overrides)`, `crates/shared/src/abi.rs`, `crates/core/src/dmx/`, `crates/plugins/inspector + scripts/blctl.py (override/clear commands must keep working)`

## Problem
Original review notes (2026-09-27):

> dmx overrides look scuffed but fine
> more spacing on the left
> different dialog designs depending on desktop / integrated
> also add a toggle to activate / deactivate the override.

## Scope
- Move `render_dmx_override_dialog` / `render_dmx_override_create_dialog` (`fixtures_setup.rs:267-336`) into System › DMX. DMX overrides are output-level, not fixture setup.
- Change `overrides: BTreeMap<(u16,u16), u8>` to keep an `active` flag, e.g. `BTreeMap<(u16,u16), Override { value, active }>`, with serde compatibility for the old shape. Add a `ControlEvent` to toggle it, and make the DMX engine skip inactive overrides.
- Layout: more left padding. In the integrated (800x480) layout use a compact list with large touch toggles. In `desktop_mode` use a table (universe, channel, value slider, active toggle, delete).
- blctl `override`/`clear` still work (they create active overrides).

## Out of scope


## Key code
- Override dialogs — `crates/core/src/app/pages/fixtures_setup.rs:267-336`
- `EngineState.overrides`
- `BlaulichtApp.desktop_mode` — `crates/core/src/app/mod.rs:145`

## Acceptance criteria
- [ ] Toggling an override off restores the underlying value in DMX output without deleting the override.
- [ ] Old showfiles with overrides still load.
- [ ] Both layouts look deliberate.

## Verification
- `cargo clippy --all-targets --all-features` and `cargo fmt` are clean.
- `cargo test --all-features` passes, and new logic has co-located `#[cfg(test)]` tests.
- If the ABI was bumped: `PLUGIN_ABI_VERSION` in `crates/shared/src/abi.rs` is incremented and `cd crates/plugins && make build_noopt` was run. The Makefile reports success even on failure, so grep its output for `^error`.
- Showfile compatibility: load a **scratch copy** of an existing showfile and confirm with `python3 scripts/blctl.py state` that it is not a silently booted default show.
- Follow **UI Verification** in `crates/core/AGENTS.md` (check that `swaylock` is not running, launch from the repo root, navigate via blctl, capture with grim). Use the headless Xvfb recipe when clicks are needed or another session is running (see memory notes). Take before and after screenshots, keep them in the scratchpad, and never commit them.
- Screenshot System › DMX with overrides (one inactive), in 800x480 and in `--desktop-mode`.

## Notes
<!-- Implementation notes, decisions, and follow-ups found while working. Keep this updated before renaming to .done.md. -->
