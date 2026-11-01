# 056 — Desktop studio style: tokens, compact density, buttons, sidebar and status bar

| | |
|---|---|
| Epic | E11 Desktop layout (first slice of E9 Console UI for the desktop layout) |
| Depends on | 055 |
| ABI / showfile | none |

**Touches:** new `crates/ui_egui/src/app/ui_tokens.rs`, `crates/ui_egui/src/app/ui.rs` (style install, fonts),
`crates/assets` (IBM Plex fonts), `crates/ui_egui/src/app/components/{button.rs,button_new.rs,clickable.rs,switch.rs,
tree.rs,desktop_nav.rs,navbar.rs}`, new `crates/ui_egui/src/app/components/status_bar.rs`, page toolbars under
`crates/ui_egui/src/app/pages/`

## Problem
The maintainer, verbatim (2026-11-01):

> okay make the desktop version look less like a toy and a bit more like studio software. pay close attention to
> how buttons look and the navbar feels like, compare to other studio software and ship

After 055 the desktop layout has a sidebar, but every widget still uses the kiosk's touch look: 32 px filled tiles with
drop shadows, stretched equal-width toolbar buttons with cut labels, 28 px nav rows, Ubuntu Light at kiosk sizes and
no persistent status.

## Scope
- One tokens module (`app/ui_tokens.rs`) with the Console vocabulary (`bg`, `surface`, `line`, `muted`, `accent`,
  `selection`, ...) and `Density { Touch, Compact }`; the desktop layout installs a Compact egui `Style` (text
  styles, spacing, widget visuals, radius, strokes, selection, scroll bars) and IBM Plex Sans Condensed / Mono.
- Shared buttons (`components::button`, `action_button`, `Button`, `clickable`, `Switch`) render in a studio style on
  Compact: content-sized, flat with a hairline border, accent "on" state, primary / danger variants, no truncation.
- Desktop sidebar restyle: compact rows, muted icons that turn accent when active, 2px accent bar, section headers,
  collapsible to an icon rail. Visualizer gets its own icon.
- A slim desktop status bar with live state (show, unsaved, BPM / beat, loop time, connection).
- A pass over the main desktop pages' toolbars.
- Mobile (kiosk, Touch) stays pixel-identical apart from the Visualizer icon.

## Out of scope
- Mobile / kiosk restyle (032–035 at Touch density), PageHeader with "More" overflow (035), ListRow row actions and
  overlays (033, 034), page re-layouts (036–038), external screen dock chrome.
- Engine behaviour, `PROTOCOL_VERSION`, `PLUGIN_ABI_VERSION` (19), crates/shared enums.

## Acceptance criteria
- [ ] Desktop uses the tokens module, Compact density and the Plex fonts; mobile is unchanged (before/after shots).
- [ ] Buttons on desktop are content-sized, flat with hairline borders, have hover / pressed / on states and never
      truncate their labels.
- [ ] The sidebar has compact rows, accent active state with a 2px bar, section headers, and folds to an icon rail;
      every page and sub-page stays reachable.
- [ ] A status bar shows show name / unsaved state, BPM and beat, loop time and the connection.

## Verification
- In the blenv container: `cargo clippy --workspace --all-targets --all-features` (no new warnings), `cargo fmt`,
  `cargo test --workspace --all-features`.
- `crates/web/build.sh --lint` if disk allows.
- blenv screenshots, desktop and mobile, before and after.

## Notes
<!-- Implementation notes, decisions, and follow-ups found while working. Keep this updated before renaming to .done.md. -->
