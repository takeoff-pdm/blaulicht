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
- [x] All tests above pass.
- [x] Single-operator behaviour is unchanged, including korg and console selection showing on the native UI.
- [x] Plugins load without being rebuilt.

## Verification
- `cargo clippy --workspace --all-targets --all-features`, `cargo fmt`, `cargo test --workspace --all-features`.
- blenv: load a scratch copy of a showfile, select via `blctl event`, then check the Programmer page.

## Notes

### What changed
- **`crates/core/src/dmx/session.rs` (new)**, re-exported from `dmx`:
  - `OperatorSession { selection, selection_stack, control_buffer, scene_focus, focused_graph }`,
    `type SessionStore = BTreeMap<SessionId, OperatorSession>`, and the borrowed read view
    `SessionView<'a> { selection, selection_stack, control_buffer, scene_focus, focused_graph }`.
  - On the core `EngineState`, for everyone: `open_session(id) -> bool`, `close_session(id) -> bool`,
    `has_session(id)` (always true for `DESK`), `session(id) -> Option<SessionView>` (for `DESK` it reads the
    shared fields), `scene_focus_of(id) -> Option<u8>`, `remote_sessions()` and `repair_sessions()`.
  - Only inside `dmx`: `swap_session(id)` (an involution: it swaps the session's five fields with the `DESK`
    fields in `.0`), `remote_sessions_mut()` and `reset_sessions()`.
- **The core wrapper** is now `EngineState(pub blaulicht_shared::EngineState, pub(super) SessionStore)`, so every
  `.0` reader is unchanged and only `dmx` sees the store. The unused `Serialize`/`Deserialize` derives are gone
  (nothing serialized the wrapper; the showfile and plugins use `.0`). `clone_for_ui` clones the store too.
- **Engine (`dmx/mod.rs`).** `dispatch` refuses a message from a session that is not open with
  `Refused("Unknown session")`. Otherwise it swaps the sender's session in, applies the message (`apply` for a
  `ControlEvent`, `apply_host` for a `HostCommand`) and swaps it back, all under the engine lock. The swap is
  outside `apply`, so the `Transaction` snapshot holds the sender's session and a rollback restores only that
  session. `control_buffer.reset()` resets only the sender's buffer. `apply_bus_messages` drains the bus, then takes
  the hub's session changes: it opens new sessions before dispatching and closes dropped ones after, so a client
  that sends and then disconnects within one tick still gets its messages applied.
- **Hub (`hub.rs`).**
  - `SystemHub::open_session() -> SessionHandle` (ids start at 1 and are never reused);
    `SessionHandle::id()`; dropping the handle closes the session.
  - `take_session_changes() -> Vec<SessionChange { Opened, Closed }>` is for the engine.
  - `ClientConnection::connect_new_session(id, &hub, &bus, &mirror)` opens a session and closes it when the
    connection drops.
  - `impl Display for SessionId`.
  - **Session-scoped events**: `SessionScope { Global, Session, Control }` and `session_scope(&ControlEvent)`, an
    exhaustive match. `Session` covers the selection events, `SelectFolder`, `SetSceneFocus` and
    `SetFocusedSceneGraph`. `Control` covers `CONTROLS_REQUIRING_SELECTION!()` and `RemoveChange`, the events
    that act on the selection through the control buffer. A `Transaction` takes the widest scope of its events.
    `BusMessage::concerns_session(s)` is true for messages from `s` and for global messages from other sessions.
- **Plugins.** `hub::plugin_events` (what `TickInput` is built from in `tick.rs`) drops messages that don't concern
  `DESK` before the envelope is stripped, so a remote selection never reaches the korg's LEDs.
- **Desk (`app/event.rs`).** `handle_events` keeps the envelope. The jog target (`recent_fixture_property`) only
  observes messages that concern the desk's session. `control_event_marks_showfile_dirty(event, own_session)`
  ignores `Session`-scoped events from other sessions.
- **Repairing every session after structural changes:**
  - `delete_group`, `delete_fixture_from_group` and the wrapper's `delete_scene` call `repair_sessions`. Repair
    drops missing groups and fixtures from selections and stacks. A focus on a missing scene or on `BLANK` moves
    to the first user scene, as `delete_scene` does for `DESK`. A missing focused graph becomes `None`.
  - `load_showfile` keeps the sessions open but resets them as if they were new.
  - The Scene Graph page's graph delete (a direct UI write) calls `repair_sessions` too; that is one line in UI code.

### Decisions and deviations
- **Dirty marking** deviates from the ticket's wording: control events from another session (`SetColor`, ...)
  still mark the show dirty, because they edit scenes, and skipping them would let remote edits go unsaved. Only
  `Session`-scoped events from other sessions are ignored. That covers their focus changes, which used to mark the
  show dirty for the desk because the showfile saves the desk's focus.
- **"Control-buffer events" count as session-scoped**, so plugins don't see a remote operator's `SetColor`
  either. A remote `Transaction` that mixes global and session events is hidden from plugins as a whole.
