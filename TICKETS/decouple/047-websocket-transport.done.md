# 047 — Websocket transport and RemoteClient

| | |
|---|---|
| Epic | E10 Decouple |
| Depends on | 053 |
| ABI / showfile | none; `port` from `config.toml` becomes the listen port |

**Touches:** new `crates/engine/src/server/`, `crates/engine/src/client.rs` (`RemoteClient`), `crates/shared/src/` (wire types), `crates/engine/src/routes/` (delete)

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
- [x] Integration test over loopback: a client connects, gets the snapshot, sends `SelectGroup` + `SetColor` in
      its own session, the DMX output changes and the desk session's selection does not; disconnecting cleans up
      the session.
- [x] Logs reach both the native UI and a remote client.
- [x] A stalled client doesn't block the engine or grow its memory without bound.
- [x] With a remote client connected, the native UI's frame time is unchanged; snapshot size and rate with a large
      showfile are recorded in Notes.

## Verification
- `cargo clippy --workspace --all-targets --all-features`, `cargo fmt`, `cargo test --workspace --all-features`.

## Notes

Done in one part (no split).

### What changed
- **`crates/engine/src/server/` (new): the websocket server.** Sync `tungstenite` 0.24, no async runtime.
  - `mod.rs`:
    - One `server` listener thread on `0.0.0.0:<port>` (`config.toml`'s `port`, so 1234 by default).
    - A `remote-<n>` thread per client. Each has a `ClientConnection::connect_remote` (fresh `ClientId::Remote`,
      new session, hub queue capped at `REMOTE_QUEUE_CAPACITY = 4096`, bus connection, view-state publisher)
      and reads `AppState` directly. The server is engine-internal, so `AppState` stays private.
    - `Server::start(ServerOptions, ..) -> ServerHandle` (`local_addr()`, `clients() -> Vec<ClientInfo>`,
      `stop()`; dropping it stops too). `ServerOptions { bind, max_clients: 16, queue_capacity: 4096,
      stall_timeout: 30 s }`.
  - `protocol.rs`: the wire types and `PROTOCOL_VERSION = 1` (summary below).
  - `json.rs`: a serde serializer to `serde_json::Value` that writes composite map keys as their JSON text.
  - `snapshot.rs`: `ShowSnapshots` (on `AppState` as `show_snapshots`) plus `fnv1a`, which moved here from
    `plugin/tick.rs`.
  - `tests.rs`: the loopback tests.
- **`Engine::start`** starts the server after the plugin watcher unless the new `config.toml` key
  `server_enabled = false` is set (`#[serde(default = true)]`; the existing configs keep working). Both the desk
  and `blaulicht-headless` get it. If the port is taken, the engine logs an error and runs without the server.
  `EngineHandle::server_addr() -> Option<SocketAddr>`. `EngineHandle::shutdown` stops the server first, so no
  remote command arrives during the final save. I used a start hook inside `Engine::start` instead of an
  `EngineHandle` method that hands out the hub and bus.
- **`client::RemoteClient`** (`crates/engine/src/client/remote.rs`) implements `EngineClient` over the protocol
  (bincode only). API below.
- **Hub (`hub.rs`):**
  - `SystemHub::subscribe_with_capacity(client, capacity)`: subscribers now carry their own cap. The default
    `subscribe` is unchanged.
  - `ClientConnection::connect_remote(hub, bus, mirror, queue_capacity)`.
  - `ClientConnection::send_with_seq(seq, body)`: a remote client numbers its own commands.
  - `ClientConnection::queued_system()`.
  - `HostCommand::is_desk_only()`, which is true for `SetFrontendSection`.
- **Engine (`dmx/mod.rs`):** `dispatch` refuses a desk-only host command from any client other than
  `ClientId::Desk` with `Refused(DESK_ONLY_REFUSAL)` ("Only the desk frontend can send this command"). This is
  checked before the command is forwarded to persistence, so it covers local and remote clients alike. I found no
  other desk-only host commands. Quit/Restart and the showfile commands stay allowed for remote clients: 052
  already closes the desk window on any client's `Exiting`, and a headless engine needs them. Security is in the
  Inbox.
- **A remote client's copy of the engine state (`dmx/session.rs`, `dmx/stage_commands.rs`):**
  - `EngineState::set_mirrored_show(shared)`, `set_mirrored_session(id, OperatorSession, live_owner)` and
    `set_mirrored_stage(scene, revision, showfile)`.
  - `SessionView::to_session() -> OperatorSession`.
  - With these, `RemoteClient::with_show` hands out a real `EngineState`, and `session()`, `with_session`,
    `clone_for_session`, `stage()`, `showfile()` and `live_owner()` work unchanged.
- **Plugin tick (`plugin/tick.rs`):** after ticking the plugins, `app.show_snapshots.offer(digest, bytes)`
  *moves* the engine-state bytes it already built for plugins into the slot. It does nothing unless a remote
  client is connected (one atomic load).
- **Serde derives for the wire:**
  - `ClientId`, `SessionId`, `HostCommand` and its parameter types (`AudioParam`, `SceneGraphNodeParam`,
    `SceneGraphEdgeParam`, `StagePatch`, `FixturePlacement`), `BusBody`, `BusMessage`, `CreatedId`,
    `CommandOutcome`, `CommandResult`, `OperatorSession`.
  - `TickSpeeds`, `DmxTickSpeeds`, `StartupStage`, `StageState`, `StageStatus`, `WasmLogBody` (gained
    `Deserialize`), `ShowfileStatus`, `SaveStatus`.
  - The health types (`AppHealthState`, which gained `Default`, plus `DmxHealth*`, `MidiHealth`,
    `MidiDeviceState`, `MidiError`, `SerialHealth`, `SerialDeviceState`, `SerialError`), `ArtNetReceiver`,
    `PluginState`/`PluginFlags`, `PluginRuntimeKind` and `AudioThreadControlSignal`.
  - In audio_engine: `AudioBucket` (gained `Deserialize`) and `SignalCollectorParams`. In shared: `LogLevel`
    gained `Deserialize`.
  - Fields with a special wire form:
    - **`ShowfileStatus.saved_at`** goes out as UNIX milliseconds, `null` if never (`serde_as::unix_millis`).
    - **`SetFrontendSection.section`** goes out as JSON text (`serde_as::json_string`), because
      `ShowfileUiState` uses `skip_serializing_if` and `untagged`, which bincode can't decode.
    - **`ModelBytes`** goes out as a byte string (`serde_as::shared_bytes`).
  - None of these types is in the plugin ABI.
- **`WasmUiOp`** moved to `blaulicht-shared` (`blaulicht_shared::ui_ops`) and derives `Serialize`, `Deserialize`,
  `Encode`, `Decode` and `PartialEq`. `blaulicht_engine::ui_ops` re-exports it. Plugins never see this type (one
  host function per op), so the ABI bytes are unchanged and no separate wire mirror was needed.
  `PLUGIN_ABI_VERSION` stays 18. `ControlEvent` and `TickInput` are unchanged.
- **Deleted `crates/engine/src/routes/`** (dead actix code). The `blaulicht_engine::server` log target now maps to
  the "Web" source (it used to be `routes`).
- **`crates/engine/examples/remote_probe.rs`:** connects a `RemoteClient` to all streams and prints frames and KB
  per message kind every second (`cargo run -p blaulicht-engine --example remote_probe -- 127.0.0.1:1234 60`).
- **Docs:** `crates/core/AGENTS.md` (the server module and `RemoteClient`). Two new `TODO.md` Inbox entries: no
  authentication, and full show snapshots at 10 Hz.

### Protocol (for 048)
- **Transport:** a websocket at `ws://<engine>:<port>/`; any path is accepted.
- **Handshake:**
  1. The client's first message is `ClientMessage::Hello { protocol_version, streams }`.
  2. **The frame type picks the encoding** for the whole connection, both ways: a binary frame means bincode, a
     text frame means JSON.
  3. The server answers `ServerMessage::Welcome { protocol_version, client: ClientId, session: SessionId,
     engine_version }`. Or it answers `Rejected { protocol_version, reason }` and closes. It rejects another
     version ("Protocol version N is not supported (the engine speaks 1)"), a first message other than `Hello`,
     an undecodable `Hello`, and the 17th concurrent client ("Too many clients (at most 16)").
  4. Each connection is a new `ClientId::Remote(n)` with a new operator session. A reconnect gets new ids.
- **Encodings:**
  - **bincode:** bincode 2 in serde mode, `config::standard()`.
  - **JSON:** serde's externally tagged form. Examples: `{"Command":{"seq":7,"body":{"Control":{"originator":
    "Web","body":{"SelectGroup":1}}}}}`, `{"Result":{"client":{"Remote":0},"seq":7,"outcome":{"Ok":{"created":
    null}}}}`; unit variants are strings (`"Clean"`). In server messages, composite map keys are their JSON text
    (`"overrides":{"[0,5]":77}`), number keys are strings (`"groups":{"1":…}`), and non-finite floats are
    `null`. `SocketAddr` is `"10.0.0.5:6454"`, `PathBuf` is a string, and `Duration` is `{secs, nanos}`.
- **Client → server (`ClientMessage`):**
  - `Hello`.
  - `Command { seq, body: BusBody }`: the client picks `seq`, and the result carries it back. The server wraps the
    body in a `BusMessage` with the client's ids and stamps every `ControlEventMessage` as
    `EventOriginator::Web`. Without that stamp, the engine would skip a client's `DmxEngine` event as a rollback,
    and the desk would follow a fake `Plugin` `MainUi` request.
  - `Subscribe { streams }` (replaces the set).
  - `PublishUi { open_plugins }`: plugins see these via `ui_is_open`. External screens stay the desk's.
- **Server → client (`ServerMessage`):**
  - `Welcome`, `Rejected`.
  - `Show(ShowSnapshot { digest, state })`: when the digest changed, at most every 100 ms. In bincode, `state` is
    a byte string holding the plugin ABI's bytes (`blaulicht_shared::EngineState::serialize()`; decode with
    `protocol::decode_engine_state`). In JSON it is the `EngineState` object.
  - `Stage(StageSnapshot { revision, showfile, scene })`: when the stage revision or the showfile changed.
  - `Session(SessionSnapshot { session, state: OperatorSession, live_owner })`: checked every 20 ms, and before
    any result is forwarded. **A command's effect on the session arrives before its result**; the show may lag
    by one snapshot interval.
  - `System(WireSystemMessage)`: every `SystemMessage` except `DeskRequest`, `DMX` and the desk's
    `frontend_section`, starting with the hub replay (stages, init flag, audio devices and input, `ShowLoaded
    { generation }`, `ShowfileStatus`, last 1000 log lines).
  - `Result(CommandResult)`.
  - The streams below.
- **Streams (`Stream`):** each is throttled and only sent on change. The show, stage, session and system messages
  are always sent.

  | Stream | Message | Rate |
  |---|---|---|
  | `Dmx` | `Dmx(Vec<DmxUniverse { universe, channels: 513 bytes }>)`, changed universes only | ≤ 30 Hz |
  | `Health` | `Health(AppHealthState)` | ≤ 2 Hz |
  | `Spectrogram` | `Spectrogram(SpectrogramUpdate { reset, max_columns, bin_count, tick_period_ms, columns: new columns only })`; no `debug_data` | ≤ 20 Hz |
  | `PluginUi` | `PluginUi(HashMap<u8, Vec<WasmUiOp>>)` | ≤ 20 Hz |
  | `AnimationEditorUi` | `AnimationEditorUi(HashMap<u64, Vec<WasmUiOp>>)` | ≤ 20 Hz |
  | `Status` | `Status(EngineStatus { grand_master_percent, mainloop_state, audio_params, artnet_receivers, plugins, plugin_runtime_kinds })` | ≤ 2 Hz |
  | `Bus` | `Bus(BusMessage)`: every message on the bus | as it happens |

  Plugin UI interaction is `ControlEvent::PluginUi` / `AnimationPluginUi`, sent as a normal `Command`.
- **Limits:**
  - 64 MiB per message (`MAX_MESSAGE_BYTES`) and 16 clients.
  - The hub queue holds 4096 messages per client and drops the oldest.
  - A client that takes no data for 30 s is disconnected.
  - Closing the connection, from either side or by a stall, closes the session and drops the client's view-state
    entries.
- **Version:** bump `PROTOCOL_VERSION` whenever a type in `protocol.rs` changes, or `ControlEvent`,
  `HostCommand`, `blaulicht_shared::EngineState` or a stream payload.

### RemoteClient API
- `RemoteClient::connect(addr: impl ToSocketAddrs, streams: &[Stream]) -> anyhow::Result<RemoteClient>`. It
  blocks until `Welcome` (5 s connect timeout, 5 s handshake timeout). It fails with "Could not connect to …" or
  "The engine rejected the connection: <reason>".
- `engine_version()`, `is_connected()`, `subscribe(&[Stream])`, `show_digest()`,
  `wait_for_snapshot(timeout) -> bool` (until the first show and session snapshots arrive), and `stats() ->
  RemoteStats { frames, bytes, by_kind, last_show_bytes }`.
- **Before the first snapshots**, the show is empty and `with_session` is `None`, as for a local client before the
  engine opens its session.
- **All `EngineClient` methods work:**
  - `with_show` reads the client's copy under a read lock that only the I/O thread writes.
  - `send` returns the client's own `seq`.
  - `take_results`, `try_recv_system` (system messages turned back into `SystemMessage`) and `try_recv_bus`
    (`Bus` stream) queue at most `SUBSCRIBER_CAPACITY` and drop the oldest.
  - The `with_*` stream readers return defaults until their stream arrives.
  - `grand_master_percent`, `mainloop_state`, `with_audio_params`, `with_artnet_receivers`, `with_plugins` and
    `with_plugin_runtime_kinds` read the `Status` stream.
  - `publish_ui` sends `PublishUi` on change.
- **Disconnect:** every command still waiting gets `CommandOutcome::Refused(DISCONNECTED)` ("Disconnected from
  the engine") in `take_results`, and so does every command sent afterwards. `ReplyHandlers` closures therefore
  always run. There is no timeout for a lost reply on a live connection.
- Dropping the client closes the connection, which closes its session on the engine.
- One I/O thread per client; the socket is non-blocking.

### Decisions and deviations
- **Encoding by the frame type of `Hello`**, with no subprotocol header. Browsers and `wscat` can pick it without
  extra API.
- **JSON can't use `serde_json` directly.** `EngineState` has tuple- and struct-keyed maps (`overrides`, fixture
  states, animations by selection), which `serde_json` refuses. The ticket assumed "serde already derives it".
  `server/json.rs` is a small serializer that keeps every map representable.
  - I first tried a `ciborium` value tree. It failed because ciborium's serializer isn't human-readable, so
    `SocketAddr` and the show state came out in their binary forms. That code was dropped, and ciborium is not a
    dependency.
  - **The Rust client speaks bincode only.** A JSON show state can't be read back into `EngineState` because of
    the key text; `ShowState`'s `Deserialize` refuses JSON with a message. JSON is for non-Rust frontends and is
    tested with a raw client.
- **`ShowState`** has a hand-written serde impl that branches on `is_human_readable()`. bincode carries the
  plugin bytes as they are (no re-encoding). JSON decodes them once per digest, and the encoded frame is cached
  per encoding and shared by all clients.
- **System messages use a wire mirror (`WireSystemMessage`)** instead of derives on `SystemMessage`. It filters
  desk-only messages, drops the desk's `frontend_section`, and survives `SystemMessage::DMX`, whose
  `[u8; 513]` has no serde impl. `HostCommand` derives serde directly, so every command can cross the wire.
- **Show snapshot source:** the ticket said to reuse the plugin tick's bytes and digest. The tick moves them into
  `ShowSnapshots` at most every 100 ms while a client is connected.
  - When the tick hasn't offered for 250 ms (no `wasmtime` feature, main loop stopped or crashed, a snapshot over
    1 MiB that fails `ensure_engine_state_fits_buffer`, or the test engine), the server serializes `.0` itself
    under a read lock.
  - So the 1 MiB `ENGINE_STATE_BUFFER_LEN` ceiling only applies to the plugin path. The server path has no
    ceiling below the 64 MiB message limit.
  - The actual rate is about 8.7 Hz rather than 10 Hz, because the 100 ms offer interval and the 100 ms server
    poll run out of phase.
- **Stage and live owner** are not in the plugin bytes, so they travel as `StageSnapshot` and
  `SessionSnapshot.live_owner`.
- **The remaining `EngineClient` readers** are one `Status` stream (≤ 2 Hz, on change). It includes each plugin's
  whole log buffer (up to 1000 lines), as `with_plugins` does locally.
- **`PublishUi` from remote clients** is coalesced to at most every 250 ms. The mirror's update channel is
  unbounded until a plugin reads it (039's design), so a client toggling plugin windows quickly mustn't flood it.
- **Stalled clients:**
  - The socket is non-blocking. After the first write that the socket doesn't take, the thread stops producing:
    tungstenite's buffer holds at most what was written that round, and its 128 MiB cap is never reached.
  - While backlogged, the show, stage, session and stream updates are skipped. They are "latest only", so the
    client gets the current values once it catches up.
  - System messages wait in the capped hub queue, and bus messages in the bus connection, which is capped at
    16384 by 049.
  - After `stall_timeout` the client is disconnected.
  - No lock is ever held across socket IO, and the engine only pushes into queues that drop their oldest entry.
- **`ReplyHandlers` is unchanged**: the fix sits in `RemoteClient`, which synthesizes refusals on disconnect.
  `LocalClient` can't lose its connection.
- `fnv1a` is no longer `#[cfg(feature = "wasmtime")]`, because the server's fallback uses it.
- Outside Touches: `hub.rs`, `dmx/{mod,session,stage_commands}.rs`, `state.rs`, `msg.rs`, `persistence.rs`,
  `stage/mod.rs`, `audio/defs.rs`, `plugin/{tick,midi,serial}.rs`, `config.rs`, `lifecycle.rs`, `log.rs`,
  `audio_engine/src/collector.rs`, `shared/src/abi.rs`, `crates/core/AGENTS.md` and `TODO.md`.

