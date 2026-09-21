use crate::*;
use blaulicht_plugin_framework as bpf;
use blaulicht_shared::{
    AnimationPropertyWrite, AnimationTickInput, AnimationTickOutput, ControlEvent, FixtureProperty,
    PluginUiEvent, TickInput,
};
use bpf::{AnimationPluginDescriptor, BlaulichtAnimationPlugin};

#[derive(Default)]
struct WasmLed {
    engine: Engine,
    clock: MotionClock,
}
impl WasmLed {
    fn events(&mut self, common: &TickInput) {
        let before = self.engine.settings.clone();
        for message in &common.events.events {
            let ControlEvent::AnimationPluginUi(event, _, _) = message.body() else {
                continue;
            };
            let s = &mut self.engine.settings;
            match event {
                PluginUiEvent::ComboBox { id: 1, selected } => s.effect = selected.min(9),
                PluginUiEvent::Button { id } if (30..40).contains(&id) => s.effect = id - 30,
                PluginUiEvent::Slider { id: 2, value } => s.speed = value.min(200),
                PluginUiEvent::Slider { id: 3, value } => s.brightness = value,
                PluginUiEvent::Slider { id: 4, value } => s.hue = value,
                PluginUiEvent::Checkbox { id, checked } if (10..15).contains(&id) => {
                    s.reversed[(id - 10) as usize] = checked
                }
                _ => {}
            }
        }
        if before != self.engine.settings {
            if let Ok(raw) = serde_json::to_string(&self.engine.settings) {
                bpf::save_plugin_state(bpf::PluginStateLocation::Showfile, &raw);
            }
        }
    }
}
impl BlaulichtAnimationPlugin for WasmLed {
    fn initialize(&mut self, _: &AnimationTickInput, _: &TickInput) {
        self.engine = Engine::default();
        self.clock = MotionClock::default();
        if let Some(raw) = bpf::load_plugin_state(bpf::PluginStateLocation::Showfile) {
            self.engine.settings = Settings::load(&raw);
        }
        self.engine.active = self.engine.settings.effect;
    }
    fn run(&mut self, input: &AnimationTickInput, common: &TickInput) -> AnimationTickOutput {
        self.events(common);
        let Some(frame) = self.engine.frame(
            self.clock.seconds(common.clock, input.delta_ms),
            common.audio_data.bpm as f64,
            input.speed_factor.as_float() * input.scene_speed_factor.as_float(),
            input.paused,
            input.fixtures.len(),
        ) else {
            return AnimationTickOutput::default();
        };
        let mut writes = Vec::with_capacity(COUNT * 4);
        for (index, rgb) in frame.into_iter().enumerate() {
            let (h, s, v) = rgb_hsv(rgb);
            for (property, value) in [
                (FixtureProperty::Alpha, (v * 255.0).round() as u16),
                (FixtureProperty::ColorHue, h.round() as u16 % 360),
                (FixtureProperty::ColorSaturation, (s * 255.0).round() as u16),
                (FixtureProperty::ColorValue, 255),
            ] {
                writes.push(AnimationPropertyWrite {
                    fixture_index: index as u32,
                    property,
                    value,
                });
            }
        }
        AnimationTickOutput { writes }
    }
    fn ui(&mut self, input: &AnimationTickInput, common: &TickInput) {
        let s = &self.engine.settings;
        bpf::ui::begin();
        bpf::ui::label_styled("WasmLED", 22, true);
        bpf::ui::label(&format!("Animation: {}", NAMES[s.effect as usize]));
        for row in 0..5 {
            bpf::ui::begin_horizontal();
            for effect in row * 2..row * 2 + 2 {
                bpf::ui::button_styled(
                    NAMES[effect],
                    30 + effect as u8,
                    s.effect as usize != effect,
                );
            }
            bpf::ui::end_horizontal();
        }
        bpf::ui::label(DESCRIPTIONS[s.effect as usize]);
        bpf::ui::slider(
            &format!("Speed ({:.2}x)", s.multiplier()),
            2,
            0,
            200,
            s.speed,
        );
        bpf::ui::slider("Brightness", 3, 0, 255, s.brightness);
        bpf::ui::slider("Hue shift", 4, 0, 255, s.hue);
        let (bpm, fallback) = tempo(common.audio_data.bpm as f64);
        bpf::ui::label(&format!(
            "Tempo: {:.1} BPM{}",
            bpm,
            if fallback { " (fallback)" } else { "" }
        ));
        match common.audio_data.tempo_source {
            Some((_, player)) => bpf::ui::label(&format!("External tempo: player {player}")),
            None => bpf::ui::label(
                "No external tempo. Enable Sync to audio in Pro DJ Link for CDJ BPM.",
            ),
        }
        bpf::ui::separator();
        for tube in 0..TUBES {
            bpf::ui::checkbox(
                &format!("Reverse tube {}", tube + 1),
                10 + tube as u8,
                s.reversed[tube],
            );
        }
        if input.fixtures.len() != COUNT {
            bpf::ui::label(&format!("Select exactly 495 pixel fixtures in tube order (5 x 99). Selected: {}. Output inactive.",input.fixtures.len()));
        } else {
            bpf::ui::label("5 tubes x 99 pixels. Each consecutive block is one tube.");
        }
    }
}
#[no_mangle]
extern "C" fn main() {
    bpf::hook_animation_plugin(
        AnimationPluginDescriptor::new("org.blaulicht.wasm-led", "WasmLED"),
        || Box::new(WasmLed::default()),
    );
}
