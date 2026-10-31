# 040 — Operator sessions in the engine

| | |
|---|---|
| Epic | E10 Decouple |
| Depends on | none |
| ABI / showfile | none intended (see Scope); showfile shape unchanged |

**Touches:** `crates/shared/src/state/engine.rs`, `crates/shared/src/event.rs`, `crates/core/src/dmx/mod.rs`, `crates/core/src/dmx/state.rs`, `crates/core/src/plugin/tick.rs`, `crates/core/src/plugin/wasm.rs`, `crates/core/src/config.rs`, `crates/core/src/app/**` (readers of the moved fields)

## Problem
`ControlEvent`s are stateful: `SetColor((r,g,b))` has no target. The engine applies it to `EngineState.selection`
and writes it into `current_scene_focus`. Those fields, along with `selection_stack` and `control_buffer`, sit in
the shared show state, so every client would share one selection. MIDI and plugin controllers (`midi_all/korg.rs`,
`midi_all/legacy`, `console`, `ddj_*`) depend on this protocol and have no UI of their own to keep a selection in.

Fix it the way web servers do: keep this state in an engine-side session, one per client.

## Scope
- Add `SessionId` (with `SessionId::DESK`) and `OperatorSession { selection, selection_stack, control_buffer,
  scene_focus }`, plus a session store owned by the engine.
- `ControlEventMessage` gets a `session: SessionId`. The UI and all plugins send `DESK` for now. Plugins call
  `bl_send_event` with a bare `ControlEvent` and the host wraps it (`wasm.rs:875`), so the plugin ABI doesn't
  change.
- `apply` (`dmx/mod.rs:730`), `apply_on_selection_and_scene` (`:1225`) and every other engine reader resolve
  selection and focus against the message's session.
- **Plugin ABI stays as it is:** plugins receive a serialized `EngineState` (`tick.rs:234`) and read
  `selection` / `current_scene_focus` (about 25 sites across console, ddj, korg, midilight, player, inspector).
  Keep those fields in the shared struct, and have the host fill them from the plugin's session in the copy it
  serializes. Document on each field that it is a projection of the caller's session, and add a debug assertion
  that the engine's canonical copy holds only defaults. Removing the fields from `EngineState` is a later ABI
  cleanup; add it to the `TODO.md` Inbox.
- **Showfile:** `SaveEngineState.current_scene_focus` stays. Load seeds the `DESK` session's focus; save writes it.
- Move the native UI's ~70 readers of `.selection` and the `current_scene_focus` readers onto the `DESK` session.
  This is mechanical; don't restructure pages here.
- Tests: two sessions with different selections each apply `SetColor` and change only their own fixtures; the
  selection stack and focus are per session.

## Out of scope
- `live_mode` (041).
- Remote clients creating sessions (047).

## Key code
- `EngineState` — `crates/shared/src/state/engine.rs:55`
- `ControlEventMessage` / `EventOriginator` — `crates/shared/src/event.rs:18`, `:60`
- Engine `apply` and `SelectGroup` handling — `crates/core/src/dmx/mod.rs:730`, `:800`
- `apply_on_selection_and_scene` — `crates/core/src/dmx/mod.rs:1225`
- Plugin state serialization — `crates/core/src/plugin/tick.rs:234`, `:332`
- Showfile focus — `crates/shared/src/state/save.rs:78`

## Acceptance criteria
- [ ] Two sessions can hold different selections and focus at the same time, covered by unit tests.
- [ ] Selecting a group on a MIDI controller (or with the `console` plugin) still highlights it on the native UI.
- [ ] Plugins still see the selection, unchanged, without being rebuilt.
- [ ] An existing showfile loads with the same focused scene as before.

## Verification
- `cargo clippy --all-targets --all-features`, `cargo fmt`, `cargo test --all-features`.
- Load a scratch copy of an existing showfile and confirm with `python3 scripts/blctl.py state` that it booted the
  real show and the focus survived.
- UI: screenshot after a plugin-driven group selection.

## Notes
