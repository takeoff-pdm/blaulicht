# 055 — Desktop / mobile layout setting with a desktop navigation sidebar

| | |
|---|---|
| Epic | E11 Desktop layout |
| Depends on | none (005 sub-page navigation and 054 web frontend are done) |
| ABI / showfile | none (the showfile's `ui.main_screen_desktop_mode` stays readable and writable) |

**Touches:** `crates/ui_egui/src/cli.rs`, new `crates/ui_egui/src/layout.rs`, `crates/ui_egui/src/desk.rs`,
`crates/ui_egui/src/app/{mod.rs,ui.rs,page.rs,app_navbar.rs,external_screen.rs}`,
`crates/ui_egui/src/app/components/navbar.rs`, new `crates/ui_egui/src/app/components/desktop_nav.rs`,
`crates/ui_egui/src/app/pages/{setup/mod.rs,system/misc.rs,system/file_dialog.rs}`, `crates/web/src/lib.rs`

## Problem
The maintainer, verbatim (2026-11-01):

> lets begin modifying the egui ui so that we can set if it is a desktop or mobile ui. on desktop, the left side of
> the viewport should get a proper, egui-style navigation bar and the sub-pages (like `General` `Dmx`, etc) are
> folded into the left as sub-pages

Today the UI has a bare `desktop_mode: bool` (`-d/--desktop-mode`). In desktop mode the main window is an
external-screen dock (`main_screen_desktop_mode`, marked `// TODO: this is completely broken.`); the kiosk layout
has a 45 px icon rail and every page with sub-pages draws its own tab bar along the bottom.

## Scope
- A layout setting `UiLayout { Mobile, Desktop }` instead of the bool. Mobile is today's kiosk/touch layout,
  unchanged.
  - Native: `--layout desktop|mobile`; `-d/--desktop-mode` stays, with `--desktop` as another alias.
  - Web: `?layout=desktop|mobile` in the page URL, with a documented default from the browser viewport.
  - Everything that keyed off `desktop_mode` (resizable window, file dialog style) keeps working.
- Desktop: a resizable `Panel::left` navigation sidebar in egui style: icon + label per page, the current page or
  sub-page highlighted, System and Setup with their sub-pages folded underneath (collapsible, the current page's
  section opens by itself). Clicking a sub-page goes through `Navbar::navigate_to_subpage`; clicking a page with
  sub-pages opens its selected or default sub-page. The startup status indicator stays in the sidebar.
- Desktop content: the page fills the `CentralPanel`; pages with sub-pages don't draw their tab bar, switched
  through `PageRenderContext`.
- The dock-based desktop main screen is replaced for the main window. `main_screen_desktop_mode` keeps loading and
  saving (serde compatibility). External screens (`-e`, docks, `PageRenderMode`) are unchanged.
- Unit tests for the sidebar navigation logic and the layout parsing (CLI and URL query).

## Out of scope
- Restyling the mobile nav rail and tab bar (035).
- A runtime layout switch inside the UI (a setting in System › General); the layout is chosen at startup.
- Engine behaviour, the wire protocol (`PROTOCOL_VERSION`) and the plugin ABI (`PLUGIN_ABI_VERSION` 19).

## Key code
- `Navbar` — `crates/ui_egui/src/app/components/navbar.rs`
- `subpage_ui` / `PageRenderContext` — `crates/ui_egui/src/app/page.rs`
- Main screen switch — `crates/ui_egui/src/app/ui.rs` (`match self.desktop_mode`)
- `UiArgs` — `crates/ui_egui/src/cli.rs`; window — `crates/ui_egui/src/desk.rs`
- Web entry — `crates/web/src/lib.rs`; zoom to fit — `fit_web_zoom` in `app/ui.rs`

## Acceptance criteria
- [ ] `--layout desktop`, `-d`, `--desktop-mode` and `--desktop` start the desktop layout; no flag or
      `--layout mobile` the mobile one.
- [ ] `?layout=desktop|mobile` picks the web layout; without it the documented default applies.
- [ ] Desktop: sidebar with every navbar page, System and Setup sub-pages folded and collapsible, current entry
      highlighted, resizable within limits, startup status slot; no horizontal sub-page tab bar.
- [ ] Mobile looks exactly as before (before/after screenshots).
- [ ] `NavigatePage` / `NavigateSubPage` events still navigate in both layouts.
- [ ] Older showfiles with `ui.main_screen_desktop_mode` still load, and save it back.
- [ ] Unit tests for the sidebar navigation and layout parsing; existing navbar tests pass.

## Verification
- In the blenv container: `cargo clippy --workspace --all-targets --all-features` (no new warnings),
  `cargo fmt`, `cargo test --workspace --all-features`.
- `crates/web/build.sh --lint` compiles the wasm build.
- blenv screenshots: desktop (`run --desktop`) with System › DMX, Setup › Palettes, Programmer, a collapsed and an
  expanded section, a resized sidebar; mobile before/after.

## Notes
<!-- Implementation notes, decisions, and follow-ups found while working. Keep this updated before renaming to .done.md. -->
