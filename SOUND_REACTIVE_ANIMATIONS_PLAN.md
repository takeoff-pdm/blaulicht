# Sound-Reactive Animation v1

## Summary

Create a coherent sound-reactive system built from editable signal layers and starter presets. Layers react to continuous energy, frequency-band energy, or hybrid onset/clock pulses while preserving authored scene values. Existing phasers remain available for tempo-locked movement.

## Data Model and Runtime

- Add `AnimationSpecBody::AudioModulation(AudioModulationSpec)` with:
  - Signal: overall energy, configurable band energy, or hybrid beat pulse.
  - Shaping: threshold, sensitivity, attack, release, inversion, and per-section multipliers for Breakdown, Drop, and Active Beat.
  - Blend: additive signed depth or scale-at-peak percentage.
- Treat valid signal input as `Active`, or `NoFrame` with `frame_age_ms <= 250`; stale, disconnected, ended, and error states target zero and decay using the configured release.
- Normalize continuous signals to `0..1`. Calculate band energy from the RMS of FFT buckets overlapping the requested frequency bounds, using each bucket’s real bounds rather than assuming uniformly spaced frequencies.
- Trigger hybrid pulses immediately from a new `onset_event_id`. Use a new clock beat only as fallback when no onset covered the expected beat, BPM confidence is at least `0.5`, and audio is fresh. Deduplicate both event IDs.
- Use elapsed-time exponential smoothing so behavior is independent of engine tick rate. Inversion applies only while audio is valid; invalid audio always decays toward zero.
- Stop mutating saved `fixture_states` during animation ticks. Store transient outputs in `DmxEngine` runtime state and resolve each scene property as:
  1. Authored/palette-resolved base.
  2. Existing phaser or compatibility absolute output, preserving ascending animation-ID/last-wins behavior.
  3. `base_or_absolute × product(scale factors) + sum(additive contributions)`.
  4. Property-specific clamping (`0..360` for hue, `0..255` otherwise).
- Apply modulation before normal base/overlay scene merging. Pausing, removing, or retargeting an animation removes its transient output on the next frame.

## Compatibility and Authoring

- Append the new serialized variants without reordering existing variants. Bump the showfile format and plugin ABI versions, then rebuild bundled plugins.
- On load, migrate legacy audio templates and active cloned specs into the new serialized model:
  - Volume, BPM, bass, beat-clock, and spatial-frequency behaviors use hidden compatibility signal/output variants that reproduce their former absolute values.
  - Compatibility variants remain editable only through their applicable legacy fields and are absent from new-animation choices.
  - Saving writes the migrated new representation, never the old `AnimationSpecBody` variants.
- Replace placeholder audio editors with controls for signal source, custom frequency bounds, blend/depth, threshold, sensitivity, attack/release, inversion, and the three section multipliers.
- Show live raw-signal, shaped-envelope, contribution, and audio-health indicators.
- Presets create ordinary single-layer animations with editable values and no persistent preset identity:
  - **Kick Flash:** hybrid pulse → additive alpha, immediate attack, short release.
  - **Bass Pump:** 40–180 Hz energy → scaled alpha.
  - **Energy Lift:** broad-band energy → scaled color value with slower smoothing.
  - **Tempo Sweep:** existing sine phaser → pan, four-beat pinned cycle, evenly stretched across fixtures.
- Newly authored audio layers expose only Add and Scale; absolute replacement remains compatibility-only.

## Test Plan

- Unit-test normalization, threshold/sensitivity mapping, inversion, section multipliers, elapsed-time attack/release, and invalid-audio decay.
- Verify FFT band selection uses real bucket bounds and produces one uniform band-energy signal for the selected fixtures.
- Test onset immediacy, event deduplication, missed-beat fallback, confidence gating, and prevention of onset/clock double firing.
- Verify authored and palette-bound fixture state remains unchanged while animation output changes.
- Test additive summation, scale multiplication, phaser-before-modulation ordering, clamping, selection order, overlays, pause/removal, and audio dropout.
- Load representative legacy JSON for every old audio mode, compare generated outputs against the old behavior, save it again, and assert that only the new representation remains.
- Validate the four preset specifications and run full Rust tests, clippy, formatting checks, and bundled-plugin rebuilds.

## Assumptions

- This version targets the native egui animation workflow; the web dashboard is unchanged.
- Beat-detection and section-classification algorithms are reused rather than retuned.
- Frequency animation becomes uniform band-energy modulation for new effects; spatial spectrum behavior exists only for faithful legacy playback.
- Presets do not introduce grouping or coordinated multi-animation lifecycle.
