# 050 — Programmer onto EngineClient

| | |
|---|---|
| Epic | E10 Decouple |
| Depends on | 042 |
| ABI / showfile | none (new operations go into `HostCommand`) |

**Touches:** `crates/core/src/app/pages/programmer.rs`, `crates/core/src/app/pages/programmer_animations.rs`, `crates/core/src/dmx/mod.rs`, `crates/core/src/plugin/wasm.rs`

## Problem
Programmer still uses `AppState` directly:
- Production writes: `programmer.rs:299` (scene master alpha and speed) and `programmer_animations.rs:205`,
  `:221` (`toggle_preset`, which reports "Animation limit reached").
- `clone_animation_instance_state` (`wasm.rs:267`) writes plugin state storage and
  `wasm_animation_instances_to_reset`. It is called from `programmer.rs:818`, `:855` and
  `programmer_animations.rs:259`.

The direct writes at `programmer.rs:1363`, `:1372` and `programmer_animations.rs:767`, `:775`, `:810` are in
`#[cfg(test)]` code and may stay.

## Scope
- All reads go through `with_show` and all writes are commands.
  - `programmer.rs:299` uses the existing `SetSceneMasterAlpha` / `SetSceneMasterSpeed` events.
  - `toggle_preset` becomes a `HostCommand`, and its refusal ("Animation limit reached") comes back as
    `CommandResult::Refused`, shown as today.
  - Cloning animation instance state becomes a `HostCommand` handled by the engine.
- Drags and faders keep a local value while they are being dragged, so the one-frame delay doesn't show.

## Out of scope
- Programmer redesign (036).

## Acceptance criteria
- [ ] No production code in `programmer*.rs` references `AppState`, `self.data.state` or a lock.
- [ ] Selection, attributes, palette apply, animation presets (including the limit message), scene change with
      undo-all and LIVE all work as before.

## Verification
- `cargo clippy --workspace --all-targets --all-features`, `cargo fmt`, `cargo test --workspace --all-features`.
- blenv: before and after screenshots, plus interactions: select a group, set a color, apply a palette, toggle an
  animation preset.

## Notes
