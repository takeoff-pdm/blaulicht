# E10 — Decouple engine and frontends

Goal: the core of Blaulicht runs headless. The native egui UI is one frontend plugged into it, and other
frontends (a web UI running alongside, for example) can attach at the same time.

Tickets in this folder follow the normal rules in [`../README.md`](../README.md) (global numbering, `planned` /
`doing` / `done` status in the file name, index in `TODO.md`). To filter by status, include this folder:
`find TICKETS -name '*.doing.md'`.

Revised 2026-10-31 after five parallel code reviews. The original 040, 042, 044, 045, 046 and 048 were split
into new tickets 049–054.

## Model: think of it like a web server

The engine is the server, and each frontend is a client.

| Layer | Web analogy | Contents | Lives in |
|---|---|---|---|
| **Show state** | database | groups, scenes, palettes, views, animation templates, overrides, scene graphs (minus focus), folders, live preview, **stage geometry** (trusses, models, attachments), non-plugin Art-Net receivers, audio params, grand master, per-show plugin state | engine; global; last write wins |
| **Operator session** | server-side session | selection, selection stack, control buffer, scene focus, focused scene graph | engine; one per `SessionId` |
| **View state** | client-side state | page and sub-page, dialogs, scroll, plugin UI windows (open, popped out, maximized, tab), external screen windows, visualizer camera and editor | the frontend; the desk frontend's part is stored in the showfile as an opaque `ui` section that the engine keeps but never interprets |
| **Config** | server config | `config.toml`: last showfile, audio device, global plugin state, plugin list | engine; it is the only writer |
| **Live streams** | server-sent events | DMX buffers, spectrogram, health, tick speeds, logs, plugin UI ops, startup progress | engine publishes them; clients subscribe to them read-only |
| **Engine internals** | — | plugin runtime bookkeeping, animation outputs, timers, `external_tempo`, `spawned_commands`, `mainloop_state` | engine only |

`current_overlay_scenes` is rewritten by the scene-graph tick every frame, so treat it as derived output.

## Rules

- **Only the engine mutates show state, sessions and config.** Frontends send commands and never take a write
  lock on engine state. At the end of the epic, `BlaulichtApp` holds only an `EngineClient`, and the compiler
  enforces that (052).
- **Messages on the bus** are host-only envelopes `{ client: ClientId, session: SessionId, seq, body }`
  (049). `body` is either a `ControlEvent` (as today; forwarded to plugins) or a `HostCommand` (host-only;
  never serialized to plugins). Plugins keep receiving plain `ControlEventMessage`s, so the envelope itself is
  **not** a plugin ABI change.
- **New UI-only operations go into `HostCommand`, not `ControlEvent`.** Every `ControlEvent` change bumps
  `PLUGIN_ABI_VERSION` and is serialized to every plugin every tick. Add a `ControlEvent` only if plugins
  should be able to send or see it.
- **Replies:** each command gets a `CommandResult { client, seq, outcome }` on the sender's subscription (049),
  so dialogs can close on success, show refusals and learn new ids.
- **Sessions:** the native UI and all locally hosted plugins share `SessionId::DESK`. Remote clients get their
  own. The `DESK` session stays mirrored in the existing `EngineState` fields (so plugins and the showfile
  are unchanged). Other sessions live in a side store, and the engine swaps the sender's session in while it
  applies an event (040).
- **Performance:** the in-process frontend must be as fast as today. It reads through
  `with_show(|s| ..)` (a read guard locally) and sends over a crossbeam channel, with no serialization.
  Remote frontends only need to be correct. Latency hiding is out of scope.
- **Conflicts:** last write wins. Structural changes (deleting a scene or group, patch edits) repair every
  session, not only `DESK`.

## Order

```
039 view state ─────────────────────────────────────────────────────────────┐
049 event hub ── 040 sessions ── 041 live preview ── 042 EngineClient ─┬─ 050 Programmer
                                                                       ├─ 043 Setup
                                                                       ├─ 044 Performance / View / Graph
                                                                       ├─ 051 Visualizer + stage
                                                                       ├─ 045 Audio / Art-Net / System
                                                                       └─ 052 persistence + lifecycle (after 051)
046 egui out of engine code (independent) ──────────────────────────────────┤
                                                    053 crate split + headless ◄┘
                                                    └─ 047 websocket ── 048 web spike ── 054 web frontend
```

043, 044, 045, 050 and 051 touch different pages but all edit `dmx/mod.rs` and the `HostCommand` enum.
Run them one after another, not in parallel.

## Interaction with other epics

- **E9 (Console UI):** run the E10 ticket for a page first, so the redesign is written against `EngineClient`:
  036 after 050, 037 after 043, 038 after 044, 045 and 051.
- **013** (selection lock) depends on 040: a lock is show state and applies to every session focused on that
  scene.
- **020** (bank stack): the bank owner is a `ClientId`. Pop pushes when a remote client disconnects, as well as
  when a plugin crashes.
- **029** (plugin self UI control) depends on 039. Plugin `NavigatePage` / `SetPluginUIOpen` go to the desk
  frontend only.
- **034** (refusal feedback) should use `CommandResult` from 049.
- **053** renames crate paths, so it runs alone and sweeps the paths in every planned ticket.
