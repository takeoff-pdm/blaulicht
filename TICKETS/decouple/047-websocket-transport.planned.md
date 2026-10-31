# 047 — Websocket transport and RemoteClient

| | |
|---|---|
| Epic | E10 Decouple |
| Depends on | 046 |
| ABI / showfile | none; config gets a listen port (`port` already exists in `config.toml`) |

**Touches:** `crates/engine/src/server/` (new), `crates/core/src/routes/` (old actix code, delete or replace), `crates/shared/src/` (wire types)

## Problem
Remote frontends need a way to attach to the engine. The old actix/websocket code in `routes/` is commented out
and predates the current state model.

## Scope
- A websocket server in the engine, enabled in both binaries (it can be turned off in config):
  - **On connect:** the client gets a new `SessionId` and a full show snapshot (`EngineState` is already
    `Serialize` + bincode `Encode`).
  - **After that:** a versioned stream of changes. Starting with "new snapshot after every applied command,
    rate-limited" is acceptable if it is simple and correct; write the decision in Notes.
  - **Commands from the client:** `ControlEvent`s tagged with the client's session. Bincode on the wire, JSON
    optional for debugging.
  - **Live streams the client subscribes to, throttled:** DMX buffers, spectrogram, tick speeds, logs, plugin UI
    ops.
  - Plugin UI interaction (`WasmUiOp` button, slider and similar events) routed back to the plugin.
- `RemoteClient` implements `EngineClient` on top of this, so the egui pages can run against it. This is
  tested in 048.
- Sessions are dropped when a client disconnects.
- Authentication: none for now (local network only, like today). Add it to the Inbox as a follow-up.

## Out of scope
- Latency hiding or optimistic updates. Correct is enough; only the local frontend has to be fast.
- A web UI (048).

## Key code
- Old routes — `crates/core/src/routes/{ws,api,html}.rs`
- Websocket event bus connection — `crates/core/src/main.rs:~98`

## Acceptance criteria
- [ ] A test client connects, receives the snapshot, sends `SelectGroup` + `SetColor` in its own session, and the
      DMX output changes while the native UI's selection does not.
- [ ] Disconnecting cleans up the session.
- [ ] The native UI's frame time is not affected by a connected remote client.

## Verification
- `cargo clippy --all-targets --all-features`, `cargo fmt`, `cargo test --all-features` (including an
  integration test with an in-process client over loopback).

## Notes
