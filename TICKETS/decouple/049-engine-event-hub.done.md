# 049 — Engine event hub: client ids, envelopes, replies and broadcast

| | |
|---|---|
| Epic | E10 Decouple |
| Depends on | none |
| ABI / showfile | none (the envelope is host-only; plugins keep receiving `ControlEventMessage`) |

**Touches:** `crates/core/src/event/mod.rs`, `crates/core/src/msg.rs`, `crates/core/src/main.rs`, `crates/core/src/state.rs`, `crates/core/src/dmx/mod.rs`, `crates/core/src/plugin/tick.rs`, `crates/core/src/plugin/wasm.rs`, `crates/core/src/mainloop/supervisor.rs`, `crates/core/src/app/event.rs`, `crates/core/src/app/page.rs`, `crates/core/src/app/jog.rs`, new `crates/core/src/hub.rs`

## Problem
Today, more than one frontend can't attach:
- **The event bus is fixed at startup.** `SystemEventBus::new_connection(&mut self)` only works before the bus
  moves into its thread (`event/mod.rs:86`, `main.rs:95–103`), and every per-connection channel is unbounded
  (`:87`).
- **Messages don't say who sent them.** `ControlEventMessage` carries only `originator: Web | DmxEngine | Plugin`
  (`shared/src/event.rs:18`, `:60`). The engine's errors and rollbacks are logged with `debug!` and
  re-broadcast as `DmxEngine` (`dmx/mod.rs:389–397`). Nobody can tell the sender that its command failed or
  which id it created.
- **System messages have a single consumer.** `system_message_receiver` comes from one `unbounded()` channel
  (`main.rs:34`), and `AppStateWrapper` is `Clone`. A second frontend would steal about half of the Log,
  StartupStage and Heartbeat messages. Headless, nobody drains it. The supervisor stops the engine when every
  receiver is dropped (`supervisor.rs:87–95`).

## Scope
- **Envelope on the bus:** change the bus message type (the bus is generic,
  `SystemEventBusConnection<T>`, `event/mod.rs:12–16`) to a host-only
  `BusMessage { client: ClientId, session: SessionId, seq: u64, body: BusBody }` with
  `BusBody::Control(ControlEventMessage) | BusBody::Host(HostCommand)`. `HostCommand` starts empty (or with one
  variant used by a test) and lives in core, not shared. `SessionId` is defined here with `DESK`;
  040 adds the store.
- **Plugins** keep receiving only `ControlEventMessage`s, built from `BusBody::Control` when `TickInput` is
  assembled (`tick.rs:286–297`). Plugin-sent events get the plugin's `ClientId` and `SessionId::DESK`
  (`wasm.rs:875`, `:966`).
- **Runtime connections:** an `EventBusHandle` that can add and remove connections while the bus runs. Use a
  bounded channel per connection. When a client's queue is full, drop its oldest messages and log it once;
  never block the engine.
- **Replies:** after applying a message, the engine publishes `CommandResult { client, seq, outcome:
  Ok { created: Option<CreatedId> } | Refused(String) }` to that client only. Rollbacks carry the original
  client and session instead of being re-sent as an anonymous `DmxEngine` event.
- **System-message hub:** replace the single-consumer channel with a broadcast hub in which every subscriber
  has its own bounded queue. Late subscribers get replayable state: current startup stages and the last N log
  lines (move the log ring buffer here, `AppState.logs` is legacy). The supervisor's "shut down when nobody
  listens" check moves to an explicit shutdown signal.
- **Native UI:** gets a `ClientId` and subscribes to the hub. Its existing sends (`page.rs:104`, `jog.rs:67`,
  `view_perf.rs:75`, …) go through one helper that builds the envelope. `handle_events`
  (`app/event.rs:25–100`) reads its own subscription.
- **Tests:** two subscribers both receive every log line; a full subscriber doesn't block the engine; a
  `CommandResult` reaches only its sender; a plugin's `TickInput` is byte-for-byte unchanged in shape.

## Out of scope
- Session store and session-scoped resolution (040).
- Moving `SavePluginState` and other UI-side handlers into the engine (052).

## Key code
- Bus — `crates/core/src/event/mod.rs:12–130`
- Engine dispatch and rollback — `crates/core/src/dmx/mod.rs:380–397`, `apply` `:730`
- System channel — `crates/core/src/main.rs:34`, drained at `crates/core/src/app/event.rs:100`
- Supervisor heartbeat — `crates/core/src/mainloop/supervisor.rs:85–95`

