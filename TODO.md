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
| 005 | [Generic sub-page navigation for all pages](TICKETS/005-subpage-navigation.planned.md) | planned | — |

### E2 — Setup page

| # | Ticket | Status | Depends on |
|---|---|---|---|
| 006 | [Setup page with Fixtures / Groups / Scenes sub-pages](TICKETS/006-setup-page-shell.planned.md) | planned | 005 |
| 007 | [Fixture setup UX: add-fixture affordance, default names, bulk rename](TICKETS/007-fixture-setup-ux.planned.md) | planned | 006 |
| 008 | [Fixture identify wizard](TICKETS/008-identify-wizard.planned.md) | planned | 006 |
| 009 | [View preparation into Setup › Views, with a Test button](TICKETS/009-views-into-setup-with-test.planned.md) | planned | 006, 020 |
| 010 | [Animation template editor and Palettes page become Setup sub-pages](TICKETS/010-editors-into-setup.planned.md) | planned | 006, 012 |

### E3 — Programmer

| # | Ticket | Status | Depends on |
|---|---|---|---|
| 011 | [Programmer page: rename, clear selection, scene changes with undo-all](TICKETS/011-programmer-page.planned.md) | planned | 006 |
| 012 | [Apply animation presets from the Programmer](TICKETS/012-programmer-animation-presets.planned.md) | planned | 011 |
| 013 | [Per-scene selection lock (freeze) in the Programmer](TICKETS/013-programmer-selection-lock.planned.md) | planned | 011 |
| 014 | [Bug: pointer to a color palette shows a different color](TICKETS/014-palette-pointer-color-fix.planned.md) | planned | — |
| 015 | [Apply only selected attributes of a compound palette](TICKETS/015-palette-partial-apply.planned.md) | planned | 011 |

### E4 — Hierarchy (folders)

| # | Ticket | Status | Depends on |
|---|---|---|---|
| 016 | [Folder hierarchy data model for groups, scenes and palettes](TICKETS/016-folders-data-model.planned.md) | planned | — |
| 017 | [Tree UI for group folders (Setup › Groups and Programmer)](TICKETS/017-folders-tree-ui-groups.planned.md) | planned | 016, 006 |
| 018 | [Tree UI for scene and palette folders](TICKETS/018-folders-tree-ui-scenes-palettes.planned.md) | planned | 016, 006, 010, 017 |

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

## Inbox
Raw ideas that are not ticketed yet. Add new notes here, and turn them into tickets once they are clear.
Items marked `→ TICKET NNN` are covered by that ticket.

- Port `midi_all` hold-overlay / hold-alpha mappings to the bank stack once 020 is done.
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