- **Patch moves and swaps** remap remote selections the same way they remap `DESK`'s, instead of pruning them
  (the ticket allowed pruning). `repair_sessions` then runs after a move, because `move_engine_selection` keeps
  the other filtered fixture ids and reads them in the target group (an existing quirk, unchanged for `DESK`).
- **Group and fixture deletion** still clears `DESK`'s selection and stack, as before, but only prunes other
  sessions. If pruning removes every fixture in a fixture filter, the group is dropped too, so the selection
  never widens to the whole group.
- **Host commands also run with the sender's session swapped in**, so later commands like "clone the focused
  scene" act on the sender's focus without extra code.
- **Session lifecycle runs through the hub** (a pending list the engine drains each tick), not through a
  `HostCommand`. A new session also copies `DESK`'s focused scene graph, along with its scene focus.
- **Live mode** events stay `Global` (041).
- **Unknown sessions:** the engine refuses the message. Plugins read the bus directly, so they still get the
  message if its event is global.

### Verification
- `cargo test --workspace --all-features`: all pass. blaulicht-core went from 263 to 280 tests:
  - `dmx::session::engine_tests` (9, against a real `DmxEngine`):
    - two sessions `SetColor` their own groups only, and a selection event resets only the sender's control
      buffer;
    - stack, focus, writes into the focused scene, `SelectFolder` and the focused graph are per session;
    - deleting a group or a fixture prunes a remote session, and the desk is cleared as before;
    - deleting a scene repairs a remote session's focus;
    - a remote transaction rolls back only its own session;
    - a patch move or swap carries a remote session's selection;
    - loading a show resets remote sessions;
    - an unknown session is refused;
    - open and close through the hub with a real bus thread, including use right after
      `connect_new_session`.
  - `dmx::session::tests` (4): a new session starts on the desk focus; swap is an involution; pruning never
    widens a selection; repair of scene and graph focus.
  - `hub` (3): the session-scoped list, written out in full; plugins see only `DESK`'s and global events; session
    open and close order, fresh ids, and `connect_new_session` closing on drop.
  - `app::event` (1): dirty marking for other sessions.
  - 049's `command_result_tests` now opens a real session for its remote client.
- `cargo clippy --workspace --all-targets --all-features`: blaulicht-core lib 141, lib test 160, the same as after
  049. `cargo build`: 84 lib warnings. None of them is in new code.
- `cargo fmt`: clean for the touched files. The pre-existing reformatting of `audio_features`, `pages/audio.rs`,
  `visualizer/ui.rs` and `app/ui.rs` was reverted.
- `PLUGIN_ABI_VERSION` is unchanged (18). blenv loaded all five plugin bundles, which were built before this
  ticket.
- blenv (kiosk, a copy of `Kuze_Theater.json`, screenshots read), on the Programmer page:
  - Before: nothing selected.
  - `blctl event SelectGroup(6)`, `SetAlpha(200)` and `SetColor([0,128,255])` (inspector, plugin origin, `DESK`
    session): "Par Truss" is highlighted with its four fixtures, Alpha shows 200, Hue 209 and Sat/Val 255, the
    swatch is blue, and the unsaved-changes dot appeared.
  - `PushSelection`, `SelectGroup(0)` and `LimitSelectionToFixtureInCurrentGroup(1)`: Frontlicht is selected
    with "Linse ML" limited, and the inspector shows `selection_stack: [{6}]`.
  - `PopSelection`: back to Par Truss, with an empty stack.
  - The desk's own input still works: clicking "Fluter" added group 5 (`{5, 6}`), and dragging Alpha set it
    to 182.
  - No errors in the app log.
  - Not checked live, because no in-tree client opens a second session: remote sessions (covered by the tests
    above) and the Scene Graph page's graph delete.

### Follow-ups
- **041:**
  - Read a remote owner's focus with `EngineState::scene_focus_of(owner)`, and check `has_session(owner)`.
    Sessions close in `DmxEngine::apply_bus_messages` (the `SessionChange::Closed` loop), which is where the
    preview should be cleared.
  - `set_live_mode` now resets the animations of the *sender's* focused scene, because of the swap, while
    `render_base_scene` still renders `DESK`'s focus.
- **042:**
  - A `LocalClient` for a second session is `hub.allocate_client()` plus
    `ClientConnection::connect_new_session`. It reads its session with
    `with_show(|s| s.session(client.session()))`.
  - The core helpers `get_selection()` and `curr_scene()`, and `clone_for_ui` (which keeps only the focused
    scene), are `DESK`-only. Pages for another session need session-aware variants.
- **043+:**
  - Structural `HostCommand`s run with the sender's session swapped in. For example, `delete_group` from a remote
    session clears *that* session's selection and only prunes `DESK`'s, which sits in the store at that moment.
    Swap back first if `DESK` must always be cleared.
  - UI code that changes structure directly must call `repair_sessions()`. The wrapper methods already do.
- **047:** a remote client is `allocate_client()` plus `connect_new_session`, and dropping the connection closes
  the session.
- If `apply` panics while a remote session is swapped in, the store keeps `DESK`'s fields. The engine lock is
  poisoned in that case anyway; that is not new.
- As 039 noted, the view-state mirror is per client, not per session.
