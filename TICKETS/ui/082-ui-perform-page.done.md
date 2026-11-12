# 082 — Perform page bar and horizontal fader

| | |
|---|---|
| Epic | E13 UI port |
| Depends on | 069 |
| ABI / showfile | none expected; check while implementing |

**Touches:** `app/pages/view_perf.rs`, `app/components/fader.rs` (horizontal)

## Problem
Perform's bar was cramped (`pages.html#performance`). Mockup: `design/studio`; gaps listed in `design/studio/UI_GAP.md`.

## Scope
- Page bar like the Programmer's: Add overlay, Grand master horizontal fader with value and a reset icon (greyed at 100 %), Blackout and Exclusive preview right.
- Horizontal fader: the vertical fader's cap, slot and ticks on its side, ticks above and below; used everywhere a horizontal fader appears (Audio too).

## Out of scope
- Nothing beyond the scope.
- The kiosk layout (`Density::Compact`) stays unchanged.

## Acceptance criteria
- [x] Matches the mockup at the same window size, both themes.
- [x] Grand master reset, Blackout and restore work from the bar.

## Verification
- `cargo clippy --all-targets --all-features`, `cargo fmt`, `cargo test --all-features`.
- UI: screenshots per `crates/core/AGENTS.md` next to the mockup: Perform bar, Audio faders.

## Notes
- Desktop only (`layout.is_desktop()` / `Density::Compact`). The kiosk (`Density::Touch`) keeps its old toolbar, separator, live label and `HFader` drawing: every change is behind that branch. Checked with a kt shot at 800x480 (old toolbar, unchanged); not diffed pixel by pixel against a decouple build.
- Bar: `studio::page_bar` with Add overlay (disabled when no scene is left), a 1px separator, muted "Grand master", a 200x28 horizontal fader with a 22px cap and "N%" value, a 28px reset icon (greyed at 100 %), and Blackout / Exclusive preview on the right (Programmer's `blackout_button` and `exclusive_preview_button`, now `pub(crate)`). Blackout goes through `toggle_blackout`, so it restores the previous level. The live-mode red label below the bar is gone on desktop; Exclusive preview lights up and the status bar shows LIVE.
- While dragging, the Grand master fader keeps its own value in temp memory. Without that, each frame's drag delta was applied to the engine's stale echo and a 100 px drag moved only ~6 %.
- Horizontal fader: `studio::hfader_with` + `HFaderStyle` (track size, cap width, value column, suffix, name on/off); `studio::hfader` uses it. Ticks sit above and below the slot, 7px clear of it. On desktop `components::HFader` delegates to it (200x32 track, 36px right-aligned value, name), so Audio, the animation spec editor, Palettes, the fixture setup dialogs and plugin UIs all get it. Only Audio was checked visually.
- Verified with kt (tools/blenv/KITTEST.md) at 1500x950 against `pages.html#performance` and `#audio`: drag, reset, Blackout -> Restore, Exclusive preview on/off, Add overlay. Light theme checked in the app only; the headless mockup shot can't switch the theme.
- Checks on the host with CARGO_BUILD_JOBS=4: `rustfmt` (stable toolchain, the nightly override has no rustfmt) on the changed files, `cargo clippy -p blaulicht-ui-egui --all-targets --all-features` (no warnings in the changed files), `cargo test -p blaulicht-ui-egui --all-features` (164 passed). Did not run them workspace-wide.
- Out of scope, still old: the overlay cards (mockup: "Overlays" pane, knob strips with Reset/Off/Remove) and the rest of the Audio page (pane headers, spectrogram).
