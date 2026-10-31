# 042 — EngineClient trait and LocalClient

| | |
|---|---|
| Epic | E10 Decouple |
| Depends on | 049, 040, 041 |
| ABI / showfile | none |

**Touches:** new `crates/core/src/client.rs`, `crates/core/src/app/mod.rs`, `crates/core/src/state.rs`, `crates/core/src/app/event.rs`

## Problem
Pages read and write `Arc<AppState>` directly: about 157 lock sites in `app/`, 64 of them `.write()`. A frontend
should only see "read show state, read my session, send commands, get results, subscribe to streams", so the
same page code can later run against a remote engine.

How pages read today:
- `clone_for_ui` per frame: `programmer.rs:83`, `palettes.rs:414`.
- A full deep `.clone()` per frame: `view.rs:119`, `:335`, `view_perf.rs:206`, `setup/mod.rs:41`,
  `animations.rs:408`, `scene_graph.rs:338`.
- A read guard held while drawing: `fixtures_shared.rs:351`, `:488`, `scenes.rs:384`, `visualizer/data.rs:110`.

The engine write-locks `EngineState` on every tick (animation timers, `dmx/animation/mod.rs:834`; scene graph
runtime, `dmx/mod.rs:540`), so "publish a snapshot on change" would mean a full copy every tick.

