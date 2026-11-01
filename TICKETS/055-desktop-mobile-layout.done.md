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
- [x] `--layout desktop`, `-d`, `--desktop-mode` and `--desktop` start the desktop layout; no flag or
      `--layout mobile` the mobile one.
- [x] `?layout=desktop|mobile` picks the web layout; without it the documented default applies.
- [x] Desktop: sidebar with every navbar page, System and Setup sub-pages folded and collapsible, current entry
      highlighted, resizable within limits, startup status slot; no horizontal sub-page tab bar.
- [x] Mobile looks exactly as before (before/after screenshots).
- [x] `NavigatePage` / `NavigateSubPage` events still navigate in both layouts.
- [x] Older showfiles with `ui.main_screen_desktop_mode` still load, and save it back.
- [x] Unit tests for the sidebar navigation and layout parsing; existing navbar tests pass.

## Verification
- In the blenv container: `cargo clippy --workspace --all-targets --all-features` (no new warnings),
  `cargo fmt`, `cargo test --workspace --all-features`.
- `crates/web/build.sh --lint` compiles the wasm build.
- blenv screenshots: desktop (`run --desktop`) with System › DMX, Setup › Palettes, Programmer, a collapsed and an
  expanded section, a resized sidebar; mobile before/after.

## Notes
<!-- Implementation notes, decisions, and follow-ups found while working. Keep this updated before renaming to .done.md. -->

Done in one part. Commits: `feat: UiLayout setting (mobile / desktop) for the egui UI`, `feat: desktop layout with a
navigation sidebar`, `docs: layout flags in AGENTS.md, desktop dock notes in the TODO inbox`.

### The setting
- `UiLayout { Mobile, Desktop }` in `crates/ui_egui/src/layout.rs` (`clap::ValueEnum`, `FromStr`, `from_query`,
  `web_default`). `BlaulichtApp::layout` replaces `desktop_mode`.
- Native: `--layout desktop|mobile`. `-d` / `--desktop-mode` stay, `--desktop` is a visible alias; an explicit
  `--layout` wins over `-d` (`UiArgs::layout()`). blenv still passes `--desktop-mode`.
- Desktop window: 1280x800, min 640x400, resizable (mobile: 800x480, not resizable, as before). The file dialogs
  keep keying off the layout (desktop: native-style window, mobile: popup).
- Web: `?layout=desktop|mobile` (`crates/web/src/lib.rs`, `page_layout`). Default without the parameter: desktop
  if the window is at least 1024 CSS px wide **and** `(pointer: coarse)` does not match, else mobile. So laptops and
  desktop browsers get the sidebar, phones and tablets (even a landscape iPad, 1024-1366 px wide) the touch layout.
  Chosen once at load; reload to switch. The desktop layout keeps the browser's scale; only mobile zooms the
  800x480 layout to the window (`fit_web_zoom`). Needs web-sys feature `MediaQueryList`.

### Desktop sidebar (`app/components/desktop_nav.rs`)
- `egui::Panel::left("desktop_nav")`, resizable 150-320 px (default 200), `show_inside` the root `Ui` (the
  non-deprecated egui 0.34 API; the mobile path keeps its `ctx` panels unchanged). Header "Blaulicht", then one
  row per `AppPage::navbar_pages()` with the phosphor icon and `app_page_label` (new, in `navbar.rs`; the rail's
  `short()` names like "Sys" / "V. Perf" are too terse). Rows are custom full-width selectable rows painted with
  `interact_selectable` visuals (selection fill, hover fill), labels truncated with "…" when narrow.
- System and Setup are `CollapsingState` sections: egui's fold arrow, sub-pages indented with egui's indent line,
  labels in line with the page names. The current page's header is highlighted while its section is folded,
  otherwise the sub-page row is.
- The `Navbar` owns the folding state (`expanded`), egui's state only animates. `Navbar::sync_sections` unfolds the
  current page's section once per page change, whatever changed the page (sidebar, `NavigatePage` /
  `NavigateSubPage` events); a section folded by hand stays folded until the page changes again.
- Clicks come back as `SidebarClick` and are applied by `Navbar::sidebar_click` (page: `navigate_to`, i.e. its
  selected or default sub-page; sub-page: `navigate_to_subpage`; fold arrow; status: Logs). It returns a
  `NavChange`, and `show_desktop_sidebar` tells plugins with `NavigatePage` (on page changes) and `NavigateSubPage`
  (on sub-page clicks), as `Web` like the rail and the tab bar.
- Startup status (failed / restarting / busy) is a full-width row at the bottom (`Panel::bottom` inside the
  sidebar), same icons and colors as the rail slot (`status_style`, shared), summary on hover, click opens Logs.
