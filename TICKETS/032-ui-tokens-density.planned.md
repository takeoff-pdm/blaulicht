# 032 — UI tokens, density and theme foundation

| | |
|---|---|
| Epic | E9 Console UI |
| Depends on | none |
| ABI / showfile | none |

**Touches:** new: `crates/ui_egui/src/app/ui_tokens.rs` (or `app/tokens/`), `crates/ui_egui/src/app/theme.rs`, `crates/ui_egui/src/app/ui.rs` (style install, fonts), `crates/assets` (font files), `crates/ui_egui/src/app/page.rs` (`PageRenderContext`), `crates/ui_egui/src/app/external_screen.rs` (pane render mode)

## Problem
The app has no shared sizes or type scale. `theme.rs` only sets colours; there are 237 `vec2(…)` and 235 colour literals in `app/`, and `interact_size = 44×36` is copied into 7 files. Desktop mode renders the kiosk layout into a fixed 723 × 480 canvas. The maintainer chose the "Console" direction with the navy palette (2026-10-30); its tokens and rules are in the design system **Blaulicht Console** (https://claude.ai/artifact/ChF4HkbnuyT8bycizagoja): read its `README.md` and `tokens.json`.

## Scope
- Add a tokens module with every colour, spacing, radius, stroke and size token from the design system's `tokens.json`, named as there.
- Add `Density { Touch, Compact }` with methods for row height, header height, rail width, text styles (the `touch-*` / `compact-*` styles), padding and gaps. The kiosk is always `Touch`; desktop defaults to `Compact`, with a per-pane override stored like `render_mode`.
- Make density reachable from every page (via `PageRenderContext` or egui context data), so no page code checks the mode to pick a size.
- Install `egui::Style` from the tokens at frame start: `text_styles`, `spacing` (`interact_size`, `button_padding`, `item_spacing`), widget visuals (fills, strokes, `corner_radius`), selection and window colours. Replace the `BLUE` theme's `Visuals` with the Console values. Keep the light-mode toggle working as it does today (plain egui light), and note it in Notes.
- Bundle IBM Plex Sans Condensed (400/500/600) and IBM Plex Mono (500) TTFs (OFL) and register them as the proportional and monospace families, keeping `egui_phosphor` and `icons.ttf` as fallbacks.
- Desktop dock panes: lay out at the pane's real size by default (drop the fixed 723 × 480 canvas as the default, keep it selectable if cheap).
- Delete the copied `interact_size` lines and use the density instead.

## Out of scope
- Restyling individual components or pages (033–038). Pages may look half-migrated after this ticket; that is expected.

## Key code
- `set_theme`, `BLUE` — `crates/ui_egui/src/app/theme.rs`
- Font setup and `set_pixels_per_point` — `crates/ui_egui/src/app/ui.rs:43-62`
- `PageRenderContext` — `crates/ui_egui/src/app/page.rs:15-72`
- Pane `render_mode` — `crates/engine/src/config.rs:144`, `crates/ui_egui/src/app/external_screen.rs:415-490`

## Acceptance criteria
- [ ] All design-system tokens exist in code with the same names and values; a unit test checks a few values against the design system (or a copied `tokens.json`).
- [ ] Text renders in IBM Plex Sans Condensed; values can use IBM Plex Mono.
- [ ] Switching a desktop pane between Touch and Compact changes row heights and text sizes in standard egui widgets without page code changes.
- [ ] Desktop panes fill their real size by default.

## Verification
- `cargo clippy --workspace --all-targets --all-features`, `cargo fmt --all --check` and `cargo test --workspace --all-features` are clean (run them in the blenv container).
- Follow the `blaulicht-env` skill: run the app in blenv (`scripts/blenv run` and `run --desktop`), drive it with real clicks, and look at every screenshot yourself. Take before and after shots; keep them in `.blenv/shots/` and never commit them.
- Compare against the design system's component previews and rules (Blaulicht Console, linked under Problem).
- Screenshots of three pages (Programmer, Setup › Groups, System) at 800 × 480 and on desktop in both densities.

## Notes
<!-- Implementation notes, decisions, and follow-ups found while working. Keep this updated before renaming to .done.md. -->
