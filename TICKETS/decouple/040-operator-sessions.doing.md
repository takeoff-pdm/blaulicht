# 040 — Operator sessions in the engine

| | |
|---|---|
| Epic | E10 Decouple |
| Depends on | 049 |
| ABI / showfile | none: `DESK` stays mirrored in `EngineState`, and the session id travels in 049's host-only envelope |

**Touches:** `crates/core/src/dmx/mod.rs`, `crates/core/src/dmx/state.rs`, `crates/core/src/dmx/management.rs`, `crates/shared/src/folder.rs` (only if a helper is needed), `crates/shared/src/state/engine.rs`, `crates/core/src/plugin/tick.rs`, `crates/core/src/app/event.rs`

## Problem
`ControlEvent`s are stateful: `SetColor((r,g,b))` has no target. The engine applies it to
`EngineState.selection` and writes it into `current_scene_focus`. Those fields, `selection_stack`,
`control_buffer` and `scene_graphs.focused_graph` belong to one operator. MIDI and plugin controllers (korg,
legacy, console, ddj) depend on this protocol.

## Scope
- **Session store:** `OperatorSession { selection, selection_stack, control_buffer, scene_focus,
  focused_graph }` for every session except `DESK`. Keep it in the core wrapper (`EngineStateRef`,
  `dmx/state.rs`) under the same lock as `EngineState`, **not** in the shared struct.
- **`DESK` stays in the existing `EngineState` fields.** Plugins (who get the serialized state at
  `tick.rs:234`), the showfile (`SaveEngineState.current_scene_focus`, `scene_graphs.focused_graph`) and the
  ~57 UI reader lines don't change.
- **Swap in when applying an event:** for an event from session S ≠ `DESK`, the engine swaps S's fields into
  `state.0` before `apply` and back afterwards. Do the swap *outside* the transaction snapshot
  (`dmx/mod.rs:753`, `:762`), so a transaction rolls back the session it ran in. All existing readers and
  writers (dmx/mod.rs, management.rs, folder.rs `SelectFolder`) then become session-aware without being
  rewritten. `control_buffer.reset()` (`dmx/mod.rs:747`) then resets only the sender's buffer.
- **Repair every session after structural changes.** After any event that deletes or changes groups, fixtures
  or scenes, prune the side-store sessions: drop missing groups and fixtures from the selection and stack, and
  reset focus to a valid scene, the same way `delete_scene` already repairs `DESK` (`engine.rs:224–231`). Patch
  moves and swaps in `management.rs` (`:466–475`, `:548–549`) then end up pruning instead of moving for
  non-desk sessions. That is acceptable; document it.
- **Filter session-scoped events by session where they're consumed:**
  - Plugins' `TickInput` gets only `DESK` events plus global ones. Otherwise a remote client's selection would
    light up the korg's LEDs (`korg.rs:304–318`).
  - The native UI's `handle_events` (jog target `recent_fixture_property`, dirty marking) ignores
    session-scoped events from other sessions (`app/event.rs:34–46`, `:214–231`).
  - Which events count as session-scoped: selection, focus and the control-buffer events. Write the list in
    code, with a test.
- **Session lifecycle:** create and drop sessions through the hub (049). New sessions start with focus on
  `DESK`'s focus and an empty selection.
- **Tests:**
  - Two sessions select different groups, and `SetColor` changes only each one's own fixtures.
  - Stack and focus are per session.
  - Deleting a group prunes a remote session's selection.
  - Deleting a scene repairs a remote session's focus.
  - A transaction from a remote session rolls back only that session.

## Out of scope
- Live preview (041).
- UI code (042+).

## Key code
- `apply` — `crates/core/src/dmx/mod.rs:730`; control buffer reset `:747`; transaction `:753`
- `SelectGroup` — `crates/core/src/dmx/mod.rs:800`; `SetSceneFocus` `:993`; `RemoveChange` `:927`
- `apply_on_selection_and_scene` — `crates/core/src/dmx/mod.rs:1225`
- Patch edits — `crates/core/src/dmx/management.rs:128`, `:384`, `:466`, `:548`
- Folder selection — `crates/shared/src/folder.rs:399–420`, rollback `:489–502`
- `delete_scene` / `clone_scene` — `crates/shared/src/state/engine.rs:175`, `:224`

## Acceptance criteria
- [ ] All tests above pass.
- [ ] Single-operator behaviour is unchanged, including korg and console selection showing on the native UI.
- [ ] Plugins load without being rebuilt.

## Verification
- `cargo clippy --workspace --all-targets --all-features`, `cargo fmt`, `cargo test --workspace --all-features`.
- blenv: load a scratch copy of a showfile, select via `blctl event`, then check the Programmer page.

## Notes
