# TODO

Work is tracked as tickets in [`TICKETS/`](TICKETS/README.md). Each ticket is one file named
`NNN-slug.<status>.md`, where the status is `planned`, `doing` or `done`. To change a status, `git mv` the file and update
the row below. Read [`TICKETS/README.md`](TICKETS/README.md) before picking up a ticket.

Target navigation (agreed 2026-09-27):
`Logs | System | Audio | Setup | Programmer | Performance | Visualizer`
- Setup: Fixtures, Groups, Scenes, Views, Animations, Palettes
- Programmer: selection, attributes, palette apply, animation presets
- Performance: Banks/Overlays, Graph
- Visualizer: 3D, DMX simulator

## Ticket index
### E1 — Foundations

| # | Ticket | Status | Depends on |
|---|---|---|---|
| 001 | [Unify host logging (one path to terminal + Logs page)](TICKETS/001-unify-host-logging.done.md) | done | — |
| 002 | [Visible startup progress (per-stage checklist)](TICKETS/002-startup-progress.done.md) | done | 001 |
| 003 | [Showfile name instead of abspath; audio page header order](TICKETS/003-showfile-name-and-audio-header.done.md) | done | — |
| 004 | [Selectable dummy audio input (silence / noise)](TICKETS/004-audio-dummy-input.done.md) | done | — |
| 005 | [Generic sub-page navigation for all pages](TICKETS/005-subpage-navigation.done.md) | done | — |

### E2 — Setup page

| # | Ticket | Status | Depends on |
|---|---|---|---|
| 006 | [Setup page with Fixtures / Groups / Scenes sub-pages](TICKETS/006-setup-page-shell.done.md) | done | 005 |
| 007 | [Fixture setup UX: add-fixture affordance, default names, bulk rename](TICKETS/007-fixture-setup-ux.planned.md) | planned | 006 |
| 008 | [Fixture identify wizard](TICKETS/008-identify-wizard.planned.md) | planned | 006 |
| 009 | [View preparation into Setup › Views, with a Test button](TICKETS/009-views-into-setup-with-test.planned.md) | planned | 006, 020 |
| 010 | [Animation template editor and Palettes page become Setup sub-pages](TICKETS/010-editors-into-setup.done.md) | done | 006, 012 |

### E3 — Programmer

| # | Ticket | Status | Depends on |
|---|---|---|---|
| 011 | [Programmer page: rename, clear selection, scene changes with undo-all](TICKETS/011-programmer-page.done.md) | done | 006 |
| 012 | [Apply animation presets from the Programmer](TICKETS/012-programmer-animation-presets.done.md) | done | 011 |
| 013 | [Per-scene selection lock (freeze) in the Programmer](TICKETS/013-programmer-selection-lock.planned.md) | planned | 011 |
| 014 | [Bug: pointer to a color palette shows a different color](TICKETS/014-palette-pointer-color-fix.planned.md) | planned | — |
| 015 | [Apply only selected attributes of a compound palette](TICKETS/015-palette-partial-apply.planned.md) | planned | 011 |

### E4 — Hierarchy (folders)

| # | Ticket | Status | Depends on |
|---|---|---|---|
| 016 | [Folder hierarchy data model for groups, scenes and palettes](TICKETS/016-folders-data-model.done.md) | done | — |
| 017 | [Tree UI for group folders (Setup › Groups and Programmer)](TICKETS/017-folders-tree-ui-groups.done.md) | done | 016, 006, 030 |
| 018 | [Tree UI for scene and palette folders](TICKETS/018-folders-tree-ui-scenes-palettes.done.md) | done | 016, 006, 010, 017, 030 |
| 030 | [Reusable collapsible tree widget](TICKETS/030-tree-widget.done.md) | done | — |
| 031 | [Folder follow-ups (ids, undo, order, names) and the unloadable Kuze showfile](TICKETS/031-folders-followups.done.md) | done | 016 |

### E5 — DMX overrides

| # | Ticket | Status | Depends on |
|---|---|---|---|
| 019 | [DMX overrides: move to System › DMX, layout per mode, active toggle](TICKETS/019-dmx-overrides-redesign.planned.md) | planned | — |

### E6 — Performance