- Rows use fixed ids and the list and each entry a global id scope: with auto ids egui warned "Widget rect changed
  id between passes" for the scroll area and the rows in the first frame (two passes; auto ids under the root `Ui`
  differ between them). Clean after the change.
- Hidden or stubbed pages behave as before: the sidebar lists the same pages as the rail, the cfg'd-out visualizer
  shows its notice, remote-only gating stays inside the pages.

### Content area
- `PageRenderContext` got a `subpage_tabs` flag (`without_subpage_tabs`, `shows_subpage_tabs`) and
  `for_main_screen(layout, size)`: mobile = `Default` with tabs (unchanged), desktop = `Dynamic` without tabs.
  `subpage_ui` takes the render context and, without tabs, hands the whole `Ui` to the page content. External screen
  docks build their context with `new` and keep their tab bars.
- Desktop uses `Dynamic` rendering because the page fills a window of any size (as a "Dynamic" dock tile does).
  Checked: Logs, System General / DMX, Setup Fixtures / Groups / Palettes, Programmer, Visualizer.

### The old dock-based desktop main screen
- Removed for the main window: `match self.desktop_mode` in `ui.rs` is gone, both layouts go through
  `render_main_screen` (navigation + `CentralPanel` + plugin windows + external screens). Plugin UIs, which were
  dock tabs on the old desktop main screen, open as windows (System › Plugins / `SetPluginUIOpen`) like on mobile.
- Side effect: `-e W,H` external screens now open in the desktop layout too (they never did before, Inbox note from
  053), and `NavigatePage` / `NavigateSubPage` work there (Inbox note from 005, for the main window).
- `BlaulichtApp::main_screen_desktop_mode: ExternalScreen` became `legacy_main_screen_dock:
  Option<ShowfileExternalScreen>`: the showfile's `ui.main_screen_desktop_mode` is kept as loaded and written back
  as is (not interpreted, no dock state), and cleared when another show loads. A show without it no longer gets a
  default dock written (the field is `skip_serializing_if = "Option::is_none"`; older builds fall back to their
  default dock). Verified: Kuze (which has one) loaded, Save As wrote it back; the JSON differs only by serde's
  normalisation (`FixturesSetup` -> `Setup`, `FixturesPerformance` -> `Programmer`, an explicit
  `"render_mode": "Default"` on a plugin tab), the same as before.
- External screens (`-e` windows, their docks, `PageRenderMode`, context menu) are unchanged; checked `-e 900,600`
  in the desktop layout: the dock opens, its System pane still has the tab bar.

### Verification
- Tests (blenv): `cargo test --workspace --all-features` all green (ui-egui 120 tests, 13 new: 5 navbar sidebar
  logic tests, 2 headless egui click tests on real sidebar rows (page, sub-page, status slot), 2 page render
  context tests, 1 CLI flags test, 3 layout parsing / query / web default tests).
- Clippy (blenv, `--workspace --all-targets --all-features`): engine 39 lib / 42 lib test, ui-egui 89 / 104,
  binaries and blaulicht-web 0: the baseline, no new warnings. `cargo fmt` applied to my files only (unrelated
  hunks reverted).
- `crates/web/build.sh --lint` (host): builds and lints the wasm bundle. No browser check (no chromium in the
  container); the web layout choice is covered by `UiLayout` unit tests only.
- Screenshots (blenv, Kuze copy), all looked at:
  - Desktop: start on Logs with System and Setup folded; System -> General; System › DMX; Setup -> Fixtures
    (default); Setup › Palettes; System folded by its arrow; the current Setup section folded (header highlighted);
    Programmer; `blctl event '{"MainUi":{"NavigateSubPage":"Setup_Groups"}}'` from Programmer (switches and unfolds
    Setup); sidebar dragged to ~270 px and to the 150 px minimum ("View Perf…" truncated); System click reopens the
    remembered DMX tab; Visualizer; a plugin UI window; no horizontal tab bar anywhere in the main window.
  - Mobile: before/after on Logs, System General, System › DMX, Setup › Palettes, Programmer (the last three via
    `NavigateSubPage` / `nav`). ImageMagick `compare`: DMX, Palettes and Programmer pixel-identical; General
    differs only in the live loop / plugin graphs, Logs only in timestamps.
  - The startup status slot could not be triggered in blenv (no failing stage); covered by the headless test.

### Follow-ups
- A runtime layout switch (e.g. System › General) instead of startup-only; it would need the sidebar width and
  window size handled when switching.
- Web: check the desktop layout and the default in a real browser (and on a tablet).
- The sidebar look could adopt the Console design tokens once 032/035 land (035 restyles the mobile rail and
  tab bar).
- Pages under `Dynamic` rendering at narrow desktop widths (< 720 px content, e.g. a 640 px window) stack
  vertically; the main window has no outer scroll area (like mobile), so very long stacked pages may clip.
