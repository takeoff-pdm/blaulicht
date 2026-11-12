# 072 — Typing values in place and keyboard control of faders and knobs

| | |
|---|---|
| Epic | E13 UI port |
| Depends on | 058 |
| ABI / showfile | none expected; check while implementing |

**Touches:** `app/components/fader.rs`, `app/components/knob.rs`, `app/components/speed_knob.rs`

## Problem
In the mockup any fader or knob value can be typed and moved with the keyboard (`chrome.js` `editValue`, `keySteps`). Mockup: `design/studio`; gaps listed in `design/studio/UI_GAP.md`.

## Scope
- Double-click a fader's value or name, or a knob, to type in place: Enter or leaving applies, Esc cancels, clamped, in the control's units (percent, 1/4 or 2x, degrees). No layout shift.
- Click or Tab focuses a fader or knob; focus shows as a blue label, no border.
- Arrow keys move by the smallest unit (1 on faders, 1 % intensity, one speed notch); held keys accelerate after 300 ms up to range/25 per repeat; speeds never accelerate.
- Offline: typing is refused with a not-sent notification (070).

## Out of scope
- Backspace undo (074).
- The kiosk layout (`Density::Compact`) stays unchanged.

## Acceptance criteria
- [ ] Matches the mockup at the same window size, both themes.
- [ ] Typing 300 into a 0-255 fader gives 255.
- [ ] Holding Up on a 0-255 fader reaches 255 in under 4 s; on speed it steps notch by notch.

## Verification
- `cargo clippy --all-targets --all-features`, `cargo fmt`, `cargo test --all-features`.
- UI: screenshots per `crates/core/AGENTS.md` next to the mockup: a fader being typed into, a focused knob.

## Notes
