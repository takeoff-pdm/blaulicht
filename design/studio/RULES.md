# Desktop UI rules (draft)

These mockups are throwaway HTML used to settle the desktop design before the egui port.
Open any `.html` file here directly in Chrome. `system.html`, `setup.html`, `pages.html` and
`animation-editor.html` have their own navigation; `programmer.html` is the flagship page.

Status: **agreed** on 2026-11-05.

## Page shell

Every page has the same four regions and nothing else:

| Region | Contents |
|---|---|
| Sidebar | Navigation only |
| Page bar | Page-level choices: view tabs on the left, page actions on the right. Left out if the page has neither. Fixed height, never scrolls |
| Panes | Titled panes on one grid with 2px gaps. One pane per semantic unit, sized by its content |
| Status bar | Identical on every page, see below |

- Pane title bars hold only actions that affect that pane. Anything that affects the page goes in the page bar.
- Exception (2026-11-08): when a page has a single pane, its filters may sit in the page bar instead of a second row inside the pane. Logs does this: search, level chips (icon + count, click to hide a level), source dropdown on the left; Follow, Copy, Clear on the right.
- Sub-pages stay in the sidebar. View tabs are for different views of the same data, such as Map and Plan.

## Status bar

Same cells in the same order on every page, drawn by one shared component:
showfile and save state, audio, BPM, engine load, plugin load, then on the right one problems cell ("4 issues", led by an error icon and/or a warning icon for the kinds present; the tooltip splits the count) that opens the problem list. Output failures such as an offline universe count as errors there instead of having their own cell.

A healthy cell stays quiet. A problem colours that cell and clicking it opens the page that can fix it.

## Controls

The value decides the control, not the page:

| Value | Control |
|---|---|
| Fixture attribute you play live | Vertical fader |
| Master or trim, set occasionally | Knob |
| Long-range value that needs a timeline or level feel, such as audio gain | Horizontal fader |
| Exact number | Number field; every fader and knob also shows one. Double-click a fader's value or name, or a knob, to type the value in place |
| Two or three linked values | Dedicated pad or picker (XY pan/tilt; colour as a hue ring around a saturation and brightness triangle) |
| On or off setting that is saved | Switch |
| Row selection in a list or table | Checkbox |
| On or off action you play live | Lit button |
| One of 2 to 5 visible options | Segmented control |
| One of 6 or more options | Dropdown (`BL.dropdown`) |
| Several of a few options, such as log levels | Multi-select dropdown with checks |

- Every dropdown is one component, `BL.dropdown` in `chrome.js`: a popover on the pane surface with a 1px line, folder bands for groups, a filter field from 8 options, arrow keys, Enter and Esc. Multi-select rows have checks and an All row; Alt-click picks only that row and the list stays open. Native `<select>` elements open the same popover. Only the scene picker dims the page.
- No browser-default sliders. `pages-orig.js` still uses them.
- One fader component and one knob component, in two sizes at most.
- Inputs are sized to their content and never stretch to fill a pane.

## Colour

Each colour has one meaning:

| Colour | Meaning |
|---|---|
| Blue fill | Selected: the row, tab or chip you are working on |
| Amber value text and fader marker | Value changed in the Programmer |
| Green | Running or playing, only where it is the point of the control |
| Amber with a warning icon | Needs attention |
| Red | Output at risk, plus Blackout, plus the fixture being identified (red row, wins over blue selection). Exclusive preview's dot blinks red while on (steady with reduced motion) and is grey when off |
| Fixture colours | Only the lighting data itself |

Healthy states show nothing. Patch map universes use neutral chrome, not blue.

## Type and targets

- Text is 12px, with 11px as the minimum. The 10px sizes go away.
- Plex Mono is for numbers, channels and addresses only.
- Minimum desktop hit target is 24px, with 28px for controls used during a show.

## Notifications

- Cards stack in the bottom right above the status bar, newest at the bottom, at most 4 at once. Each has a type icon and edge (blue information, red error), the message, actions as blue links, and a close button.
- Information closes after 5 s, or 8 s with an action, paused while hovered. Errors stay until closed.
- A repeated message counts up (×2) instead of adding a card.

## Dangerous actions

- Never next to everyday actions, and always visually distinct.
- Destructive actions (`DEL`, Delete, Shut down, Clear all) ask for confirmation. There is no undo.
- Blackout is one press, with no confirmation. `Cmd/Ctrl+B` works from any page and inside text fields, and ignores key repeat. The status bar shows it on every page and restores the previous level.
- Shut down, Restart and Quit move into one Application pane.

## Required states

