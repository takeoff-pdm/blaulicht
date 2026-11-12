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
- [x] Matches the mockup at the same window size, both themes.
- [x] Typing 300 into a 0-255 fader gives 255.
- [x] Holding Up on a 0-255 fader reaches 255 in under 4 s; on speed it steps notch by notch.

## Verification
- `cargo clippy --all-targets --all-features`, `cargo fmt`, `cargo test --all-features`.
- UI: screenshots per `crates/core/AGENTS.md` next to the mockup: a fader being typed into, a focused knob.

## Notes
Done on `ui/072`.

- Implemented in the desktop studio components (`app/components/studio.rs` `fader`, `hfader`, `knob`, new
  `speed_knob`) plus `app/components/value_entry.rs` (in-place field, `key_steps`, engine link for the not-sent
  refusal). The kiosk widgets named under Touches (`fader.rs`, `knob.rs`, `speed_knob.rs`) are only drawn by the
  kiosk, so per the scope rule they are left untouched; kiosk Programmer screenshot looks as before.
- Double-click a fader's value/name, a knob, or an hfader opens a field floating over the readout (well background,
  1px blue outline, centred 12px mono), no layout shift. Enter or clicking away applies, Esc cancels; clamped. Units:
  numbers accept `%`, `°`, `x` suffixes; speeds accept `1/4`, `0.25`, `2`, `2x` (nearest notch) and show as `1/4`,
  `1x`, `2x`.
- Click/drag/Tab focuses; focus = blue name, no border. Arrows: 1 unit (0.01 on ranges <= 10), held keys use the
  mockup's `keySteps` (after 300 ms, up to range/25 per repeat); speed notches cap at 1 so never accelerate.
- Offline: double-click raises "<name> not sent: engine <state>" (070) and opens nothing. The desktop status bar
  records the link each frame (`value_entry::set_engine_link`).
- Verified in blenv `--desktop` on the studio showcase (Ctrl+Shift+F12): typing 300 into a 0-255 fader gave 255;
  holding Up (xdotool keydown) took 0 to 255 in 3.8 s; Up on speed moved 1x to 2x; Esc cancelled; Tab moved focus;
  dark and light themes checked.
- Not verified: the offline refusal in the running app (could not take the engine offline in blenv; covered by the
  `typing_is_refused_offline` unit test). No desktop page uses the studio faders/knobs yet (the showcase is the only
  caller; 075 ports the Programmer), so the comparison against `programmer.html` is per component, and the mockup's
  double-click state can't be captured headless. Kiosk compared by eye with a fresh kiosk screenshot, not a
  pixel diff against a decouple build.
- Checks run inside blenv: `cargo fmt` (only the touched files kept; the container's rustfmt reformats unrelated
  files), `cargo clippy -p blaulicht-ui-egui --all-targets --all-features` (no warnings in touched files),
  `cargo test -p blaulicht-ui-egui --all-features` (144 passed). Workspace-wide clippy/test not run.
