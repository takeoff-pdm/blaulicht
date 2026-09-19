# WasmLED

Ten BPM-paced animations for five independent 99-pixel tubes arranged around a room. Select exactly **495 RGB-capable pixel fixtures**, ordered as tube 1 pixels 0–98, then tube 2, through tube 5. Reverse individual tubes in the controls to correct wiring direction. Other fixture counts produce no writes and show setup guidance.

| Effect | Character | Base timing at 1× |
|---|---|---|
| Aurora | Teal, violet and green curtains | 32 beats |
| Ocean Drift | Layered blue waves | 16 beats |
| Ember Breath | Amber breathing and ember pockets | 8 beats |
| Stardust | Soft, sparse pastel stars | Spawn opportunities every 2 beats |
| Liquid Plasma | Interfering liquid color waves | 8 beats |
| Double Helix | Contrasting crossing trails | 4 beats |
| Prism Flow | Stretching spectral ribbons | 8 beats |
| Meteor Rush | Comets and long tails | 2 beats per traversal |
| Acid Chase | Lime/magenta segmented runners | Half-beat steps |
| Glitch Rave | Fragmented flashes and dark gaps | Quarter-beat updates |

Timing describes the main motion; secondary waves and arm offsets keep patterns from repeating mechanically. Each arm has its own phase or deterministic seed. The effects are original procedural implementations, inspired by the families in the [WLED effects catalog](https://kno.wled.ge/features/effects/).

## Controls and timing

Select an effect, adjust speed (0.25×–4×, logarithmic), brightness, hue shift, and each tube's direction. Controls persist per animation instance in showfile state. Default: Aurora, 1×, full plugin brightness, no hue shift, no reversed tubes. Effect changes crossfade from the current frame over 400 ms; pause freezes progression and emits no writes.

Only the host BPM field is read. Missing, non-finite, or non-positive BPM uses 120 BPM, indicated in the UI. BPM changes preserve phase. Both host speed factors multiply the local speed. There is no audio analysis, beat-trigger response, or downbeat alignment. Hardware brightness remains controlled by the existing fixture/output pipeline.

## Build and load

From this directory:

```sh
cargo build --release --target wasm32-unknown-unknown
```

Artifact: `target/wasm32-unknown-unknown/release/wasm_led_animation_plugin.wasm` (unless `CARGO_TARGET_DIR` overrides it). The parent plugin Makefile also discovers this crate and bundles it as `wasm_led_animation_plugin.wasm`.

Load that file through the host's existing plugin workflow, then select **WasmLED** as an animation and select the 495 pixel fixtures. The stable descriptor is `org.blaulicht.wasm-led`. Existing showfiles and plugin registrations are not modified by building this crate.

## Tests and visual preview

```sh
cargo test
cargo run --example preview > target/preview.html
```

Open `target/preview.html` in a browser. It contains recorded frames from the real Rust renderer for all ten effects, displayed as five starfish arms. Select an effect, pause, or change playback speed. Clips are 12-second excerpts at 120 BPM and 15 fps; the clip loop is not a claim that each effect repeats after 12 seconds. The plugin itself renders at the host's tick rate.

Tests cover deterministic and distinct effect output, bounds, motion, mapping, reversal, tempo fallback and continuity, pause, speed factors, crossfades, persistence, and instance isolation. The native renderer accepts only BPM, elapsed time, and speed; it has no audio or beat-event input.