## Scope
- `trait EngineClient`:
  - `fn with_show<R>(&self, f: impl FnOnce(&EngineState) -> R) -> R`: a read guard locally; an owned snapshot
    remotely. Pages scope it per widget block and must not send commands from inside it.
  - `fn session(&self) -> SessionId`, `fn client(&self) -> ClientId`.
  - `fn send(&self, body: BusBody) -> CommandSeq` and `fn take_results(&self) -> Vec<CommandResult>` (per
    frame), plus a small helper for "send, then run this closure on the matching result" for dialogs.
  - Read-only stream accessors: DMX buffers, health, tick speeds, spectrogram, plugin UI ops, logs and startup
    stages (from 049's subscription), audio params, Art-Net receivers, plugin list.
  - The trait doesn't need to be object-safe if generics are simpler; settle the shape and write it in Notes.
- `LocalClient`: holds the `Arc<AppState>` privately, plus the bus and hub handles. `BlaulichtApp` gets a
  `client: LocalClient` field. Pages and helpers that take `&AppStateWrapper` (`fixtures_shared.rs`,
  `plugin_ui.rs`, `animation_spec_editor.rs`) gain a path to the client. Their bodies move in 043–052.
- **Performance:** measure frame times on Programmer, View and Setup › Fixtures before and after (blenv, the same
  showfile) and write them in Notes. It must not be slower.
- **Test harness:** a test helper that starts the engine without UI and gives you two `LocalClient`s on different
  sessions, so 043–052 can assert isolation.

## Out of scope
- Moving page code onto the client (050, 043, 044, 051, 045, 052).
- Remote transport (047).

## Key code
- `clone_for_ui` — `crates/core/src/dmx/state.rs:31`
- `AppStateWrapper` — `crates/core/src/state.rs:30`
- Pages are `impl BlaulichtApp` methods using `self.data` — `crates/core/src/app/mod.rs`

## Acceptance criteria
- [x] `LocalClient` exists, and `BlaulichtApp` owns one.
- [x] The two-client test harness works, with an isolation test (two sessions, two selections).
- [x] No behaviour change; frame times are recorded.

## Verification
- `cargo clippy --workspace --all-targets --all-features`, `cargo fmt`, `cargo test --workspace --all-features`.
- blenv smoke test with screenshots.

## Notes

### What changed
- **`crates/core/src/client.rs` (new).**
  - `trait EngineClient` (not object-safe; the readers take closures generically):
    ```rust
    fn client(&self) -> ClientId;
    fn session(&self) -> SessionId;
    fn with_show<R>(&self, f: impl FnOnce(&EngineState) -> R) -> R;
    fn with_session<R>(&self, f: impl FnOnce(&EngineState, SessionView<'_>) -> R) -> Option<R>; // provided
    fn send(&self, body: impl Into<BusBody>) -> CommandSeq;
    fn send_event(&self, event: ControlEvent) -> CommandSeq; // provided, EventOriginator::Web
    fn take_results(&self) -> Vec<CommandResult>;
    fn try_recv_system(&self) -> Option<SystemMessage>; // without CommandResults
    fn try_recv_bus(&self) -> Option<BusMessage>;
    fn with_dmx_universe<R>(&self, universe: usize, f: impl FnOnce(&[u8; 513]) -> R) -> Option<R>;
    fn with_health<R>(&self, f: impl FnOnce(&AppHealthState) -> R) -> R;
    fn with_spectrogram<R>(&self, f: impl FnOnce(&AudioSpectrogram) -> R) -> R;
    fn with_plugin_ui_ops<R>(&self, f: impl FnOnce(&HashMap<u8, Vec<WasmUiOp>>) -> R) -> R;
    fn with_audio_params<R>(&self, f: impl FnOnce(&SignalCollectorParams) -> R) -> R;
    fn with_artnet_receivers<R>(&self, f: impl FnOnce(&[ArtNetReceiver]) -> R) -> R;
    fn with_plugins<R>(&self, f: impl FnOnce(&HashMap<u8, PluginState>) -> R) -> R;
    fn publish_ui(&mut self, snapshot: FrontendUiSnapshot);
    ```
  - `LocalClient` owns the client's `ClientConnection`, plus `Arc<AppState>`, `SystemHub` and
    `EventBusHandle<BusMessage>`, all private. Constructors: `LocalClient::connect(id, session, app, &hub, &bus)`
    (the desk is `ClientId::Desk`, `SessionId::DESK`), `LocalClient::connect_new_session(app, &hub, &bus)` (allocates
    `ClientId::Remote(n)` and opens a session that closes on drop) and `open_client(&self)` (another client of the
    same engine in a new session). Readers take a read guard and ignore lock poisoning. `send` wraps the body in
    the client's envelope and sends it on the bus channel, with no serialization.
  - Results: `take_results` drains the hub subscription. Results are returned. Other system messages are kept
    (capped at `SUBSCRIBER_CAPACITY`, warned once) for `try_recv_system`, which in turn moves results it reads past
    into the result list. Both orders lose nothing. The two buffers sit behind one `Mutex`, so `LocalClient` is
    `Send + Sync`.
  - `ReplyHandlers<Ctx>` for "send, then run this closure on the matching result":
    `send_then(&mut self, client: &impl EngineClient, body, on_result: impl FnOnce(&mut Ctx, &CommandOutcome) + 'static) -> CommandSeq`,
    `dispatch(ctx: &mut Ctx, handlers: fn(&mut Ctx) -> &mut Self, results: &[CommandResult])`, `is_waiting(seq)`,
    `len`, `is_empty`. Results nobody waits for are ignored.
  - `client::testing` (`#[cfg(test)]`, `pub(crate)`): the two-client harness, see below.
- **Desk.** `BlaulichtApp` has `client: LocalClient` and `reply_handlers: ReplyHandlers<BlaulichtApp>`. Both are
  private fields; pages are `impl BlaulichtApp` blocks in descendant modules, so they reach them as `self.client` and
  `self.reply_handlers`. `BlaulichtApp::new(cc, state, client, ..)` takes the client. `main.rs` connects it at the
  same point as before (before the supervisor starts). `AppStateWrapper` lost its `client` field and is now
  documented as the legacy path that 043–052 remove.
  - `handle_events` reads the bus and system streams through the client, as before. After its loop it calls
    `take_results()` and `ReplyHandlers::dispatch(self, |app| &mut app.reply_handlers, &results)`. Nothing registers a
    handler yet, so results are still dropped, as before.
  - Every existing desk send only changed its path (`self.data.client.send(..)` became `self.client.send(..)`,
    with `use crate::client::EngineClient`). `send` accepts `impl Into<BusBody>` (new `From<ControlEventMessage>` and
    `From<HostCommand>` impls for `BusBody` in `hub.rs`), so the call shape is unchanged.
  - Helpers: `render_plugin_ops(.., client: &LocalClient, ..)` replaces its `data: &AppStateWrapper` parameter (it only
    sent through it). `AnimationEditState::show` / `anim_wasm_ui` gained `client: &LocalClient` next to `data` (their
    bodies still read `data.state`). `fixture_controls` takes `&LocalClient` instead of `&ClientConnection`.
    `plugin_ui_title(&AppStateWrapper, ..)` is unchanged (its body reads `plugins`).
- **Session-aware reads.** `SessionView::fixture_selection(&groups)` is the session-aware `get_selection()`. Both use
  the shared `dmx::state::resolve_selection`, so the code is moved, not changed. With `with_session(|show, me| ..)` a
  page reads `me.selection`, `me.fixture_selection(&show.0.groups)`, `show.get_scene(me.scene_focus)` (instead of
  `curr_scene()`), `me.control_buffer`, `me.selection_stack` and `me.focused_graph`. Live ownership is
  `show.live_owner() == Some(client.session())`. `get_selection()`, `curr_scene()` and `clone_for_ui()` still mean
  `DESK`, unchanged.

### Decisions and deviations
- **`send(impl Into<BusBody>)`** instead of the ticket's `send(BusBody)`. It accepts a superset of what the ticket
  asked for, and it keeps every existing call site's shape.
- **Streams.** Logs, startup stages, tick speeds, heartbeats, audio devices, alerts and desk requests are the
  system-message stream (`try_recv_system`), not accessors that keep state. The frontend folds them into its own view
  state, as `handle_events` already does (log window, startup popup, `tick_speeds`). The `RwLock`-backed streams get
  closure readers.
- **Reply closures live in the frontend** (`ReplyHandlers<Ctx>` on `BlaulichtApp`), not in the client, because they
  need `&mut` access to the frontend's state. The `fn` pointer in `dispatch` finds the registry inside `Ctx`, so a
  closure can borrow all of `Ctx`.
- **The client holds the hub and bus handles** (as the ticket asks) to attach more clients (`open_client`). Nothing
  else uses them yet. 052 may use the hub for lifecycle.
- **Small engine-side additions** outside the Touches list: `From` impls in `hub.rs`, and `resolve_selection` /
  `SessionView::fixture_selection` in `dmx/state.rs` and `dmx/session.rs`.
- **Missing readers** (not listed in the ticket, still read through `self.data.state`): grand master, audio device
  name, `mainloop_state`, `plugin_runtime_kinds`, `wasm_animation_ui_ops` / `wasm_animation_editor_requests`, plugin
  state storage and config. Add a reader when the page that needs one migrates (045, 051, 052).

### Two-client test harness (`client::testing`)
- `TestEngine::start(seed: impl FnOnce(&mut EngineState))` runs a real `DmxEngine`, a real bus (on its thread) and a
  real hub, with no UI and no audio thread. Methods: `connect_desk()`, `connect_remote()` (ticks once, so the session
  is open), `tick()` (a full engine tick with silent audio: bus, scene graph, animations, render),
  `replies(&client, n) -> Vec<CommandResult>`, `round_trip(&client, body) -> CommandOutcome`, `edit_show(f)` (an
  engine-side write for setup) and `app()`.
- `TwoClients::start(seed) -> TwoClients { engine, desk, remote }`. The fields are separate, so
  `h.engine.round_trip(&h.remote, ..)` borrows cleanly.
- `small_rig`: groups 0, 1 and 2 with two RGB fixtures each, scenes 0 ("Main"), 1 ("A") and 2 ("B"), plus `BLANK`.

### Verification
- `cargo test --workspace --all-features`: all pass. blaulicht-core went from 285 to 293 tests. The new ones are in
  `client::tests`:
  - `two_sessions_keep_two_selections`: the acceptance test. The desk selects group 0 and the remote selects group 1
    limited to fixture 1. Each sees only its own selection through `with_session`. The desk's selection is in the
    shared fields, and the remote's `SetAlpha` writes only its own fixture.
  - `a_session_view_follows_its_own_focus`.
  - `results_reach_only_their_sender_in_order`: Ping, a refusal, and a `CreatePalette` with its created id.
  - `closing_a_client_closes_its_session`.
  - `results_and_system_messages_are_sorted_without_loss`: both read orders.
  - `reply_handlers_run_on_their_own_result`: a dialog model that closes on success and shows a refusal; each
    closure runs once; a command nobody waits for is ignored.
  - `stream_readers_see_the_engine_state`: a channel override shows in `with_dmx_universe` after a tick.
  - `published_view_state_reaches_the_mirror`.
- `cargo clippy --workspace --all-targets --all-features`: blaulicht-core lib 141 and lib test 160, unchanged. The
  only touched warning is the existing `too_many_arguments` on `AnimationEditState::show`, which went from 9 to 10
  arguments and is still one warning. `cargo build -p blaulicht-core --all-features`: 84 lib warnings, unchanged.
  None of the warnings is in new code.
- `cargo fmt`: clean for the touched files. The usual unrelated hunks (`audio_features`, `pages/audio.rs`,
  `visualizer/ui.rs`, `app/ui.rs`) were reverted.
- **Frame times.** Setup:
  - blenv, debug build (unoptimized, llvmpipe), 800x480 kiosk, a fresh copy of `Kuze_Theater.json` per run.
  - Programmer with Par Truss selected (`SelectGroup(6)`), then View, then Setup › Fixtures, about 45 s each.
  - A temporary probe timed `App::ui` (UI build and event handling, not GL paint) in 300-frame windows. It was the
    same patch in every run and was removed afterwards (not committed).
  - Runs were interleaved: base, change, base again (rebuilt from `a376972`), change again.

  Mean of the window means, and the median of the window p50s and p95s, in µs:

  | Page | before 1 | after 1 | before 2 | after 2 |
  |---|---|---|---|---|
  | Programmer, mean / p50 / p95 | 2984 / 2944 / 3243 | 2996 / 2944 / 3246 | 2988 / 2942 / 3265 | 2960 / 2908 / 3209 |
  | View, mean / p50 / p95 | 335 / 326 / 405 | 339 / 331 / 412 | 337 / 328 / 410 | 340 / 333 / 414 |
  | Setup › Fixtures, mean / p50 / p95 | 568 / 554 / 688 | 574 / 557 / 692 | 575 / 558 / 700 | 573 / 557 / 685 |

  - Every difference is within the spread between the two "before" runs, so the change is no slower.
  - The View page drifts by about 40 µs during each 90 s run, in every run. Its 4–5 µs offset is smaller than that
    drift. The only new per-frame work is one uncontended mutex lock per system message plus one `take_results`, well
    under a microsecond.
  - `Kuze_Theater.json` has no views, so the View page draws little.
  - Frame interval (mean): Programmer about 12.8 ms, View about 5.0 ms and Setup about 6.3 ms, the same before and
    after.
- **blenv smoke test** (kiosk, a copy of `Kuze_Theater.json`, screenshots read):
  - Logs page: the replayed and live lines show (system stream through the client), and the startup popup closed.
  - Clicking the navbar's Programmer icon opened Programmer (`NavigatePage` sent through the client).
  - Clicking "Par Truss" selected it: the inspector shows `group_ids: {6}` in the `DESK` session.
  - Dragging Alpha set it to 167 and the unsaved-changes dot appeared.
  - View and Setup › Fixtures render as before.
  - Sample egui plugin window (`render_plugin_ops` now takes the client): two "Click Me" clicks gave "Clicks: 2",
    and dragging the slider set Intensity to 225.
  - LIVE on: the button is highlighted, the red dot shows, and the inspector reports `live_mode: true`. LIVE off
    clears both.
  - No unexpected lines in the app log (only the usual missing DMX/MIDI devices and the thread-priority warning).
  - Not checked live: a second session in the running app (no in-tree frontend opens one). The harness tests cover
    it.

### Guidance for 050, 043, 044, 051, 045 and 052
- Import the trait (`use crate::client::EngineClient;`) and call `self.client.<method>`. Use only trait methods in
  pages; `LocalClient`'s inherent methods are constructors. That way a remote client later changes only the field's
  type.
- **Read state** in one scoped closure per widget block, and copy out what the block needs:
  `let (groups, focus) = self.client.with_show(|show| (show.0.groups.clone(), show.0.current_scene_focus));`.
  Or draw inside the closure. Edition 2021 closures capture disjoint fields, so
  `self.client.with_show(|show| self.programmer_ui_state.draw(ui, show))` compiles. Calling a `&mut self` method
  inside the closure does not. Never nest `with_*` calls, and never send from inside one: queue the event in a local
  and send it after the closure returns.
- **Read the session:** `self.client.with_session(|show, me| ..)` (always `Some` for the desk). This replaces `.0.selection`, `get_selection()`
  (`me.fixture_selection(&show.0.groups)`) and `curr_scene()` (`show.get_scene(me.scene_focus)`), so the page is
  correct for any session.
- **Send:** `self.client.send_event(ControlEvent::X)` for a `ControlEvent`, or `self.client.send(HostCommand::Y)`
  for a new host command. The existing `self.client.send(ControlEventMessage::new(EventOriginator::Web, ..))` call
  sites can be shortened to `send_event` while migrating.
- **React to a result** (dialogs; `CreateScene` is a hypothetical host command):
  ```rust
  self.reply_handlers.send_then(&self.client, HostCommand::CreateScene { .. }, |app, outcome| match outcome {
      CommandOutcome::Ok { created } => { app.new_scene_dialog_open = false; /* use `created` */ }
      CommandOutcome::Refused(why) => app.scene_dialog_error = Some(why.clone()),
  });
  ```
  The closure runs in `handle_events` on a later frame, once the engine has applied the command. Keep the dialog
  open (maybe disabled) until then, and capture only owned values (`'static`). Add a `CreatedId` variant when the
  engine allocates a new kind of id.
- **Live streams:** use `with_dmx_universe`, `with_health`, `with_spectrogram`, `with_plugin_ui_ops`, `with_audio_params`,
  `with_artnet_receivers` and `with_plugins` instead of `self.data.state.<lock>`.
- **Tests:** `let mut h = TwoClients::start(small_rig);`, then
  `h.engine.round_trip(&h.desk, ..)` / `h.engine.round_trip(&h.remote, ..)`. Assert isolation with
  `h.desk.with_session(..)` / `h.remote.with_session(..)`.
- **Helpers to clean up** as their bodies move: `AnimationEditState::show(.., data, client, ..)` (drop `data`) and
  `plugin_ui_title(&AppStateWrapper, ..)` (use `with_plugins`).
- **050:** `ToggleLiveMode`, like every event that doesn't require a selection, resets the sender's control buffer
  (`DmxEngine::apply`). This is existing behaviour, so the Programmer's faders read 0 after LIVE is toggled. I only
  noticed it during the smoke test and did not change it.

### Follow-ups
- A reply that never arrives (for example, the message was dropped by the shared bus inbox when it was full) leaves
  its closure in `ReplyHandlers` forever. Add a timeout or cap if that ever matters.
- 053/047: decide between a generic `BlaulichtApp<C: EngineClient>` and an enum client. The trait is deliberately
  not object-safe.
