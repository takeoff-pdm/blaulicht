# 047 — Websocket transport and RemoteClient

| | |
|---|---|
| Epic | E10 Decouple |
| Depends on | 053 |
| ABI / showfile | none; `port` from `config.toml` becomes the listen port |

**Touches:** new `crates/engine/src/server/`, `crates/engine/src/client.rs` (`RemoteClient`), `crates/shared/src/` (wire types), `crates/core/src/routes/` (delete)

## Problem
Remote frontends need a way to attach. `routes/` is 539 lines of commented-out actix code, and `port = 1234`
(`config.rs:34`, `config.toml:1`) is read nowhere. There is no async runtime in the tree.

## Scope
- A websocket server on the engine side: `tungstenite` (sync) with one thread per client, matching the
  crossbeam design. It is on by default in both binaries and can be turned off in config.
- Protocol:
  - **Handshake:** `PROTOCOL_VERSION` (separate from the plugin ABI, but bumped whenever `ControlEvent`,
    `HostCommand` or a stream type changes). The client gets a `ClientId` and a new `SessionId`.
  - **Show state:** reuse the bincode bytes and the FNV digest that the plugin tick already produces
    (`plugin/tick.rs:227–244`). Send them when the digest changes, at most 10 Hz. Note the 1 MiB limit
    (`abi.rs:222`) as a known ceiling, and per-section deltas as a follow-up (OUTDOOR's engine section is about
    1.3 MB as JSON).
  - **Session snapshot:** sent after every change in that session.
  - **Commands:** `BusBody` from the client, wrapped by the server into a `BusMessage` with that client's ids.
    `CommandResult`s come back to that client.
  - **Streams the client subscribes to, throttled:** DMX buffers, spectrogram, health, tick speeds, logs and
    startup stages (with replay), and plugin UI ops.
    - `WasmUiOp` moves to shared and derives `Serialize` / `Encode` (`ui_ops.rs:1`).
    - Plugin UI interaction already flows as `ControlEvent::PluginUi` / `AnimationPluginUi`
      (`plugin_ui.rs:303`), so a remote client sends the same events.
  - **Encoding:** bincode, and JSON as well (serde already derives it). JSON is needed if the web frontend
    isn't Rust.
- `RemoteClient` implements `EngineClient` over this protocol.
- When a client disconnects, its session and its desk-frontend entries are dropped (040, 039).
- Authentication: none for now (local network). Add it to the `TODO.md` Inbox.

## Out of scope
- Latency hiding, the web UI itself (048, 054).

## Acceptance criteria
- [ ] Integration test over loopback: a client connects, gets the snapshot, sends `SelectGroup` + `SetColor` in
      its own session, the DMX output changes and the desk session's selection does not; disconnecting cleans up
      the session.
- [ ] Logs reach both the native UI and a remote client.
- [ ] A stalled client doesn't block the engine or grow its memory without bound.
- [ ] With a remote client connected, the native UI's frame time is unchanged; snapshot size and rate with a large
      showfile are recorded in Notes.

## Verification
- `cargo clippy --workspace --all-targets --all-features`, `cargo fmt`, `cargo test --workspace --all-features`.

## Notes
