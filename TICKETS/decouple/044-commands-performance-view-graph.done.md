# 044 — Performance, View and Scene graph pages onto EngineClient

| | |
|---|---|
| Epic | E10 Decouple |
| Depends on | 042 |
| ABI / showfile | none (new operations go into `HostCommand`) |

**Touches:** `crates/core/src/app/pages/view.rs`, `view_perf.rs`, `scene_graph.rs`, `palettes.rs`, `crates/core/src/dmx/mod.rs`, `crates/core/src/state.rs`

## Problem
- `view.rs` writes the engine directly at `:62`, `:144`, `:185`, `:312` and `:469`, and deep-clones the state
  per frame at `:119` and `:335`.
- `scene_graph.rs` writes at `:285`, `:310` and `:344`. It saves by diffing and replacing the *whole* graph
  (`:336–344`), but the engine also mutates `scene_graphs` every tick, so replacing the whole graph loses
  updates. It also writes `focused_graph` directly (`:287–398`, `:867`).
- The grand master is written through `set_grand_master_percent` (`view_perf.rs:243`, `:252`, `:257`).

## Scope
- All reads go through `with_show`, and all writes are commands.
- Scene graph edits become granular node and edge commands (add or remove a node, move it, connect or
  disconnect an edge, set a parameter), sent when a drag ends or a connection is made, not every frame.
  `focused_graph` is session state (040): set it with the existing `SetFocusedSceneGraph`.
- The grand master becomes a `HostCommand::SetGrandMaster(u8)`. The quit-time reset at `ui.rs:530` moves in 052.
- Replace per-frame deep clones with scoped `with_show` blocks.

## Out of scope
- 020–023 (Performance features) and 038.
- The visualizer (051).

## Acceptance criteria
- [x] No production code in these files touches `self.data.state` or `AppState`.
- [x] Views, overlays, the grand master and the scene graph (drag, wire, delete, focus) work as before.
- [x] Unit test: a graph edit command while the engine ticks the graph loses no runtime state.

## Verification
- `cargo clippy --workspace --all-targets --all-features`, `cargo fmt`, `cargo test --workspace --all-features`.
- blenv: screenshots of each page, plus an interaction test that wires a scene graph node.

## Notes

### What changed
- **New `HostCommand`s** (`hub.rs`; engine side in the new `dmx/performance_commands.rs`, dispatched from
  `DmxEngine::apply_host` with the sender's session swapped in). All return `true` from `edits_show()` except
  `SetGrandMaster`.
  - `SetGrandMaster(u8)`: writes `AppState::grand_master_percent` (clamped to 100). Always `Ok`. Not saved in the
    showfile, so `edits_show()` is `false`.
  - Views: `CreateView { name }` (trimmed; captures `current_overlay_scenes`) → `CreatedId::View(id)` /
    "Name is empty" / "View limit reached". `DeleteView(id)` → "View not found" / "At least one view must
    remain". `AddViewOverlay { view, scene }` (no-op if already there) → "View not found" / "Scene not found"
    (also `BLANK`). `RemoveViewOverlay { view, scene }` (prunes masters) → "View not found".
    `SetViewSceneMasters { view, scene, masters }` → "View not found" / "Scene is not an overlay of this view".
  - Scene graphs (one node, edge or position per command, so the tick's `active_node`, `active_countdowns` and
    the runtime clock are never overwritten):
    - `CreateSceneGraph { name }` creates and focuses it **in the sender's session** (compound, like 043's
      create-and-place) → `CreatedId::SceneGraph(id)` / "Scene graph limit reached".
    - `DeleteSceneGraph(id)` clears the sender's focus and calls `repair_sessions()` (the 040 behaviour, now
      engine-side) → "Scene graph not found".
    - `AddSceneGraphNode { graph, x, y }` (create-and-place) → `CreatedId::SceneGraphNode { graph, node }` /
      "Scene graph not found" / "Node limit reached".
    - `RemoveSceneGraphNode { graph, node }` (drops its edges and the start if it was) → "Node not found".
    - `MoveSceneGraphNodes { graph, positions: Vec<(node, x, y)> }`, all or none, clamped to the canvas →
      "Node not found".
    - `ConnectSceneGraphNodes` / `DisconnectSceneGraphNodes { graph, from, to }` (connect adds a `Manual` edge,
      no duplicates) → "Node not found" / "Edge not found".
    - `SetSceneGraphNode { graph, node, param: SceneGraphNodeParam }` with `Name`, `MasterAlpha` (clamped to
      100), `MasterSpeed`, `AddScene`, `RemoveScene` → "Node not found" / "Scene not found".
    - `SetSceneGraphEdge { graph, from, to, param: SceneGraphEdgeParam }` with `Condition`, `Priority` →
      "Edge not found".
    - `SetSceneGraphStart { graph, node }` ("Set as Start") → "Node not found".
  - `CreatedId::{View(u8), SceneGraph(GraphId), SceneGraphNode { graph, node }}` are new.
  - `dmx::SCENE_GRAPH_CANVAS_HALF_EXTENT` (1500) is the engine's clamp; the page uses the same constant.
