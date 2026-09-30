# 020 — Engine bank stack: save/restore overlay sets safely

| | |
|---|---|
| Epic | E6 Performance |
| Depends on | none |
| ABI / showfile | **ABI bump** (serialized shared types change) and **showfile shape** change (new fields need `#[serde(default)]`) (only if banks are persisted; runtime-only is fine) |

**Touches:** `crates/shared/src/state/engine.rs`, `crates/shared/src/view.rs`, `crates/shared/src/abi.rs`, engine event handling, `crates/plugin_framework/src/ (new bank module)`, `crates/core/src/plugin/ (cleanup on plugin error/reload)`

## Problem
Original review notes (2026-09-27):

> maybe extend view performance: multiple 'banks', each with multiple overlay scenes,
> User clarification: the engine has an internal way of pushing and popping banks. One bank is the current set of overlay scenes. The idea is a safe way for plugins to temporarily override the system state, e.g. save the current bank at a named address (e.g. 0), then apply a view as long as a button is held. This should make button holds more stable and less likely to break the system.

## Scope
- Define **Bank** = a snapshot of the current overlay set (`current_overlay_scenes` plus per-overlay masters/alpha, i.e. whatever `View::apply_events` touches).
- Engine operations as `ControlEvent`s appended at the end: `BankSave { slot }`, `BankRestore { slot }`, `BankPush`, `BankPop`, `BankClear { slot }`. Slots are keyed by `u8`, and each save records its owner (plugin id or UI).
- Safety: if a plugin that pushed or saved a bank errors, is disabled or is reloaded, its outstanding pushes are popped automatically. The stack depth is capped, with a warning when exceeded.
- plugin_framework helpers: `bank::save(slot)`, `bank::restore(slot)`, `bank::push()`, `bank::pop()`, and a `HoldView` guard (apply the view on press, restore on release or on drop).
- Add `blctl.py state` visibility of the bank stack.

## Out of scope
- UI for banks (021/022).
- Porting existing plugins (e.g. midi_all hold-overlay) to it. Add an Inbox entry in TODO.md for that follow-up.

## Key code
- `View`, `apply_events` — `crates/shared/src/view.rs`
- `current_overlay_scenes` — `crates/shared/src/state/engine.rs`
- Plugin error/reload paths — `crates/core/src/plugin/mod.rs`, `tick.rs`

## Acceptance criteria
- [ ] Unit tests: save, apply view, restore gives an identical overlay state; nested push/pop; auto-pop on plugin error.
- [ ] A blctl-driven sequence (`event` BankSave, then apply a view, then BankRestore) round-trips.
- [ ] ABI bumped and plugins rebuilt.

## Verification
- `cargo clippy --all-targets --all-features` and `cargo fmt` are clean.
- `cargo test --all-features` passes, and new logic has co-located `#[cfg(test)]` tests.
- If the ABI was bumped: `PLUGIN_ABI_VERSION` in `crates/shared/src/abi.rs` is incremented and `cd crates/plugins && make build_noopt` was run. The Makefile reports success even on failure, so grep its output for `^error`.
- Showfile compatibility: load a **scratch copy** of an existing showfile and confirm with `python3 scripts/blctl.py state` that it is not a silently booted default show.

## Notes
<!-- Implementation notes, decisions, and follow-ups found while working. Keep this updated before renaming to .done.md. -->
