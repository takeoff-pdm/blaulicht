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
- [x] Desktop uses the tokens module, Compact density and the Plex fonts; mobile is unchanged (before/after shots).
- [x] Buttons on desktop are content-sized, flat with hairline borders, have hover / pressed / on states and never
      truncate their labels.
- [x] The sidebar has compact rows, accent active state with a 2px bar, section headers, and folds to an icon rail;
      every page and sub-page stays reachable.
- [x] A status bar shows show name / unsaved state, BPM and beat, loop time and the connection.

## Verification
- In the blenv container: `cargo clippy --workspace --all-targets --all-features` (no new warnings), `cargo fmt`,
  `cargo test --workspace --all-features`.
- `crates/web/build.sh --lint` if disk allows.
- blenv screenshots, desktop and mobile, before and after.

## Notes
<!-- Implementation notes, decisions, and follow-ups found while working. Keep this updated before renaming to .done.md. -->

Done in one part. Commits: `feat: Console tokens, compact density and IBM Plex for the desktop layout`,
`feat: studio-style buttons, list rows and dialogs at compact density`, `feat: studio-style desktop sidebar with section
groups and an icon rail`, `feat: desktop status bar with show, tempo, engine and connection state`, `feat: rail tooltips
to the right of the icon`.

### Studio traits adopted (and where they come from)
Compared against Ableton Live, Bitwig, DaVinci Resolve, Resolume Arena, grandMA3 onPC, ChamSys MagicQ, QLab and
Reaper:
- **Dense but legible:** 24 px controls, 26 px nav rows, 13 px body / 12.5 px button text in a condensed face
  (IBM Plex Sans Condensed); Resolve, Bitwig and MA3 all sit at 22–26 px rows.
- **Flat dark surfaces with small elevation steps** instead of drop shadows: `bg` (page) < `surface` (sidebar, status
  bar, dialogs) < `surface_raised` (controls) < `surface_hover` < `surface_pressed`; 1px hairlines (`line`) separate
  panels and list rows (Resolve, Bitwig, QLab).
- **Small radius:** 3 px on controls, 4 px on windows.
- **Buttons:** flat with a hairline border, sized to their label, never stretched equal-width tiles and never
  truncated; hover brightens fill and border, pressed darkens, focus gets an accent ring. One accent colour is
  reserved for active / armed / selected (Ableton's and Bitwig's single highlight colour).
- **Toggles look lit when on** (`ButtonVariant::On`: accent-tinted fill, accent border, bright label), like
  Resolume's and MA3's lit keys; `Switch` is a small pill that fills with the accent.
- **Primary / danger distinction** only where the code distinguishes it: confirm / OK / Save are Primary (filled
  accent), Delete and Shutdown are Danger (red label and border), row actions Quiet.
- **Values in monospace / tabular figures:** status bar numbers and the log in IBM Plex Mono.
- **Muted secondary text** (`muted`) for captions, details and idle icons; **section headers** as small uppercase
  letter-spaced labels ("PERFORM", "GROUPS"), as in Bitwig's inspector and Resolve's panels.