- **Existing events:** the graph tabs send `SetFocusedSceneGraph`, the Enabled checkbox `SetSceneGraphEnabled`.
  Both were added to `keeps_buffer` in `DmxEngine::apply`, so they no longer reset the sender's control buffer
  (seen in blenv: the Programmer's Alpha 108 stayed after switching graphs and toggling Enabled).
- **Client:** `EngineClient::grand_master_percent() -> u8` (new reader).
- **Shared page helpers** (`app/`): `commands.rs` has `send_command(cmd, on_ok) -> CommandSeq` (send, refusal
  popup, `on_ok` with the created id) and `show_refusal(why)`, lifted from 043's `send_setup_command`, which now
  wraps it and keeps its `setup_reply` tracking. `components/held.rs` has `Held<T, K = u8>` moved out of
  `programmer.rs` (with its test), generic over the key, plus `settle_row` for lists where several rows share one
  `Held` (only the held row settles).
- **Pages:**
  - `view_perf.rs`: one scoped `with_show` copies out the cards (id, name, master alpha/speed, removable), live
    mode and the overlay tree (no per-frame deep clone). The DMX simulation dialog clones `groups` only while it
    is open. The grand master reads `grand_master_percent()` and sends `SetGrandMaster`. Live rendering still uses
    `render_base_scene()` (041).
  - `view.rs`: both per-frame deep clones are scoped reads (view names, the selected view, scene names, overlay
    count; the picker's tree). Create sends `CreateView` and closes and selects the new view in the reply (OK is
    ignored while the reply is pending); delete sends `DeleteView` and picks the next selection in the reply;
    add/remove overlay and the row knobs send their commands. The row knobs (delta-based `Knob`/`SpeedKnob`) use
    `Held` keyed by (view, scene).
  - `scene_graph.rs`: reads names, the session's focused graph (`me.focused_graph`, a copy of that one graph),
    its countdown and scene names in one `with_session`. The viewer only edits the snarl; `SceneGraphUI::sync`
    reconciles snarl and engine each frame:
    - Operator edits are diffs against what the snarl was last synced from (`seen_pos`, `seen_edges`): wires
      added/removed become connect/disconnect (also right-click pin drops and ctrl-drag rewires, which the old
      code never wrote back), positions that differ become one `MoveSceneGraphNodes` once no pointer button is
      down (drag end; Auto Layout moves the snarl and the next sync sends it).
    - While the reply to such an edit is pending, nothing is taken from the engine (no snap-back); otherwise
      engine node adds/removes, moves and edge changes are applied to the snarl.
    - Node editor: name (typed text kept while the field has focus, so keystrokes aren't lost to the one-tick
      delay), alpha, speed, scenes, Set as Start and edge condition/duration/beats/priority send granular
      commands. Add Node / Delete Node / Create / Delete Graph use `send_command`.
    - `merge_scene_graph_edits` and its two tests are gone (replaced by the engine tests).

### Decisions and deviations
- **`CreateSceneGraph` focuses the new graph itself** (sender's session) rather than a follow-up
  `SetFocusedSceneGraph` from the reply: one atomic command, same pattern as 043's `CreateGroup { folder }`.
- **Edges are identified by `(from, to)`** (the page never creates duplicates; connect refuses none, it is a
  no-op).
- **View create closes on success** (it closed immediately before) so the new id can be selected; the dialog's
  text field is still not focused on open (pre-existing TODO).
- **Untouched pre-existing behaviour:** View Performance's scene card knobs already sent events (no `Held`, as
  before). `AddOverlayScene` still resets the control buffer (it did before).
- `palettes.rs` (Touches) was already moved by 043; `state.rs` needed no change (the setter is used by the
  engine now). The quit-time grand master reset in `ui.rs` and the showfile-load resets in
  `pages/system/showfile.rs` still call `set_grand_master_percent` directly (052 / 045).

### Verification
- `cargo test --workspace --all-features`: all pass; blaulicht-core 309 → 321. New `dmx::performance_commands::tests`
  (two-client harness):
  - `a_graph_edit_while_the_engine_ticks_the_graph_loses_no_runtime_state` (the acceptance test): a running graph
    moves from node 0 to node 1 by itself after the page's snapshot; then a name, a move, an edge priority and a
    new node are sent from desk and remote; the edits land, `active_node` stays 1, node 1's overlay stays and the
    countdown keeps running (not restarted). Ran 5x, stable.
  - `a_new_graph_is_focused_for_its_creator_only`, `deleting_a_graph_unfocuses_every_session`,
    `nodes_are_added_moved_and_removed_by_id`, `edges_are_connected_edited_and_disconnected`,
    `node_parameters_are_set_one_at_a_time`, `views_are_created_edited_and_deleted_by_id`,
    `the_grand_master_is_set_and_clamped`, `graph_focus_and_enable_keep_the_control_buffer` (all with refusals).
  - Page: `scene_graph::edit_tests::{the_snarl_shows_the_engine_graph, a_drag_is_sent_once_when_it_ends,
    wires_become_connect_and_disconnect_commands, another_graph_rebuilds_the_snarl}` and
    `held::tests::a_row_settles_only_its_own_held_value`.
- `cargo clippy --workspace --all-targets --all-features`: blaulicht-core lib 134, lib test 152 (baseline 141 /
  160); remaining warnings in touched files are pre-existing (`Stroke::new(1.0, ..)` fallback, the two unused
  consts in `view_perf.rs`, deprecated `ctx.run` in the old header test). `cargo build`: 84 lib warnings
  (unchanged).
- `cargo fmt`: clean; the usual unrelated hunks reverted.
- **blenv** (desktop layout, copy of `Kuze_Theater.json`, which has no graphs or views; screenshots read):
  - Scene Graph: "+" → Create → "New Graph" focused; Add Node x3; wired 0→1 and 1→2 by dragging pins (engine
    edges match); dragged node 1 (engine `pos` updated once, node stays put); selected it, typed "Chorus
    Section" (all characters arrived), alpha slider → 38 %, added Empty Scene, Set as Start, Enabled → the node
    turns green and its M. Alpha 38 shows on the Programmer; edge → After Duration: after 1 s the graph moved to
    the next node by itself; duration slider → 10 s and priority drag → 10 (engine state matches); right-click on
    node 0's output pin dropped the 0→1 wire in the engine; Auto Layout moved the nodes and the positions reached
    the engine; Delete Node → Confirm removed it and its edges; created "Second" (focused), switched back to
    "New Graph" (snarl rebuilt), Delete Graph → Confirm → "Select or create a graph."
  - View Performance: grand master drag → 45 %, BLACKOUT → 0 %, RESET → 100 %; ADD OVERLAY → Empty Scene card.
  - View: Add View → "New View" with the overlay captured and selected; Alpha knob drag 100 → 65 (engine
    `master_alpha: 65`); Remove → overlays empty, masters pruned; Add Overlay → back; second view created, Delete
    → Confirm → view 0 selected.
  - Logs: only the usual DMX/MIDI/XSETTINGS/XRandR/thread-priority lines.
  - Not compared against a before build: the scene graph canvas draws a little past its pane's bottom edge and
    the pane shows a horizontal scrollbar after deleting the last graph; the layout code is unchanged, so these
    are most likely pre-existing. Kiosk layout not captured separately. The View "Apply" button (unchanged code)
    was not clicked.

### For 045, 051, 052
- Use `self.send_command(cmd, |app, created| ..)` for "send, popup on refusal, follow up"; `Held<T, K>` from
  `components` for delta-based knobs (`settle_row` in lists).
- Pages that edit state the engine also changes every tick should send granular commands, and a widget that keeps
  its own model (like the snarl) should diff against what it last synced from, and pause engine sync while its
  own edit's reply is pending (see `SceneGraphUI::sync`).
- 052: `ui.rs`'s quit-time `set_grand_master_percent(100)` is the last UI write of the grand master (plus the two
  in `pages/system/showfile.rs` for 045/052); `SetGrandMaster` exists for them.