Every page must have a designed version of:

- Empty, with the action that fills it
- Engine disconnected or restarting
- 500 fixtures, 50 palettes, and names of 40 or more characters

## Scope

Desktop layout only. The 800x480 kiosk stays as it is, and the Compact density keeps kiosk
drawing unchanged. The web UI uses this layout with `?layout=desktop`, so anything engine-local,
like Shut down, must be hidden when `AppClient::is_remote` is true.

## Decisions

1. Programmer: Groups and Fixtures, then faders (440x320), wheel and pad in one row, with Palettes and the Scene pane (animations and Scene changes) below them. The scene being edited is a picker in the page bar, a filterable list of scene folders that opens over the page dimmed by 55%, with its master intensity and speed as small knobs beside it. Below 1400px wide, wheel and pad stack under the faders, Palettes sits under them, and the Scene pane runs the full height. Below 1200px wide, the sidebar also collapses to icons. Faders, wheel and pad keep their size at every width. Panes can be resized and folded, see Resizable panes. The page bar holds the scene picker and masters, Blackout, Reset layout and Exclusive preview, all 28px tall; other actions sit in the pane they affect.
2. Changed values are amber. Selection stays blue.
3. Plugin UI is embedded in the Plugins page, with a button to pop it out to its own window.
4. The mockups stay in the repo with their real show data.

## Trying the states

Add these to any page's address:

- `?state=disconnected` or `?state=restarting`: engine banner, controls paused
- `?state=savefail`: Save shows the failure dialog
- `?remote`: hides Shut down, Restart and Quit, as the web UI must

Blackout toggles Grand Master to 0 and restores the previous level, kept by the UI, so no engine change is needed. `Cmd/Ctrl+B` triggers it.

## Lists

Items move between folders by dragging them in the Setup lists (onto a folder or any item in it; below the folders means no folder where the kind allows it, which is Groups). Detail panes have no Folder field. Live lists (Programmer) never move by drag. Recessed list background, folder bands in the title-bar grey with a 1px line on top (decided 2026-11-11, A4 B1 on `folder-accent-variants.html`), a grey folder icon and a tree line, hairlines under rows. The active row is bright blue with a left edge; included rows have a thin blue outline and no fill. Counts show numbers only, folder counts only when collapsed, and scene IDs are faint until selected.

## Resizable panes

- Dividers trade space between neighbouring panes that can use it: the lists, Palettes and the Scene pane. Controls with a fixed size have no divider of their own.
- Every Programmer pane folds to its title and reopens from it. Double-click a divider to restore its default.
- Dividers keep the 2px gap and have an 8px grab area, an agreed exception to the minimum target size.
- Reset layout restores everything.
- Panes cannot be moved, tabbed or floated. Sizes are saved per machine, not in the showfile, and separately for the rack and the stack.

## Placeholders

Limit and Limit all on the Programmer are dummy buttons: visible, with no action, until their behaviour is decided.

## State honesty

- One save state drives the status bar and every dialog.
- Offline: live values dim and stop, commands report "not sent", and nothing is queued.
- Text fields that change the lights apply on Enter or leaving the field; Esc cancels. Esc also cancels a drag.
- A clicked or tabbed-to fader or knob takes focus, shown by its label turning blue. Arrow keys then move it by its smallest unit: 1 on faders (a degree on Hue), 1% on intensity, one notch on speeds. Holding a key speeds up after 300 ms and caps after about 2 s at a 25th of the range per repeat (10 on 0-255 faders); speeds never accelerate.
- Typed values are clamped to the control's range and use its units: percent for master intensity, 1/4 or 2x for speeds, degrees for hue. A value set by a palette can't be typed over.
- Removing more than one thing asks first and names the consequences. Removing a single Scene changes row doesn't ask.
- Backspace or Delete on the selected fader undoes that value's changes from this session for the selected fixtures, without asking, and drops changes this session added. Pressed again with nothing left to undo, it removes the scene's earlier changes for that value from the list.
- Tables select like a desktop list: click selects one, Ctrl/Cmd+click toggles, Shift+click selects a range. With several fixtures selected, per-fixture fields (universe, start channel, position) are frozen and show `--`; actions that make sense for many (Move to group, Identify, Initialize, Delete) stay.
- Identify drops down a small panel from its button (Previous, Next, ←/→, Transfer to selection, Stop, Esc) that flashes one fixture at a time and marks its row red: through the selection, or through the group when one fixture is selected.
- Selected items hidden by a filter are called out. No selection shows `--`, never 0.

State params stick between pages: use `?state=ok` and `?remote=0` to clear them.