- **Navigation like a browser / tool rail:** muted icons, the active entry with `selection` fill, accent icon and a
  2px accent bar (Resolve's page bar, VS Code-like rails), collapsible to an icon rail with names on hover.
- **Persistent status bar** (Reaper, Bitwig, QLab): show and saved state, BPM and beat, loop / plugin time, LIVE,
  connection.

### What changed
- `app/ui_tokens.rs`: one module for every desktop value. `Palette` with `DARK` (Console navy) and `LIGHT`, named
  `bg`, `surface`, `surface_raised/hover/pressed/sunken`, `line`, `line_strong/hover`, `text`, `text_strong`, `muted`,
  `faint`, `accent`, `accent_hover`, `selection`, `on_fill/on_fill_hover/on_text`, `primary_*`, `danger`, `warn`,
  `ok`; `RADIUS` 3, `RADIUS_WINDOW` 4, `HAIRLINE` 1, `ACCENT_BAR` 2. `Density { Touch, Compact }` (control / row
  heights, interact size, text sizes, `font(weight, size)`, `overline`), stored in egui context data
  (`Density::of(ctx)`, default Touch). `desktop_style(theme)` builds the Compact egui `Style` (text styles, spacing,
  widget visuals for all states, selection, window / panel fills, shadows, thin floating scroll bars) for both
  themes; `install_desktop` installs it, the fonts and Compact once at startup.
- Fonts: IBM Plex Sans Condensed 400/500/600 and IBM Plex Mono 500 from google/fonts (`ofl/ibmplexsanscondensed`,
  `ofl/ibmplexmono`) in `crates/assets/fonts/plex/` with `OFL.txt`; `blaulicht_assets::add_plex_fonts` puts them
  first in the proportional / monospace families (egui's fonts, `egui_phosphor` and `icons.ttf` stay as fallbacks)
  and adds the `plex-medium` / `plex-semibold` families. Desktop only; they add ~470 KB to binaries and the wasm.
- Buttons (`components/button.rs`): at Compact, `button`, `action_button`, `Button`, `clickable` render through
  `studio_button` / `studio_clickable`. `ButtonVariant { Tonal, On, Primary, Danger, Quiet }`; `variant_button`,
  `button_as(ui, active, variant, ..)` and `action_button_as` keep a call site's kiosk look (its `active` flag) while
  picking the desktop variant. `ButtonSize::with_touch_width` marks widths that only line up kiosk rows (Programmer
  toolbar, System buttons, dialog Close / Cancel / Undo all, preset category filters, folder picker Close): on the
  desktop those size to the label. Presets size to the label; a custom width is a minimum that grows to fit.
- Other components at Compact: `Switch` (pill toggle), `Dialog` frame (`surface`, hairline, window radius, shadow),
  `Tree` (flat single-line rows with hairlines, folder icons, the detail right-aligned in `muted` on every row or on
  none, selection fill + 2px accent bar, "Move…" as a Quiet button on the hovered or selected row),
  `column_header` (overline instead of "Groups:"), and the 7 copied 44x36 `interact_size` lines read
  `Density::interact_size()` (unchanged 44x36 on Touch).
- Sidebar (`components/desktop_nav.rs`): header with logo mark and name and a fold button; groups PERFORM
  (Programmer, View Performance, Views, Scene Graph, Visualizer), SETUP (Setup, Audio), SYSTEM (System, Logs), pages
  missing from the table go under "More" (unit test); 26 px rows, muted icons that turn accent, `selection` fill and
  2px bar on the current entry, the parent's icon stays accent while its sub-page is current, hover fill, fold
  arrows on the right, an indent guide under the sub-pages; auto-expand and the startup status row unchanged. Folded
  to a 44 px icon rail (`SidebarClick::ToggleRail`, `Navbar::is_rail`; separate panel id so the dragged width
  survives) with the page name to the right of the hovered icon. Visualizer's icon is `CUBE_FOCUS` (was Audio's
  `WAVEFORM`), in both layouts: the only mobile change.
- Status bar (`components/status_bar.rs`, `BlaulichtApp::show_desktop_status_bar`): showfile name, Saved / Unsaved
  changes / Saving… / Save failed (error on hover; replaces the floating save dot on the desktop), BPM and four beat
  squares (downbeat flash in `warn`), loop and plugin times (loop in `warn` from 20 ms), a LIVE pill while any
  session previews, and "Local engine" or the remote connection state. Only reads the client's existing state.
- Pages: Programmer toolbar content-sized (LIVE dot as a corner badge), Scene Changes buttons content-sized, System
  › General buttons content-sized with Shutdown as Danger, Primary confirm / OK / Save / Create / Add and Danger
  Delete in Setup dialogs and toolbars, the lifecycle guard's Save Primary.

### Decisions
- The density is per egui context, i.e. per process: the desk is either kiosk or desktop. Desktop external-screen
  windows share the context, so their pages also render Compact; their dock chrome (tab bars, pane frames) is
  unchanged.
- The desktop skips the per-frame `set_theme(BLUE)`; its styles are installed once for both themes, so the LIGHT /
  DARK toggle switches between `LIGHT` and `DARK` palettes (checked: light Setup › Groups and System › General look
  right). The kiosk keeps BLUE and plain egui light.
- Kiosk look preserved by construction: every Compact path is behind `Density::of(ctx).is_compact()`, Touch values
  are the old constants (unit test), and converted call sites pass their old `active` flag.

### Delivered from 032 / 033 / 035 (desktop only) and what is left
- 032: tokens module, `Density { Touch, Compact }`, Compact egui style install, Plex fonts, the copied
  `interact_size` lines. Left: aligning names / values with the design system's `tokens.json` (the artifact was not
  reachable from this session, so the values are my navy take on the vocabulary) plus the unit test against it;
  Touch-density restyle of the kiosk; per-pane density override and real-size dock panes.
