# UI port: desktop egui to the studio mockups

These tickets port the desktop egui UI to match the agreed mockups in `design/studio` closely: every page, with
Visualizer and Scene Graph only matching the shell. Numbering is global (see `../README.md`).

Sources of truth, in order: the mockup page itself, `design/studio/RULES.md`, `design/studio/UI_GAP.md`.
Open the mockup next to the app and compare screenshots at the same window size. The kiosk (800x480,
`Density::Compact`) must not change.

Base tickets that stay: 058 (components), 059 (status bar, Blackout, connection), 062 (DMX page), 064 (command
replies). 036, 037 and 038 are superseded by the tickets here.

| Ticket | Title | Depends on |
|---|---|---|
| 069 | [Studio tokens, 40px bars, folder bands and lit buttons](069-ui-tokens-bars-livebutton.planned.md) | 058 |
| 070 | [Stacked notifications](070-ui-notifications.planned.md) | 058 |
| 071 | [One dropdown component](071-ui-dropdown.planned.md) | 058, 069 |
| 072 | [Typing values in place and keyboard control of faders and knobs](072-ui-value-entry-keyboard.planned.md) | 058 |
| 073 | [Drag items between folders in Setup lists](073-ui-folder-tree-drag.planned.md) | 069 |
| 074 | [Status bar issues cell](074-ui-status-bar-issues.planned.md) | 059, 070 |
| 075 | [Programmer layout, page bar and resizable panes](075-ui-programmer-layout.planned.md) | 069, 071, 072 |
| 076 | [Programmer colour picker and Add animation](076-ui-programmer-colour-animations.planned.md) | 075 |
| 077 | [Setup Fixtures: multi-select and Move to group](077-ui-setup-fixtures-multiselect.planned.md) | 069, 071, 073 |
| 078 | [Identify panel and Initialize for a selection](078-ui-setup-identify.planned.md) | 077 |
| 079 | [Fixture position view](079-ui-setup-position-view.planned.md) | 077 |
| 080 | [Setup pages: title bars and detail panes](080-ui-setup-pages-polish.planned.md) | 069, 073 |
| 081 | [Views page (formerly Scene Stacks)](081-ui-views-page.planned.md) | 069, 071 |
| 082 | [Perform page bar and horizontal fader](082-ui-perform-page.planned.md) | 069 |
| 083 | [Logs page](083-ui-logs-page.planned.md) | 071 |
| 084 | [System General and Screens](084-ui-system-general-screens.planned.md) | 069, 074 |
| 086 | [Plugins, MIDI & Serial, Remote UI and Audio pages](086-ui-system-audio-pages.planned.md) | 069, 071 |
| 085 | [Visualizer and Scene Graph in the studio shell](085-ui-visualizer-scenegraph-shell.planned.md) | 069 |
