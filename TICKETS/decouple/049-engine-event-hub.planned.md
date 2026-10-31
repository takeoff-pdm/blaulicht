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
- [ ] Two in-process subscribers both receive logs and startup stages; one late subscriber gets the replay.
- [ ] A command's `CommandResult` arrives at its sender, including `Refused` for an engine refusal.
- [ ] The app behaves exactly as before (blenv smoke test: start, navigate, select a group, set a color).
- [ ] Plugins load without being rebuilt (`PLUGIN_ABI_VERSION` unchanged).

## Verification
- `cargo clippy --workspace --all-targets --all-features`, `cargo fmt`, `cargo test --workspace --all-features`
  (blenv container).
- blenv: run, check the Logs page fills and the startup popup completes, take screenshots.

## Notes
