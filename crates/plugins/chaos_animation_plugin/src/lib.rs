use blaulicht_plugin_framework as bpf;
use blaulicht_shared::{
    AnimationPropertyWrite, AnimationTickInput, AnimationTickOutput, ControlEvent, FixtureProperty,
    PluginUiEvent, TickInput,
};
use bpf::{AnimationPluginDescriptor, BlaulichtAnimationPlugin};
use std::f64::consts::TAU;

const SPEED_ID: u8 = 1;
const TWIST_ID: u8 = 2;
const CHAOS_ID: u8 = 3;
const AUDIO_REACTIVE_ID: u8 = 4;
const MOVE_HEADS_ID: u8 = 5;
const STROBE_ID: u8 = 6;
const REVERSE_ID: u8 = 7;

struct ChaosAnimation {
    phase: f64,
    beat_burst: f64,
    speed: u8,
    twist: u8,
    chaos: u8,
    audio_reactive: bool,
    move_heads: bool,
    strobe: bool,
    reverse: bool,
}

impl Default for ChaosAnimation {
    fn default() -> Self {
        Self {
            phase: 0.0,
            beat_burst: 0.0,
            speed: 110,
            twist: 90,
            chaos: 115,
            audio_reactive: true,
            move_heads: true,
            strobe: true,
            reverse: false,
        }
    }
}

impl ChaosAnimation {
    fn handle_ui(&mut self, common: &TickInput) {
        for message in &common.events.events {
            let ControlEvent::AnimationPluginUi(event, _, _) = message.body() else {
                continue;
            };
            match event {
                PluginUiEvent::Slider {
                    id: SPEED_ID,
                    value,
                } => self.speed = value,
                PluginUiEvent::Slider {
                    id: TWIST_ID,
                    value,
                } => self.twist = value,
                PluginUiEvent::Slider {
                    id: CHAOS_ID,
                    value,
                } => self.chaos = value,
                PluginUiEvent::Checkbox {
                    id: AUDIO_REACTIVE_ID,
                    checked,
                } => self.audio_reactive = checked,
                PluginUiEvent::Checkbox {
                    id: MOVE_HEADS_ID,
                    checked,
                } => self.move_heads = checked,
                PluginUiEvent::Checkbox {
                    id: STROBE_ID,
                    checked,
                } => self.strobe = checked,
                PluginUiEvent::Switch {
                    id: REVERSE_ID,
                    value,
                } => self.reverse = value,
                _ => {}
            }
        }
    }

    fn write(
        writes: &mut Vec<AnimationPropertyWrite>,
        fixture_index: usize,
        property: FixtureProperty,
        value: u16,
    ) {
        writes.push(AnimationPropertyWrite {
            fixture_index: fixture_index as u32,
            property,
            value,
        });
    }
}

