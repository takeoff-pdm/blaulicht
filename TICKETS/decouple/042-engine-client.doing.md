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
- [ ] `LocalClient` exists, and `BlaulichtApp` owns one.
- [ ] The two-client test harness works, with an isolation test (two sessions, two selections).
- [ ] No behaviour change; frame times are recorded.

## Verification
- `cargo clippy --workspace --all-targets --all-features`, `cargo fmt`, `cargo test --workspace --all-features`.
- blenv smoke test with screenshots.

## Notes
