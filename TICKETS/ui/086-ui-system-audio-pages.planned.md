# 086 — Plugins, MIDI & Serial, Remote UI and Audio pages

| | |
|---|---|
| Epic | E13 UI port |
| Depends on | 069, 071 |
| ABI / showfile | none expected; check while implementing |

**Touches:** `app/pages/system/{misc,midi,serial,remote}.rs`, `app/pages/audio.rs`

## Problem
The rest of 038: these pages exist in the mockup (`system.html#plugins`, `#devices`, `#remote`, `pages.html#audio`) but not yet in the studio layout. Gaps listed in `design/studio/UI_GAP.md`.

## Scope
- Plugins: list and detail (status, file, reload on change), Show UI / Pop out, embedded plugin interface.
- MIDI & Serial: one table (device, kind, status, last activity, Reconnect).
- Remote UI: address, Access (switch, passcode with Show and Remove, warning without passcode), connected clients.
- Audio: Current input with Change device and Info, spectrogram, Input faders (horizontal, 082) and Auto-calibrate.

## Out of scope
- DMX & Art-Net (062).
- The kiosk layout (`Density::Compact`) stays unchanged.

## Acceptance criteria
- [ ] Each page matches its mockup at the same window size, both themes.
- [ ] No ComboBox left on these pages.

## Verification
- `cargo clippy --all-targets --all-features`, `cargo fmt`, `cargo test --all-features`.
- UI: screenshots per `crates/core/AGENTS.md` next to the mockup of each page.

## Notes