- 033: Button variants (Tonal / On / Primary / Danger / Quiet, disabled with reason, sized from density) and flat
  ListRow-style tree rows with hairlines, selection fill, accent bar and hover action; column header as overline.
  Left: the kiosk side, chips, Status pill / dots as a component (the status bar draws its own), "⋯" row menu instead
  of "Move…", removing `ButtonSize` widths.
- 035: desktop nav (muted icons, accent active with 2px bar, status slot). Left: PageHeader with prioritised actions
  overflowing into More (desktop toolbars wrap or fit today but don't overflow), kiosk nav rail and tab bar restyle,
  dock pane chrome.
- 034, 036–038 untouched: page re-layouts (Programmer faders / knobs and Scene Changes list, health cards on System ›
  General, Plugins page's hard-coded greys, pagination lists on Animations / Views, Audio graphs), abbreviations
  ("UnLmt", "Lmt All", "Clone Sc.", "Scene +"), sheets / menus / toasts.

### Verification
- blenv: `cargo test --workspace --all-features` all green (ui-egui 128 tests, 8 new: 3 tokens / density, 3 sidebar
  (groups cover every page, fold to the rail and open a page from it, section fold arrow), 2 navbar (rail toggle,
  every page has its own icon)).
- Clippy (blenv, `--workspace --all-targets --all-features`): engine 39 / 42, ui-egui 88 / 103 (baseline 89 / 104:
  one float-literal warning in `dialog.rs` moved into a helper that types it), binaries and blaulicht-web 0. No
  warning in the new code. Formatted with `rustfmt --config skip_children=true` on the touched files only; the one
  unrelated `ui.rs` hunk was reverted.
- `crates/web/build.sh --lint` (host): the wasm bundle builds (7.1 MB raw, 2.9 MB gz). The host has no clippy, so
  its lint step finds nothing to report rather than running.
- Screenshots (blenv, Kuze copy, all looked at): desktop before / after of Programmer, Setup › Groups, Setup ›
  Palettes, System › General, Logs, View Performance; after only of Fixtures, Scenes, Animations, DMX, Art-Net,
  Plugins, Screens, Audio, Views, Scene Graph, Visualizer; a selected and a hovered group row with "Move…"; the
  Programmer's Active Animations dialog; LIVE on (lit button, corner badge, status bar pill; switched off again,
  `live_mode: false`); nav hover; the icon rail with its tooltip; toolbar button hover; light theme.
- Mobile before / after (`compare -metric AE`): Programmer, Setup › Groups, Setup › Palettes and View Performance
  differ in 117 px, all in the Visualizer icon's 13x12 box in the rail; System › General additionally in the live
  loop / plugin graphs, Logs in timestamps and the wasm cache dir name.

### Follow-ups
- Persist the rail state and sidebar width (today per session); a flyout for System / Setup sub-pages from the rail.
- Status bar actions (click the save state to save, the BPM to open Audio) once they are wanted; keep it read-only
  until then.
- External-screen docks: Compact pane chrome (035).