impl BlaulichtAnimationPlugin for ChaosAnimation {
    fn run(&mut self, input: &AnimationTickInput, common: &TickInput) -> AnimationTickOutput {
        self.handle_ui(common);
        if input.paused || input.fixtures.is_empty() {
            return AnimationTickOutput::default();
        }

        let dt = input.delta_ms as f64 / 1_000.0;
        let speed = 0.35 + self.speed as f64 / 24.0;
        let direction = if self.reverse { -1.0 } else { 1.0 };
        let animation_speed = input.speed_factor.as_float() * input.scene_speed_factor.as_float();
        self.phase = (self.phase + dt * speed * animation_speed * direction).rem_euclid(TAU);

        if self.audio_reactive && common.audio_data.beat_trigger {
            self.beat_burst = 1.0;
        } else {
            self.beat_burst = (self.beat_burst - dt * 2.8).max(0.0);
        }

        let fixture_count = input.fixtures.len();
        let twist = 0.5 + self.twist as f64 / 32.0;
        let chaos_amount = self.chaos as f64 / 255.0;
        let audio_energy = if self.audio_reactive {
            (common.audio_data.bass as f64 / 255.0) * 0.55
                + (common.audio_data.volume as f64 / 255.0) * 0.2
        } else {
            0.0
        };
        let mut writes = Vec::with_capacity(fixture_count * 8);

        for fixture_index in 0..fixture_count {
            let position = fixture_index as f64 / fixture_count as f64;
            let fixture_phase = self.phase + position * TAU * twist;
            let wave = fixture_phase.sin() * 0.5 + 0.5;
            let counter_wave =
                (self.phase * 0.71 - position * TAU * (twist + 1.7)).cos() * 0.5 + 0.5;
            let glitch = ((fixture_index as f64 * 12.9898 + self.phase * 4.17).sin() * 43_758.5453)
                .fract()
                .abs();
            let shard = (wave * (1.0 - chaos_amount) + glitch * chaos_amount).clamp(0.0, 1.0);
            let comet = shard.powf(2.4);
            let brightness = (0.05
                + comet * 0.72
                + counter_wave * chaos_amount * 0.18
                + audio_energy
                + self.beat_burst * (1.0 - position * 0.45))
                .clamp(0.0, 1.0);

            let hue = (position * 360.0
                + self.phase.to_degrees() * 2.3
                + counter_wave * 130.0
                + glitch * chaos_amount * 160.0
                + self.beat_burst * 180.0)
                .rem_euclid(360.0);
            let saturation = (190.0 + 65.0 * (1.0 - self.beat_burst)).clamp(0.0, 255.0);
            let focus = ((counter_wave * 180.0 + self.beat_burst * 75.0).clamp(0.0, 255.0)) as u16;

            Self::write(
                &mut writes,
                fixture_index,
                FixtureProperty::Alpha,
                (brightness * 255.0) as u16,
            );
            Self::write(
                &mut writes,
                fixture_index,
                FixtureProperty::ColorHue,
                hue as u16,
            );
            Self::write(
                &mut writes,
                fixture_index,
                FixtureProperty::ColorSaturation,
                saturation as u16,
            );
            Self::write(&mut writes, fixture_index, FixtureProperty::ColorValue, 255);
            Self::write(&mut writes, fixture_index, FixtureProperty::Focus, focus);

            if self.strobe {
                let strobe = if self.beat_burst > 0.72 {
                    245
                } else if glitch > 0.88 && chaos_amount > 0.35 {
                    (80.0 + chaos_amount * 130.0) as u16
                } else {
                    0
                };
                Self::write(&mut writes, fixture_index, FixtureProperty::Strobe, strobe);
            }

            if self.move_heads {
                let pan = ((wave * 0.78 + glitch * chaos_amount * 0.22) * 255.0) as u16;
                let tilt = ((counter_wave * 0.84 + comet * 0.16) * 255.0) as u16;
                Self::write(&mut writes, fixture_index, FixtureProperty::Pan, pan);
                Self::write(&mut writes, fixture_index, FixtureProperty::Tilt, tilt);
            }
        }

        AnimationTickOutput { writes }
    }

    fn ui(&mut self, _input: &AnimationTickInput, _common: &TickInput) {
        bpf::ui::begin();
        bpf::ui::label_styled("CHAOS ENGINE", 22, true);
        bpf::ui::label("A fixture-order vortex with beat detonations and deterministic glitches.");
        bpf::ui::separator();
        bpf::ui::slider("Warp speed", SPEED_ID, 1, 255, self.speed);
        bpf::ui::slider("Spatial twist", TWIST_ID, 0, 255, self.twist);
        bpf::ui::slider("Glitch amount", CHAOS_ID, 0, 255, self.chaos);
        bpf::ui::checkbox(
            "Audio-reactive beat detonations",
            AUDIO_REACTIVE_ID,
            self.audio_reactive,
        );
        bpf::ui::checkbox("Pan/tilt vortex", MOVE_HEADS_ID, self.move_heads);
        bpf::ui::checkbox("Glitch strobe", STROBE_ID, self.strobe);
        bpf::ui::switch("Reverse spacetime", REVERSE_ID, self.reverse);
    }
}

#[no_mangle]
extern "C" fn main() {
    bpf::hook_animation_plugin(
        AnimationPluginDescriptor::new("org.blaulicht.chaos-engine", "Chaos engine"),
        || Box::new(ChaosAnimation::default()),
    );
}