### Verification
- `cargo test --workspace --all-features`: all pass. blaulicht-engine went from 251 to 261 tests; ui-egui is
  unchanged at 101. The engine suite passed four more runs in a row.
  - `server::tests` (8, real `DmxEngine`, bus and hub via `TestEngine`, the server on `127.0.0.1:0`):
    - `a_remote_client_works_in_its_own_session_and_cleans_up_on_disconnect` (the acceptance test):
      - The `RemoteClient` gets the show (groups 0–2) and its session, and `server.clients()` lists it (bincode,
        its session).
      - The desk selects group 0. The remote sends `SelectGroup(1)` (its session snapshot shows `{1}` by the time
        the result arrives), then `SetColor((255,0,0))` and `SetAlpha(255)`.
      - Group 1's DMX channels become `255,0,0,255,0,0` in the remote's `Dmx` stream and in the desk's universe.
        Group 0 stays at 0, the desk's selection is still `{0}`, and its control buffer is untouched.
      - `publish_ui({3})` makes `is_plugin_open(3)` true.
      - Dropping the client closes its session, removes it from `clients()` and clears plugin 3 from the mirror.
    - `logs_reach_the_desk_and_a_remote_client`: real `tracing` events go through the log layer into the hub
      (`log::with_hub_logging`, test-only). The line logged before the remote connected (replay) and the one
      after reach both the desk and the remote, with level and source.
    - `a_stalled_client_neither_blocks_the_engine_nor_grows_without_bound`:
      - A raw client sends `Hello` with all streams and never reads.
      - 10,000 × 4 KB log lines (40 MB) are published in well under 5 s; publishing never waits.
      - The stalled client becomes `backlogged`. Its `sent_bytes` doesn't change over the next 500 ms, and its hub
        queue stays ≤ 1500 (the test's cap).
      - Meanwhile 20 `Ping` round trips of a healthy remote client each take < 1 s, and 100 engine ticks take
        < 2 s.
      - After the 4 s stall timeout the server drops the stalled client and its session closes. The healthy client
        stays connected.
    - `json_clients_get_json_snapshots_and_can_send_commands`: a raw text-frame client gets a JSON `Welcome`, a
      `Show` with `state.overrides["[0,5]"] == 77` and `state.groups["1"].name == "G1"`, and a `Dmx` stream
      (513 channels). Its JSON `Command` `SelectGroup(1)` gets `Result {seq: 7, outcome: Ok}`.
    - `a_client_of_another_protocol_version_is_rejected` (`Rejected` with the reason, connection closed, no
      client). `RemoteClient::connect` to a closed port fails with "Could not connect".
    - `desk_only_commands_are_refused_from_other_clients`: a local client in its own session and a
      `RemoteClient` get `Refused(DESK_ONLY_REFUSAL)`; the desk gets `Ok`.
    - `waiting_commands_fail_when_the_connection_drops`: a `send_then` closure gets `Refused(DISCONNECTED)` after
      `server.stop()`, and so does a later `send`.
    - `wire_types_round_trip`:
      - Host commands with `ModelBytes`, `PathBuf`, `SocketAddr` and `SetFrontendSection` round-trip in both
        encodings.
      - A bincode `Show` frame contains the plugin ABI bytes verbatim and decodes.
      - `ShowfileStatus` comes out as `{"path":…,"save":"Clean","saved_at":1700000000123}` and round-trips.
  - `server::json` (1): composite keys, number keys, every variant form, and values identical to `serde_json` for
    `SocketAddr`/`Duration`; NaN becomes `null`.
  - `server::snapshot` (1): bytes are kept only while wanted, changed and due; `latest` falls back to its own
    serialization when stale.
  - `log::tests::our_module_paths_map_to_subsystems` gained `blaulicht_engine::server` → "Web".
- `cargo clippy --workspace --all-targets --all-features`: blaulicht-engine lib 39 / lib test 42, blaulicht-ui-egui
  lib 89 / lib test 104, binaries 0. That is the 053 baseline. Three new-code warnings were fixed (large enum
  variants are now boxed, a type alias was added, struct init). `cargo build`: engine 17 + egui 64 = 81 warnings,
  unchanged. `cargo check -p blaulicht-engine --no-default-features` passes (only pre-existing unused imports).
- `cargo fmt`: touched files are clean. The usual unrelated hunks (`audio_features`, `pages/audio.rs`,
  `visualizer/ui.rs`, `app/ui.rs`) were reverted.
- `cargo tree -p blaulicht-engine | grep -E 'egui|eframe|epaint|winit|glow|three-d'` is empty, also with
  `-e normal --all-features`. The new dependencies are `tungstenite` 0.24 (default features: handshake, no TLS;
  it brings `http`, `httparse`, `sha1`, `data-encoding`, `utf-8`) and `bincode` 2.0.1 (already in the tree via
  shared).
- Plugins: `scripts/blenv build plugins` (default five) after the shared change, with no `^error`. ABI 18
  (inspector ping).
- **Frame time and snapshot traffic** (blenv, debug build, llvmpipe, kiosk 800x480, a copy of
  `crates/core/OUTDOOR.json`, the biggest showfile in the repo at 4.6 MB: 5 groups × 100 fixtures, 5 scenes, 2000
  changes in NORMAL):
  - **Method:** a temporary probe (not committed) timed `App::ui` (UI build and event handling, not GL paint) in
    300-frame windows. The remote client was `remote_probe`, run inside the container and subscribed to **all**
    streams. Phases were interleaved: base, remote, base, remote. Results are the mean of window means and the
    median of window p50s and p95s, in µs.

    | Page | base 1 | remote 1 | base 2 | remote 2 |
    |---|---|---|---|---|
    | Programmer, S0 selected (≈ 10 fps), mean / p50 / p95 | 53794 / 53626 / 56242 | 53989 / 53856 / 56126 | 54031 / 54031 / 56677 | 53878 / 53729 / 56154 |
    | View, mean / p50 / p95 | 390 / 351 / 476 | 402 / 355 / 493 | 384 / 351 / 466 | 397 / 351 / 484 |

  - **Programmer:** with a bincode remote client the differences are within the base-to-base spread.
  - **View:** p50 is unchanged, while the mean is about 12 µs (3 %) higher and the frame interval goes from
    5.34 ms to 5.45 ms. The UI thread does no extra work for a remote client. I put the difference down to CPU
    contention: in the same container, the probe process decodes about 9 snapshots of 126 KB per second and the
    server thread encodes them.
  - **A JSON client** (a raw Python client, show only) raises the View mean to 419 µs (p50 357, p95 548) and the
    frame interval to 6.0 ms (base 389 and 391 µs, interval 5.3 ms). Most of that is the server thread
    converting about 1.2 MB of JSON 8 times a second in a debug build, which competes with llvmpipe for CPU.
  - **Snapshot size and rate** (OUTDOOR; `remote_probe` over 2 × 60 s, `json_probe` over 10 s):

    | | bincode | JSON |
    |---|---|---|
    | Show snapshot | 125.6 KB (≈ 123 KB headless) | ≈ 1244 KB |
    | Show rate | 8.7 / s, continuously: the digest changes every tick while animations run | 8.2 / s |
    | Show traffic | ≈ 1.09 MB/s | ≈ 10 MB/s |
    | DMX stream | 27.5 / s, 2.2 KB each, 61 KB/s | |
    | Spectrogram stream | 17.5 / s, 85–129 KB/s (the first update holds all 40 columns) | |
    | System messages | 11 / s, 0.3 KB/s | |
    | Session, stage, status, health, plugin UI | once each (unchanged afterwards) | |

  - Desk memory after 4 remote connects and disconnects: RssAnon 439 MB, 45 threads (OUTDOOR, debug build). The
    desk's App log only shows the connect/disconnect lines and the usual DMX/MIDI/XSETTINGS/XRandR/thread-priority
    lines.
- **Headless** (`scripts/blenv build headless`, `run --headless --showfile OUTDOOR.json`): it logged "Remote
  clients can connect to ws://0.0.0.0:1234/", and `remote_probe` got the snapshots (5 s: 8.6 shows/s, 123 KB).
  `scripts/blenv stop` (SIGTERM) with a client connected: "Shutdown hook done (Quit)", then "Remote client remote 1
  … disconnected: the server stopped", then "Engine stopped". The client saw the close.
- **Visual checks** (screenshots read): the Programmer on OUTDOOR with S0 selected (5 groups, 100 fixtures shown),
  and the View page. There is no UI change in this ticket, so these only set up the frame-time measurement.
- Not done live: a remote client on another machine (the container's port isn't published), and a remote client
  sending showfile commands to a running desk (covered by engine tests since 052).
- Disk: my builds left about 16 GB of stale per-hash build directories in this worktree's own cache (the lockfile
  change gave the engine crates new hashes). I deleted the stale `blaulicht-{engine,ui-egui,core}` directories
  in `/build/target/debug/build`; no other worktree's data was touched.