| # | Ticket | Status | Depends on |
|---|---|---|---|
| 020 | [Engine bank stack: save/restore overlay sets safely](TICKETS/020-engine-bank-stack.planned.md) | planned | — |
| 021 | [Design: merge ViewPerformance and SceneGraph into one Performance page](TICKETS/021-performance-page-design.planned.md) | planned | 020 |
| 022 | [Performance page (ViewPerformance + SceneGraph)](TICKETS/022-performance-page-merge.planned.md) | planned | 021, 005 |
| 023 | [Grand master control redesign](TICKETS/023-grand-master-redesign.planned.md) | planned | 022 |

### E7 — Visualizer

| # | Ticket | Status | Depends on |
|---|---|---|---|
| 024 | [Move the 2D DMX simulator into the Visualizer](TICKETS/024-simulator-into-visualizer.planned.md) | planned | 005 |
| 025 | [Visualizer bug triage (produces tickets)](TICKETS/025-visualizer-bug-triage.planned.md) | planned | — |
| 026 | [Research exact fixture and truss dimensions](TICKETS/026-fixture-dimension-research.planned.md) | planned | — |
| 027 | [Visualizer position assistant (grid placement with layers)](TICKETS/027-visualizer-position-assistant.planned.md) | planned | 026 |

### E8 — Plugins

| # | Ticket | Status | Depends on |
|---|---|---|---|
| 028 | [Plugin list: kind tags, no Show UI for animation plugins](TICKETS/028-plugin-list-ux.planned.md) | planned | — |
| 029 | [Plugins can open/close their own UI and navigate pages](TICKETS/029-plugin-self-ui-control.planned.md) | planned | 005 |

### E9 — Console UI

