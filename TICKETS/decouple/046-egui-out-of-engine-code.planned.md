# 046 — Remove egui from engine-side code

| | |
|---|---|
| Epic | E10 Decouple |
| Depends on | none |
| ABI / showfile | none |

**Touches:** `crates/audio_engine/Cargo.toml`, `crates/audio_engine/src/spectrogram.rs`, `crates/core/src/mainloop/bg_worker.rs`, `crates/core/src/plugin/wasm.rs`, `crates/core/src/app/components/*` (spectrogram image helper), `crates/core/src/main.rs`

## Problem
Engine-side code depends on egui:
- `crates/audio_engine/Cargo.toml:40` depends on `egui` unconditionally, and `spectrogram.rs:3`, `:301`, `:443`,
  `:680` produce `ColorImage` / `Color32`.
- `mainloop/bg_worker.rs:9`, `:62–63` uses `egui::ColorImage` and calls
  `crate::app::components::create_spectrogram_image`. It is a debug BMP dump, active only when
  `BLAULICHT_SPECTROGRAM_DIR` is set.
- `plugin/wasm.rs:11` imports `egui::ahash::HashMapExt`.
- `main.rs:17` imports `winit` without using it.

## Scope
- audio_engine: the spectrogram image functions output plain RGBA (`Vec<[u8; 4]>` plus width and height), and
  the egui conversion moves to the UI. Remove the `egui` dependency from audio_engine.
- bg_worker: write the BMP from the plain RGBA, with no egui and no `crate::app` import.
- wasm.rs: use std `HashMap::with_capacity`. main.rs: remove the unused `winit` import.
- `stage_assets.rs` (three-d-asset, image) is used only by the visualizer. Note it for 053, but don't move it here.

## Acceptance criteria
- [ ] `cargo tree -p blaulicht-audio-engine -i egui` finds nothing.
- [ ] Outside `crates/core/src/app/`, nothing in `crates/core/src` imports `egui`, `eframe`, `epaint`,
      `winit` or `crate::app` (grep).
- [ ] The spectrogram on the Audio page looks the same (blenv screenshot before and after).

## Verification
- `cargo clippy --workspace --all-targets --all-features`, `cargo fmt`, `cargo test --workspace --all-features`.

## Notes