### For 048 and 054
- **Stage models:** a remote visualizer can't load stage models yet. `StageAsset.relative_path` is a file next to
  the showfile on the engine host. The assets are already named by content hash (`<stem>.assets/<hash>.glb`), so
  054 can add a fetch-by-hash request (or an HTTP GET on the same port).
- **`ImportStageModel`** works over bincode, as a byte string inside the frame. Two problems remain:
  - The 64 MiB message limit is below `MAX_GLB_BYTES` (100 MiB), so a model between 64 and 100 MiB can't be
    imported remotely.
  - A JSON client would send the bytes as an array of numbers.

  054 needs binary frames, or an upload message, for JSON clients and big models.
- **Show traffic:** full snapshots of about 126 KB at about 9 Hz (1.1 MB/s; JSON 10 MB/s) while animations run.
  Follow-ups are per-section deltas, or keeping animation runtime state (timers) out of the snapshot (Inbox).
  The 1 MiB plugin ceiling still applies to the plugin path.
- **Ordering:** only the session is guaranteed current when a result arrives. A frontend that needs the show
  right after a command (create-then-use) should use `CreatedId` or wait for the next digest.
- **Showfile paths:** `SaveShowfileAs` and `LoadShowfile` take engine-host paths, and there is no "list
  showfiles" request yet.
- **Where the code lives:** `RemoteClient` and the wire types live in the engine crate (which pulls in wasmtime,
  cpal, midir and serialport). An egui-on-wasm frontend needs `server::protocol` and the types it references
  (`HostCommand`, `StageScene`, `AppHealthState`, `OperatorSession`, …) in a light crate first. The
  generic-versus-enum client question (042) is still open for `BlaulichtApp`.
- `SpectrogramUpdate` carries no `debug_data` (the audio page's debug view is empty remotely). `Status` carries
  whole plugin log buffers.
- **Authentication** is in the Inbox: anyone on the network who reaches the port can control the show and write
  files through `SaveShowfileAs`. `server_enabled = false` turns the server off.