## Acceptance criteria
- [x] Two in-process subscribers both receive logs and startup stages; one late subscriber gets the replay.
- [x] A command's `CommandResult` arrives at its sender, including `Refused` for an engine refusal.
- [x] The app behaves exactly as before (blenv smoke test: start, navigate, select a group, set a color).
- [x] Plugins load without being rebuilt (`PLUGIN_ABI_VERSION` unchanged).

## Verification
- `cargo clippy --workspace --all-targets --all-features`, `cargo fmt`, `cargo test --workspace --all-features`
  (blenv container).
- blenv: run, check the Logs page fills and the startup popup completes, take screenshots.

## Notes

### What changed
- **`crates/core/src/hub.rs` (new).** All types later tickets build on:
  - `enum ClientId { Desk, Plugin(u8), Remote(u32) }` (`Copy`, `Ord`, `Hash`, `Display`). It replaces 039's
    `FrontendId`; write `ClientId::Desk` (there is no `DESK` const).
  - `struct SessionId(pub u32)` with `SessionId::DESK = SessionId(0)`. No store yet (040).
  - `type CommandSeq = u64`. Each client numbers its messages from 1. Plugins count per plugin.
  - `enum HostCommand { Ping }`. `Ping` does nothing and the engine answers `Ok`, so a client can check its
    command path. The tests use it.
  - `enum BusBody { Control(ControlEventMessage), Host(HostCommand) }`.
  - `struct BusMessage { client: ClientId, session: SessionId, seq: CommandSeq, body: BusBody }`, with
    `BusMessage::control(..)`, `plugin_event()` and `into_plugin_event()`. The free function
    `plugin_events(iter) -> Vec<ControlEventMessage>` is what `TickInput` is built from.
  - `enum CreatedId { Palette(u8) }`, `enum CommandOutcome { Ok { created: Option<CreatedId> }, Refused(String) }`
    and `struct CommandResult { client, seq, outcome }`. A reply arrives as `SystemMessage::CommandResult(..)`
    on the sender's hub subscription.
  - `SystemHub` (cheap `Clone`) has `publish(msg)` (broadcast, recorded for replay), `publish_to(client, msg) -> bool`
    (only that client's subscribers, not replayed), `subscribe(client) -> HubSubscription`, `is_subscribed`,
    `allocate_client() -> ClientId::Remote(n)`, `shutdown()` / `is_shut_down()` and
    `with_capacity(subscriber_capacity, log_replay_len)`. Defaults: `SUBSCRIBER_CAPACITY = 65536`,
    `LOG_REPLAY_LEN = 1000`.
  - `HubSubscription` has `try_recv`, `recv_timeout`, `len` and `client`. Dropping it unsubscribes.
  - `ClientConnection::connect(id, session, &hub, &bus_handle, &mirror)` bundles a bus connection, a hub
    subscription and a `FrontendUiPublisher` under one `ClientId`. It has `send(ControlEventMessage) -> CommandSeq`,
    `send_host(HostCommand)`, `send_body(BusBody)`, `try_recv_bus()`, `try_recv_system()` and `publish_ui(snapshot)`.
    Dropping it disconnects the client everywhere: bus, hub and view-state mirror. This is how 039's
    "frontend connect and disconnect follow client lifetime" is met.
- **Bus (`event/mod.rs`).** `EventBusHandle<T>` (`connect()`, `connection_count()`) comes from
  `SystemEventBus::handle()` and adds connections while the bus runs. A connection is removed when its last clone
  is dropped (a shared membership guard). Each connection keeps at most `CONNECTION_CAPACITY = 16384` messages.
  When the queue is full, the bus drops the oldest message and logs a warning once per connection. It never
  blocks. `SystemEventBusConnectionInst` is now `SystemEventBusConnection<BusMessage>`.
- **Engine (`dmx/mod.rs`).** `DmxEngine::new(state, bus, hub, devices)`. `dispatch(&mut state, BusMessage) ->
  Option<CommandResult>` wraps the unchanged `apply(ControlEventMessage)`. `apply_host` handles `HostCommand`s.
  `apply_bus_messages()` (split out of `tick_internal`) publishes the replies after releasing the engine write
  lock. The engine's own `DmxEngine`-originated messages get no reply. The `CreatePalette` arm records the new
  id in a `Cell<Option<CreatedId>>`.
  - **Rollbacks** (and the undo event that a successful folder event returns) go back on the bus as
    `BusMessage { client, session, seq }` of the original message, with
    `ControlEventMessage::new(DmxEngine, rollback)` as the body. Plugins see exactly what they saw before.
- **Plugins.** `TickInput.events` is built with `hub::plugin_events` (`tick.rs`): host commands are dropped and
  the envelope is stripped. Plugin sends get `ClientId::Plugin(id)` and `SessionId::DESK`. `bl_send_event` has no
  plugin id parameter, so each plugin's wasmtime store now carries `PluginStoreData { plugin_id, next_seq }`
  (`Store<PluginStoreData>`, `Linker<PluginStoreData>`). This is host-only; the guest sees nothing.
  `ui_create_external_screen` and `ui_remove_external_screen` take the store data the same way: they gained a
  leading `Caller` parameter, which is not part of the wasm signature.
- **System messages.** `Sender<SystemMessage>` is replaced by `SystemHub` everywhere: the tracing layer
  (`log.rs`), supervisor, mainloop, init guard, plugin manager, MIDI manager and host functions. The
  `.expect()` / `.unwrap()` calls on system sends are gone; the mainloop's `TickSpeeds` send used to panic the
  engine thread once the UI receiver was dropped. `AppState.logs` and `AppState::log` are removed (they had no
  callers). `DeskRequest` and `SavePluginState` go to `ClientId::Desk` only (`publish_to`). Everything else is
  broadcast. A global `SavePluginState` with no desk connected logs a warning.
  - **Replay** for a late subscriber: the latest status per startup stage, the `EngineInitializationComplete`
    flag, the latest `AudioDevicesView` and `AudioSelected`, then the last 1000 `Log` / `WasmLog` lines. The
    replay and the registration happen together under the hub lock, so no message is lost or duplicated.
  - **Shutdown:** the supervisor stops when `hub.is_shut_down()` is true. `main` calls `hub.shutdown()` after
    `eframe::run_native` returns, which happens at about the same moment the old "receiver dropped" check fired.
- **Desk.** `AppStateWrapper { from_frontend_sender, config, config_path, client: ClientConnection, state }`. It is
  no longer `Clone` (nothing cloned it), and `system_message_receiver`, `system_message_sender` and
  `event_bus_connection` are gone. `BlaulichtApp.desk_ui` is gone; the publisher lives in the client. All of the
  desk's sends are `self.data.client.send(ControlEventMessage::new(EventOriginator::Web, ..))`, so one helper builds
  the envelope (a mechanical rename; `cargo fmt` reflowed the touched call sites). `handle_events` reads
  `try_recv_bus()` and ignores `BusBody::Host`, and reads `try_recv_system()` and ignores `CommandResult` for now.
  The desk connects in `main` after `init_tracing` and before the supervisor starts (as in 039). Lines logged
  before that ("Starting BLAULICHT …") and the Config stage reach it through the replay.
- **`frontend_ui.rs`.** It is keyed by `ClientId`. `send_desk_request(&hub, request) -> bool` and
  `route_plugin_event_to_desk(&hub, &event)` publish to the desk client's subscription. "Desk connected", for
  routing, means the desk has a hub subscription.

### Deviations
- **Bounded queues:** each queue is an *unbounded* crossbeam channel with a cap enforced on push: when
  `len >= cap`, the oldest message is dropped first. This has the same behaviour the ticket asks for (bounded,
  never blocks, drops oldest, logged once). `crossbeam::bounded(n)` allocates all n slots up front, so the cap
  had to be small. A desk window that stops repainting (hidden on Wayland) still gets about 12 system messages per
  second (tick speeds, heartbeats, audio devices), and 4096 slots would fill in about 6 minutes, after which
  queued `DeskRequest` and `SavePluginState` messages would start to drop. With caps of 65536 (hub) and 16384
  (bus), memory is only used while a client actually lags.
- `ClientId` is an enum rather than a number, and has no `Engine` variant: rollbacks carry the original sender,
  and nothing else in the engine sends on the bus.
- `HostCommand` starts with `Ping`, not empty. `CreatedId` has only `Palette`, because `CreatePalette` is the
  only event for which the engine allocates the id today.
- When the hub overflows, the warning goes only to the terminal (`TERMINAL_ONLY_LOG_TARGET`): the Logs page
  publishes into the hub, so logging there could feed back into it. Overflow warnings are emitted after the hub
  lock is released.

### Verification
- `cargo test --workspace --all-features`: all pass. blaulicht-core went from 251 to 263 tests.
  - Bus (2): connections are added and removed while the bus runs; a full connection drops its oldest messages
    without blocking the others.
  - Hub (8):
    - two subscribers both get every log line and stage;
    - a late subscriber gets the replay (latest stage, init flag, last N logs, no heartbeats), then live messages;
    - a full subscriber doesn't block the publisher and keeps the newest messages;
    - targeted messages reach only their client and are not replayed;
    - dropping a subscription unsubscribes;
    - the replay carries each stage's latest status;
    - `ClientConnection` stamps the envelope and disconnects everywhere on drop;
    - `plugin_tick_input_is_unchanged_by_the_envelope`: encoding `TickInput` from envelopes gives the same bytes
      as building it from `ControlEventMessage`s directly. The events' bytes are pinned as
      `[3, 0,0,3, 2,9,200, 1,1,9]`.
  - Engine (1, `dmx::command_result_tests`, real `DmxEngine` and bus):
    - a remote's `SelectGroup(9)` is answered `Refused("Illegal group")` to the remote only;
    - the desk sees the rollback with the remote's client, session and seq;
    - `CreatePalette` returns `created: Some(Palette(0))`, and `Ping` returns `Ok`;
    - rollbacks get no reply.
  - Plugin envelope (1).
  - The three routing tests in `frontend_ui` now run against the hub.
- `cargo clippy --workspace --all-targets --all-features`, measured on the previous commit and on this one in
  the same container: blaulicht-core lib 142 → 141, lib test 161 → 160. `cargo build`: 84 → 84 lib warnings. None
  is in new code.
- `cargo fmt`: clean for the touched files. The pre-existing diffs in `audio_features`, `pages/audio.rs`,
  `visualizer/ui.rs` and one hunk in `app/ui.rs` were reverted, as in 039.
- blenv (kiosk, screenshots read), with plugin bundles built before this change (13:36 UTC, ABI 18, not
  rebuilt). All five plugins loaded.
  - **Startup popup:** while plugins load it showed Config ✓ (published before the desk subscribed, so it came
    through the replay), Showfile ✓, DMX engine ✓, Plugins 4/5. Once startup completed, the popup closed.
  - **Logs page:** fills; scrolled to the top, the first lines are "Starting BLAULICHT 0.30.0" and "Using default
    audio device", which were also replayed.
  - **Smoke test** with a copy of `Kuze_Theater.json`:
    - `nav Programmer` (a plugin-sent `NavigatePage`) works;
    - clicking "Frontlicht", then "Par Truss", selects the group (inspector: `selection.group_ids = {6}`);
    - dragging Alpha, Hue, Saturation and Value set 206/198/216/216, the swatch turned cyan, and the
      unsaved-changes dot appeared (the desk's own echoed events still mark the show dirty).
  - **Desk requests and plugin input:** `blctl event SetPluginUIOpen{plugin_id:4}` opened the sample egui window
    (a `DeskRequest` over the desk's hub subscription). Clicking "Click Me" twice showed "Clicks: 2", so plugins
    receive the desk's events in `TickInput`.
  - **App log:** only the usual environment warnings (no MIDI, no DMX devices, thread priority); no bus or hub
    overflow.

### Follow-ups
- **040:**
  - The envelope already carries `session`. `DmxEngine::dispatch` is where to swap the sender's session in and
    out around `apply`.
  - The hub only allocates client ids; add session allocation next to `allocate_client`.
  - Filter session-scoped events by `BusMessage.session`. In `tick.rs`, do it before `plugin_events` strips the
    envelope. `handle_events` currently discards the envelope (`.map(|m| m.body)`).
  - Remote clients will probably also send `EventOriginator::Web`, which the desk treats like its own echo
    (dirty marking, jog target).
- **041/042:**
  - `EngineClient` can wrap `ClientConnection`: `send_body` already returns `CommandSeq`, and `take_results` means
    filtering `SystemMessage::CommandResult` out of `try_recv_system()`.
  - The `SystemHub` and `EventBusHandle` exist only as locals in `main` (the hub is also cloned into the
    engine). `LocalClient` and the two-client test harness need both; pass them in or keep them on the wrapper.
    `dmx::command_result_tests` shows how to wire an engine with two clients.
- **034:** refusal feedback can use `CommandResult`; the desk ignores replies today.
- **047:** a remote client is `hub.allocate_client()` plus `ClientConnection::connect`.
  `HubSubscription::recv_timeout` is there for a transport thread.
- **052:** `SavePluginState` for global state is still written by the desk. Without a desk it is lost (with a
  warning).
- Plugins can't receive `CommandResult`s (that would be an ABI change); replies to `ClientId::Plugin(_)` find no
  subscriber and are dropped.
- As in 039, connecting the same `ClientId` twice is unsupported for the view-state mirror: the first
  connection's drop clears both.
- Pre-existing and unchanged: the shared bus inbox (128) still drops a message, with a debug log, when it is
  full.
