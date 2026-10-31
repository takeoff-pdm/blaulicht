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
- [x] `cargo tree -p blaulicht-audio-engine -i egui` finds nothing.
- [x] Outside `crates/core/src/app/`, nothing in `crates/core/src` imports `egui`, `eframe`, `epaint`,
      `winit` or `crate::app` (grep).
- [x] The spectrogram on the Audio page looks the same (blenv screenshot before and after).

## Verification
- `cargo clippy --workspace --all-targets --all-features`, `cargo fmt`, `cargo test --workspace --all-features`.

## Notes

**What changed**
- `crates/audio_engine/src/spectrogram.rs`: new `SpectrogramImage { width, height, pixels: Vec<[u8; 4]> }`
  (row-major, straight alpha, always opaque) with `new` (black), `size()`, `rgb_bytes()` (packed RGB for BMP
  dumps) and `SpectrogramImage::BLACK`. `create_spectrogram_image`, `draw_markers` and
  `create_spectrogram_image_with_freqs` take/return it, `spectrogram_color` returns `[u8; 4]`. The colour values
  are the egui ones they replaced (`BLACK`, `MAGENTA` 255/0/255, `LIGHT_GRAY` 220, `DARK_GRAY` 96), so output is
  byte-identical. `egui` is gone from `crates/audio_engine/Cargo.toml`; `main.rs`, `bin/audio_corpus.rs` and
  `benches/spectrogram_performance.rs` build their BMP/PNG with `image::RgbImage::from_raw(.., rgb_bytes())`.
- `crates/core/src/app/components/spectrogram.rs`: the old re-export of `create_spectrogram_image` is replaced by
  `spectrogram_color_image(&SpectrogramImage) -> egui::ColorImage` (plus a unit test). `pages/audio.rs`'s
  `SpectrogramRing` keeps a `SpectrogramImage` and converts at texture upload, where it used to `clone()` the
  `ColorImage`, so the per-upload cost is the same (one pass over the pixels).
- `mainloop/bg_worker.rs` calls `blaulicht_audio_engine::create_spectrogram_image` directly and writes the BMP
  from `rgb_bytes()`; no egui or `crate::app` import, and the image buffer is reused across ticks.
- `plugin/wasm.rs`: dropped `use egui::ahash::HashMapExt` (the map is a std `HashMap`, `HashMap::new()` is
  inherent). `main.rs`: dropped the unused `use winit::event_loop::EventLoop` (bin clippy 1 -> 0).

**Verification**
- clippy: blaulicht-core lib 128 (was 129), lib test 146 (was 147), bin 0 (was 1). No new warnings in touched
  code. `cargo test --workspace --all-features`: all pass (core 349+1 new, audio_engine 37).
- `cargo tree -p blaulicht-audio-engine -i egui`: "did not match any packages".
- `cargo build -p blaulicht-core --no-default-features --features "x11 audio-mock wasmtime"` compiles;
  `cargo bench -p blaulicht-audio-engine --no-run` compiles.
- Visual: blenv `--desktop` with a copy of Kuze_Theater.json, Audio tab, Dummy: Noise, before (old build) and
  after. Same inferno colour mapping (dark purple noise floor), same translucent cyan beat lines over the full
  height, same 5 px top / 20 px bottom / 5 px left padding, same right-aligned partial fill. Not pixel-compared,
  as noise is random.
- bg_worker dump: ran the app with `BLAULICHT_SPECTROGRAM_DIR` set in the container; it writes `N.bmp` every
  tick. The images are all black, which is **pre-existing**: the dump is 480x100, so after padding 75 rows remain
  for 128 bins and `create_spectrogram_image` returns early (`height < bin_count`). Same path before this
  ticket. Follow-up (Inbox-worthy, not done here): raise the dump height to >= 153.

**Remaining hits outside `app/` (grep for egui/eframe/epaint/winit/glow/three_d/crate::app), plan for 053**
- `main.rs` (bin): `blaulicht_core::app::BlaulichtApp`, `eframe::NativeOptions`, `egui::ViewportBuilder`,
  `eframe::run_native`. This is the desk binary that wires the frontend; 053 moves it into the egui crate (and
  adds a headless binary that starts engine + persistence without it).
- `stage_assets.rs`: `three_d_asset` for `load_prepared_model` / `PreparedStageModel` / `StageModelStats` and
  the texture prep helpers, which only the visualizer uses, mixed with engine-side `install_glb`,
  `resolve_asset`, `copy_assets_for_save_as`, `read_glb_for_import` (sha2/fs only). 053: split it, keep the
  install/resolve/copy part in the engine crate and move the three-d-asset loading into the egui crate.
- Comments only, no code: `hub.rs:61` ("native egui frontend"), `plugin/wasm.rs` ("egui UI bridging"),
  and a test string in `log.rs:195` (`"blaulicht_core::app::pages::audio"`, the tracing-target to "UI" mapping;
  053 must update `source_for_target` when the module path changes).
- Cargo: `crates/core/Cargo.toml` still lists `winit` directly; after this ticket only
  `app/pages/visualizer/renderer.rs` uses it. It and all egui/eframe/three-d deps move with the egui crate in 053.

