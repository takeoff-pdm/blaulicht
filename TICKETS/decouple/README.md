# E10 — Decouple engine and frontends

Goal: the core of Blaulicht runs headless. The native egui UI is one frontend plugged into it, and other
frontends (a web UI running alongside, for example) can attach at the same time.

Tickets in this folder follow the normal rules in [`../README.md`](../README.md) (global numbering, `planned` /
`doing` / `done` status in the file name, index in `TODO.md`). To filter by status, include this folder:
`find TICKETS -name '*.doing.md'`.

## Model: think of it like a web server

The engine is the server, and each frontend is a client. State falls into three layers:

| Layer | Web analogy | Contents | Lives in |
|---|---|---|---|
| **Show state** | database | groups, scenes, palettes, views, animation templates, overrides, overlays, scene graphs, folders, live preview, Art-Net receivers, audio params | engine; global; last write wins; every change is published to every client |
| **Operator session** | server-side session | selection, selection stack, control buffer, scene focus | engine; one per `SessionId`; commands resolve against their own session |
| **View state** | client-side / DOM state | current page and sub-page, dialogs, scroll, plugin UI windows (open, popped out, maximized, selected tab), external screens, visualizer camera | the frontend only; the engine never sees it |

Rules:
- **Only the engine mutates show state and sessions.** Frontends send commands (today's `ControlEvent`s, extended
  where needed) and never take a write lock on engine state.
- **Sessions:** the native UI and all locally hosted plugins (MIDI controllers like Korg or DDJ, the console)
  share the `DESK` session, so a group picked on a controller is highlighted on the device screen, as it is today.
  Every remote client gets its own session.
- **Performance:** the in-process frontend must be as fast as today. It reads show state through the shared `Arc`
  and sends commands over a crossbeam channel, with no serialization. Remote frontends only need to work correctly.
  Latency hiding (optimistic updates and the like) is out of scope for this epic.
- **Conflicts:** last write wins, like a web server without locking. Two operators fighting over the same fader
  is acceptable.

## Order

```
039 view state out of AppState ─────────────────────────────┐
040 operator sessions ── 041 live preview ── 042 EngineClient + Programmer pilot
                                               ├─ 043 Setup pages
                                               ├─ 044 Performance / View / Visualizer pages
                                               └─ 045 System / Audio pages
                                                        └─ 046 engine crate + headless binary
                                                                 └─ 047 websocket transport
                                                                          └─ 048 web frontend
```

## Interaction with E9 (Console UI)

036, 037 and 038 rewrite the same pages that 042–045 move onto commands. Don't run a page's E10 ticket and its
E9 ticket at the same time. Either order works, but the second one to land must rebase onto the first. Doing
the E10 ticket first means the redesign is written against `EngineClient` from the start.
