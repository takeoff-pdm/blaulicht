//! Writes a standalone preview with frames produced by the real Rust renderer.
use wasm_led_animation_plugin::{Engine, COUNT, NAMES};
fn main() {
    let mut clips = Vec::new();
    for effect in 0..10 {
        let mut e = Engine::default();
        e.settings.effect = effect;
        // Warm up slow effects and finish the selection crossfade.
        e.frame(7.0, 120.0, 1.0, false, COUNT);
        let mut frames = Vec::new();
        for _ in 0..180 {
            frames.push(
                e.frame(1.0 / 15.0, 120.0, 1.0, false, COUNT)
                    .unwrap()
                    .into_iter()
                    .flat_map(|rgb| rgb.map(|v| (v * 255.0).round() as u8))
                    .collect::<Vec<_>>(),
            );
        }
        clips.push(frames);
    }
    let html = include_str!("preview.html")
        .replace("__NAMES__", &serde_json::to_string(&NAMES).unwrap())
        .replace("__CLIPS__", &serde_json::to_string(&clips).unwrap());
    println!("{html}");
}
