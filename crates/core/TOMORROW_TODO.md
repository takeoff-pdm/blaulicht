# MIDI mapping follow-up

Context recovered from Claude session `75f561e0-aada-4094-8b56-00933b2e16f4`.

## Implemented so far

- APC mini: Shift+press auto-hold for hold-overlay mappings, with flashing pads. Shift handling was subsequently revised after hardware feedback.
- Removed the hardcoded scene-intensity column so its pads can be assigned.
- Added separate mapping buttons for APC mini, MIDI Mix, and nanoKONTROL.
- Added master alpha/speed mappings targeting multiple scenes per knob or fader.
- Added hold-alpha button mappings that restore previous scene alpha values on release.
- Changes are in `crates/plugins/midi_all`. Claude reported rebuilding the WASM; the recovery session confirmed `cargo clippy --lib` passes with warnings.

## Remaining verification

- [ ] Check device-specific learning for all three controllers, including ignoring other devices during capture.
- [ ] Verify knobs and faders update master alpha/speed for every selected scene.
- [ ] Verify hold-alpha buttons apply the configured value on press and restore previous values on release, including MIDI Mix hardware-style events.
- [ ] Check the APC and nanoKONTROL twins' hold-alpha toggle behavior.
- [ ] Verify existing mappings load, new mappings persist, and older single-scene alpha/speed mappings remain compatible.
- [ ] Visually inspect the three mapping buttons, device-specific learn prompts, scene selection, and alpha slider using the repository's headless UI verification procedure.
- [ ] Confirm APC Shift auto-hold and the newly assignable scene-intensity pads still work.
- [ ] Record verification results and any remaining hardware limitations.

## Coordination

Another Codex session was already running the fake MIDI device and headless app when this context was recovered. This recovery session made no source changes and left that runtime untouched. Check that session's results before repeating verification or stopping its processes.