Design system: [Blaulicht Console](https://claude.ai/artifact/ChF4HkbnuyT8bycizagoja) (Console direction, navy palette; chosen 2026-10-30).

| # | Ticket | Status | Depends on |
|---|---|---|---|
| 032 | [UI tokens, density and theme foundation](TICKETS/032-ui-tokens-density.planned.md) | planned | — |
| 033 | [Core components: Button variants, ListRow, Chip, Status](TICKETS/033-ui-core-components.planned.md) | planned | 032 |
| 034 | [Sheets, context menus, toasts and engine refusal feedback](TICKETS/034-ui-overlays-feedback.planned.md) | planned | 032, 033 |
| 035 | [Shell: page header with action overflow, nav rail and tab bar](TICKETS/035-ui-shell-header-nav.planned.md) | planned | 032, 033 |
| 036 | [Programmer in the Console layout](TICKETS/036-ui-programmer-redesign.planned.md) | planned | 033, 034, 035 |
| 037 | [Setup tabs as master–detail in the Console style](TICKETS/037-ui-setup-master-detail.planned.md) | planned | 033, 034, 035 |
| 038 | [Remaining pages in the Console style](TICKETS/038-ui-remaining-pages.planned.md) | planned | 033, 034, 035 |

### E10 — Decouple engine and frontends

Headless core, with the native UI as one frontend among several. Model, rules and order: [`TICKETS/decouple/README.md`](TICKETS/decouple/README.md). Rows are in suggested order.

| # | Ticket | Status | Depends on |
|---|---|---|---|
| 039 | [Move view state out of AppState](TICKETS/decouple/039-view-state-out-of-appstate.done.md) | done | — |
| 049 | [Engine event hub: client ids, envelopes, replies and broadcast](TICKETS/decouple/049-engine-event-hub.done.md) | done | — |
| 040 | [Operator sessions in the engine](TICKETS/decouple/040-operator-sessions.done.md) | done | 049 |
| 041 | [Live preview owned by a session](TICKETS/decouple/041-live-preview-show-state.done.md) | done | 040 |
| 042 | [EngineClient trait and LocalClient](TICKETS/decouple/042-engine-client.done.md) | done | 049, 040, 041 |
| 050 | [Programmer onto EngineClient](TICKETS/decouple/050-programmer-onto-client.done.md) | done | 042 |
| 043 | [Setup pages onto EngineClient](TICKETS/decouple/043-commands-setup-pages.done.md) | done | 042 |
| 044 | [Performance, View and Scene graph pages onto EngineClient](TICKETS/decouple/044-commands-performance-view-graph.done.md) | done | 042 |
| 051 | [Visualizer onto EngineClient; stage geometry becomes show state](TICKETS/decouple/051-visualizer-stage-into-engine.done.md) | done | 042 |
| 045 | [Audio, Art-Net and System pages onto EngineClient](TICKETS/decouple/045-commands-audio-artnet-system.done.md) | done | 042, 039 |
| 052 | [Engine owns showfile, config and lifecycle](TICKETS/decouple/052-engine-owns-persistence.done.md) | done | 049, 042, 051, 043, 044, 045, 050 |
| 046 | [Remove egui from engine-side code](TICKETS/decouple/046-egui-out-of-engine-code.done.md) | done | — |
| 053 | [Engine crate, egui crate and a headless binary](TICKETS/decouple/053-crate-split-headless.done.md) | done | 039, 046, 050, 043, 044, 051, 045, 052 |
| 047 | [Websocket transport and RemoteClient](TICKETS/decouple/047-websocket-transport.done.md) | done | 053 |
| 048 | [Web frontend spike: egui on wasm or Svelte](TICKETS/decouple/048-web-frontend-spike.done.md) | done | 047 |
| 054 | [Web frontend](TICKETS/decouple/054-web-frontend.done.md) | done | 048 |

### E11 — Desktop layout

| # | Ticket | Status | Depends on |
|---|---|---|---|
| 055 | [Desktop / mobile layout setting with a desktop navigation sidebar](TICKETS/055-desktop-mobile-layout.planned.md) | planned | — |

## Inbox
Raw ideas that are not ticketed yet. Add new notes here, and turn them into tickets once they are clear.
Items marked `→ TICKET NNN` are covered by that ticket.

- Web UI (054) follow-ups: the visualizer on the web (048 row 14: GLSL `300 es`, depth/MSAA, its two threads, stage models fetched from the engine host); showfile Load / Save As for remote UIs (needs a "list showfiles" request, since paths are the engine host's); a native remote desk (`blaulicht-core --connect host:port` with `AppClient::Remote`, mostly wiring); the web UI repaints at 60 fps like the desk even when idle, which costs battery on tablets; touch use was only tested with a mouse in desktop chromium (canvas zoom-to-fit, egui's hidden text input for the keyboard), not on a real tablet or phone.
- Log lines replayed to a newly connected frontend (the hub keeps the last 1000) are stamped with the time they arrive, not when they were logged (`LogWindow::add_log` takes `Local::now()`): a web UI opened later shows the whole replay at one time (054). `SystemMessage::Log` has no timestamp yet.
- The engine is compiled for wasm32 by the web build only "by accident" (048 row 13): lifecycle, main loop, plugin host, server and persistence compile but would fail at runtime in a browser. A `runtime` feature (or a light model crate) would let the compiler catch web-reachable runtime code.

- `crates/plugins/pioneer_pro_dj_link/Cargo.toml` has an absolute path dependency (`/home/mik/Downloads/prolink-test/prolink-rs`), so `make build_noopt` fails for it everywhere else.
- Port `midi_all` hold-overlay / hold-alpha mappings to the bank stack once 020 is done.
- In `--desktop` mode, `NavigatePage` / `NavigateSubPage` don't bring the page's dock tab to the front (found in 005).
- In `--desktop` mode, a dock pane in "Default (723 x 480)" render mode that is shorter than 480 px clips the sub-page tab bar (System, Setup); Dynamic mode shows it (found in 006).
- Fixture tiles use `name.truncate(10)`, which panics when byte 10 is inside a multi-byte char (e.g. umlauts) (`group_selection` in `fixtures_shared.rs`, found in 006). The group tiles (`group_tile`) are gone since 017; the group trees truncate by width.
- In `--desktop` mode, the Programmer in a "Default (723 x 480)" pane is narrower than the kiosk page, so its right column (masters, Scene Changes) is cut off at the pane edge; Dynamic mode fits. 017's wider group tree column (120 px instead of 90) makes the cut about 30 px larger; by the kiosk's ~10 px slack it was probably cut by ~20 px before, but that wasn't checked (found in 017).
- Removing a scene change (`RemoveChange`, Programmer row X / Undo all) sets the property to 0 in the scene instead of restoring an earlier value (found in 011).
- At 800x480 the Programmer's right column (scene changes + animation cards) does not scroll, so only the first animation card of a selection is fully visible; the Animations dialog lists all of them (found in 012).
- Setup › Animations: the template details form does not scroll, so long forms (audio modulation: section multipliers and below) run under the tab bar at 800x480. Already cut off on the old Animations page, a little less (found in 010).
- Setup › Animations: the Starter Presets dialog is 520 px tall, so at 800x480 only one preset row shows between its filters and Close (unchanged since before 010).
- In `--desktop` mode, loading a dock layout that lacks pages appends them to the first leaf with `push_to_first_leaf`, which also makes the last appended tab active, overriding the saved active tab (seen in 010).
- Three scene/palette pickers outside 018's scope still show flat lists: the palette binding picker (`palette_binding_button` in `components/fixture_value_binding.rs`, used by the animation spec editor), the pointer-palette target picker (`render_pointer_editor` in `pages/setup/palettes.rs`) and the Scene Graph's "+ Add Scene" combo box (`pages/scene_graph.rs`, which also lists BLANK). `tree_picker_dialog` with `palette_tree_items` / `scene_tree_items` fits all three; the first two need the folders passed in (found in 018).
- Programmer palette dialog: tapping a palette whose properties another active palette already covers is rejected by the engine with no feedback in the UI (same with the old Available list; found in 018).
- Programmer scene column: when the focused scene's folder is collapsed, nothing shows which scene is focused (the folder row is not highlighted) (found in 018).
- The background worker's debug spectrogram dump (`BLAULICHT_SPECTROGRAM_DIR`, `crates/engine/src/mainloop/bg_worker.rs`) is always black: it is 480x100, so after the 5/20 px padding 75 rows remain for 128 bins and `create_spectrogram_image` returns early (`height < bin_count`). It needs a height of at least 153 px (found in 046).
- In `--desktop` mode, `-e W,H` external screens never open: `drive_external_screen` is only called from the kiosk branch of `BlaulichtApp::ui` (`render_main_screen`); the System › Screens list shows them (seen in 053, code unchanged).
- The release archives (top-level `Makefile`) contain only the desk binary; `blaulicht-headless` (`cargo build -p blaulicht-engine --bin blaulicht-headless`) isn't packaged yet (053). Neither is the web UI bundle (054): `make build-web` builds it into `target/web` on the host, but the `blaulicht-cross` image has no `wasm32-unknown-unknown` target or wasm-bindgen-cli; the archive should carry it as `web/` next to the binaries, where the engine looks.
- The remote client server (047, `crates/engine/src/server/`) has no authentication: anyone on the network who reaches `port` (default 1234, on all interfaces) can control the show, and `HostCommand::SaveShowfileAs` / `LoadShowfile` take paths on the engine host, so a client can write a showfile anywhere the engine may write. Add authentication (and maybe a bind address or read-only clients) before using it outside a trusted network; `server_enabled = false` turns it off.
- The show state changes on every tick while animations run (OUTDOOR: the bincode snapshot's digest changes continuously), so remote clients get a full show snapshot at the 10 Hz cap: about 1.1 MB/s in bincode and about 10 MB/s in JSON for OUTDOOR (047). Per-section deltas, or keeping animation runtime state out of the snapshot, would cut that. Measured in 048: every change is `scenes[*].sink.active_animations[*].fixture_timers` (`last_tick_time`, `timer`; 129 KB of the 1242 KB JSON snapshot); `fixture_states`, `changeset`, `groups` and the rest stay unchanged, so keeping the timers out of the snapshot and its digest makes idle traffic about zero. `changeset` is a `HashSet`, so its JSON order differs between snapshots (matters for text diffs). 054 measured the web UI on loopback (blenv, debug engine): command round trips had a median of 18 ms on Kuze and 42 ms on OUTDOOR (slowest 65 / 100 ms), the latter in line with the engine's 34 ms loop, so commands don't queue noticeably behind snapshots; on a tablet over Wi-Fi the 1.1 MB/s (≈9 Mbit/s) for OUTDOOR is the bigger concern.
- add UDP sink(s) to which a DMX frame can be sent
  - input or output??
  - if output was meant, we have this -> artnet
- make dmx channel writes explicit so that only the updates are sent via UDP (more efficient)
- make audio signal source persistant
  - is persisted in config file
- add audio normalization (don't care what the input signal is)

- chaining animations
- fix stupid frequency animation BS
- Track down panics
- Fix long reload / load times
- More forgiving config loading.
- Configuration checkpoints / diffs.
- Trigger DMX setup async
- Flight Recorder -> trace stuff to reconstruct crashes
- Export Showfile with a list of feautures [only patches, complete state]
- Removing changes from a scene → TICKET 011
  - most important
- Setting a scene to output / select??
- Multiple engine selections or states
- Track down stupid animation bug with selections
- Better views UI → TICKET 009, 021/022
- Using more faders for views
- View shortcuts on MIDI devices
- Jumping into arbitrary scenes and changing values in them WHILE THEY PLAY
  - 2-state mode?
- Set Master Hue in scene
- Enforce fader return-to position
  - affects plugins
  - maybe add guardrail in DMX engine to reject changes over 5-10% of the current value?
- Save audio settings in showfile
  - why?
- enable / disable plugins from the UI
  - please
  - also reload?
  - better logs display per-plugin → TICKET 001
    - suggestion: on the terminal page
- fix beat detection
- split fixture setup into multiple tabs (scenes, groups) → TICKET 006
- external BPM 'suggestions'
- monitor plugin
  - watches number of monitors
  - reruns the devilspie2 script
  - kills / starts devilspie2 correctly
  - sends commands (bpf::emit?) to add / remove external monitors
  - maybe prompts the user before adding the screen?
- save shofwiles as zip?
  - easier overview, avoids messy JSON for the plugin state -> can become massive
  - also add showfile versioning: check compatibility
  - is there a framework / rust crate for these incremental file versions?
  - optimistic parsing?

## Archive: review notes 2026-09-27
The notes that tickets 001–029 were created from (formerly `crates/core/TODO.md`), kept verbatim.

```text
- noticed that logging is a bit unstructured and wild
- no clear overview whether system startup is finished or still in progress
- showfile does not need abspath, maybe under detail or in console log
- audio page maybe put current input to the very left, and change device right next to it.
    - also use a dummy audio device if no audio source is selected
- fixture groups should be hierarchical
  - same for scenes
  - also selection in the programmer should have a 'clear'
    shortcut
  - also, it would be nice to 'freeze' a selection in the programmer ui
    - also easy view of the scene changes, and one button to undo all
    - also scene animations / scene changes do not have to belong in the fixtures setup, but stuff like clone scene, etc should go in there.
    - also in fixture setup / group setup, allow bulk rename / use better default names for fixtures
    - also allow an 'identify' wizard
- fixture setup owns the simulator? - why?
    - i think this should be part of the visualizer.
- dmx overrides look scuffed but fine
    - more spacing on the left
    - different dialog designs depending on desktop / integrated
    - also add a toggle to activate / deactivate the override.
- fixture setup add fixture should just be gray, but still clickable when no group is selected
    - fixture setup should have nothing to do with scenes
    - maybe sub-pages for scenes and groups
- view preparation looks a bit ugly as well, and should be integrated into the setup meta page.
    - also there shouldn't be a single 'apply', but rather also a 'test' button

- maybe extend view performance: multiple 'banks', each with multiple overlay scenes,
    - also the grand master is ugly

- also fixtures performance should rather be 'programmer'
    - also its questionable whether we need a separate page for the animations?
    - i think not really, but would be nice to have presets.
    - also palettes are a bit fucked?
        - color gets skewed
        - no possibility of using only one attribute of a compound palette
            - would be nice to do this during the apply step in the programmer
        - like everything, we need hierarchies

- Graph is a bit random.
  - Merging graph and view performance into one 'Performance page'?
- visualizer is also a bit buggy
  - maybe do a 'position assistant?'
    - maybe allow it to be grid-pinned and pixel-art like?
    - vertical will be 'layers'
    - also research exact dimensions from official docs

- also allow plugins to open themselves and close themselves
  - also allow sub-page navigation
- do not include show ui button for animation plugins
  - also add icons / tags in the ui for different plugin kinds
  - also make the show ui button inside the plugin entry
```
