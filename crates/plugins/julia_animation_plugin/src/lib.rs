//! Julia: a slow, sine-shaped brightness pulse that fires on every second
//! beat (the offbeat). Each pulse is a half-sine hump `min -> max -> min`
//! stretched over the interval to the next trigger, so it reads as slow
//! breathing locked to half the beat rate.

use std::f64::consts::PI;

use blaulicht_plugin_framework as bpf;
use blaulicht_shared::{
    AnimationPropertyWrite, AnimationTickInput, AnimationTickOutput, ControlEvent, FixtureProperty,
    PluginUiEvent, TickInput,
};
use bpf::{AnimationPluginDescriptor, BlaulichtAnimationPlugin};

const MIN_ID: u8 = 1;
const MAX_ID: u8 = 2;
const PULSE_LEN_ID: u8 = 3;
const FLIP_ID: u8 = 4;

struct JuliaAnimation {
    last_beat_id: u64,
    beat_counter: u64,
    /// Smoothed estimate of the time between beats in seconds.
    beat_period_s: f64,
    /// Progress of the current pulse in 0..1, or `None` while idle.
    pulse_phase: Option<f64>,
    min: u8,
    max: u8,
    /// Swap which beat parity counts as the offbeat.
    flip_parity: bool,
    /// Pulse length in beats (2.0 = one hump spanning the two beats until the next trigger).
    pulse_beats: f64,
    /// Most recent computed brightness, shown in the status line.
    last_value: u16,
    /// Host clock (ms) at the previous tick, for real-time deltas.
    last_clock_ms: Option<u32>,
}

impl Default for JuliaAnimation {
    fn default() -> Self {
        Self {
            last_beat_id: 0,
            beat_counter: 0,
            beat_period_s: 0.5,
            pulse_phase: None,
            min: 0,
            max: 255,
            flip_parity: false,
            pulse_beats: 2.0,
            last_value: 0,
            last_clock_ms: None,
        }
    }
}

impl JuliaAnimation {
    fn serialize(&self) -> String {
        format!(
            "{},{},{},{}",
            self.min,
            self.max,
            u8::from(self.flip_parity),
            (self.pulse_beats * 10.0).round() as u8
        )
    }

    fn apply_serialized(&mut self, raw: &str) {
        let mut parts = raw.split(',');
        if let Some(v) = parts.next().and_then(|p| p.parse().ok()) {
            self.min = v;
        }
        if let Some(v) = parts.next().and_then(|p| p.parse::<u8>().ok()) {
            self.max = v;
        }
        if let Some(v) = parts.next().and_then(|p| p.parse::<u8>().ok()) {
            self.flip_parity = v != 0;
        }
        if let Some(v) = parts.next().and_then(|p| p.parse::<u8>().ok()) {
            self.pulse_beats = f64::from(v.clamp(5, 40)) / 10.0;
        }
    }

    fn handle_ui(&mut self, common: &TickInput) {
        let mut changed = false;
        for message in &common.events.events {
            let ControlEvent::AnimationPluginUi(event, _, _) = message.body() else {
                continue;
            };
            match event {
                PluginUiEvent::Slider { id: MIN_ID, value } => {
                    self.min = value;
                    changed = true;
                }
                PluginUiEvent::Slider { id: MAX_ID, value } => {
                    self.max = value;
                    changed = true;
                }
                PluginUiEvent::Slider {
                    id: PULSE_LEN_ID,
                    value,
                } => {
                    self.pulse_beats = f64::from(value.clamp(5, 40)) / 10.0;
                    changed = true;
                }
                PluginUiEvent::Switch { id: FLIP_ID, value } => {
                    self.flip_parity = value;
                    changed = true;
                }
                _ => {}
            }
        }
        if changed {
            bpf::save_plugin_state(bpf::PluginStateLocation::Showfile, &self.serialize());
        }
    }

    fn track_beats(&mut self, common: &TickInput) {
        let audio = &common.audio_data;
        let beat_changed = if audio.beat_event_id != 0 {
            audio.beat_event_id != self.last_beat_id
        } else {
            audio.beat_trigger
        };
        if beat_changed {
            self.last_beat_id = audio.beat_event_id;
            self.beat_counter += 1;
            let is_offbeat = (self.beat_counter % 2 == 1) ^ self.flip_parity;
            if is_offbeat {
                self.pulse_phase = Some(0.0);
            }
        }

        let measured_period = if audio.time_between_beats_millis > 0 {
            Some(f64::from(audio.time_between_beats_millis) / 1_000.0)
        } else if audio.bpm > 1.0 {
            Some(60.0 / f64::from(audio.bpm))
        } else {
            None
        };
        if let Some(period) = measured_period {
            let confidence = f64::from(audio.bpm_confidence).clamp(0.0, 1.0);
            let blend = (0.02 + confidence * 0.18).clamp(0.0, 0.25);
            self.beat_period_s += (period.clamp(0.2, 2.0) - self.beat_period_s) * blend;
        }
    }
}

impl BlaulichtAnimationPlugin for JuliaAnimation {
    fn run(&mut self, input: &AnimationTickInput, common: &TickInput) -> AnimationTickOutput {
        self.handle_ui(common);

        // Keep following the beat while paused (and in the editor preview) so
        // the status line stays live; only the output is suppressed.
        self.track_beats(common);

        // The host reports a nominal delta; the wall clock is what the beat
        // grid is measured against, so prefer it when it is plausible.
        let clock_delta_ms = self
            .last_clock_ms
            .map(|last| common.clock.wrapping_sub(last))
            .filter(|delta| (1..=500).contains(delta))
            .unwrap_or(input.delta_ms);
        self.last_clock_ms = Some(common.clock);
        let dt = f64::from(clock_delta_ms) / 1_000.0
            * input.speed_factor.as_float()
            * input.scene_speed_factor.as_float();
        let pulse_len = (self.beat_period_s * self.pulse_beats).max(0.05);
        if let Some(phase) = self.pulse_phase.as_mut() {
            *phase += dt / pulse_len;
            if *phase >= 1.0 {
                self.pulse_phase = None;
            }
        }

        let level = self.pulse_phase.map(|p| (PI * p).sin()).unwrap_or(0.0);
        let (lo, hi) = (f64::from(self.min), f64::from(self.max));
        let value = (lo + (hi - lo) * level).round().clamp(0.0, 255.0) as u16;
        self.last_value = value;

        if input.paused {
            return AnimationTickOutput::default();
        }

        AnimationTickOutput {
            writes: input
                .fixtures
                .iter()
                .enumerate()
                .map(|(fixture_index, _)| AnimationPropertyWrite {
                    fixture_index: fixture_index as u32,
                    property: FixtureProperty::Alpha,
                    value,
                })
                .collect(),
        }
    }

    fn initialize(&mut self, _input: &AnimationTickInput, _common: &TickInput) {
        *self = Self::default();
        if let Some(raw) = bpf::load_plugin_state(bpf::PluginStateLocation::Showfile) {
            self.apply_serialized(&raw);
        }
    }

    fn ui(&mut self, _input: &AnimationTickInput, common: &TickInput) {
        bpf::ui::begin();
        bpf::ui::label_styled("Julia: offbeat sine blink", 16, false);
        bpf::ui::label("Half-sine brightness pulse on every 2nd beat.");
        bpf::ui::separator();
        bpf::ui::slider("Min", MIN_ID, 0, 255, self.min);
        bpf::ui::slider("Max", MAX_ID, 0, 255, self.max);
        bpf::ui::slider(
            "Pulse length (beats x10)",
            PULSE_LEN_ID,
            5,
            40,
            (self.pulse_beats * 10.0).round() as u8,
        );
        bpf::ui::switch("Flip beat parity", FLIP_ID, self.flip_parity);
        bpf::ui::separator();
        bpf::ui::label(&format!(
            "beat #{} ({}) | period {:.0} ms | bpm {:.1} | pulse {} | out {}",
            self.beat_counter,
            if (self.beat_counter % 2 == 1) ^ self.flip_parity {
                "off"
            } else {
                "on"
            },
            self.beat_period_s * 1_000.0,
            common.audio_data.bpm,
            self.pulse_phase
                .map(|p| format!("{:.0}%", p * 100.0))
                .unwrap_or_else(|| "idle".into()),
            self.last_value,
        ));
    }
}

#[no_mangle]
extern "C" fn main() {
    bpf::hook_animation_plugin(
        AnimationPluginDescriptor::new("org.blaulicht.julia-animation", "Julia (offbeat sine)"),
        || Box::new(JuliaAnimation::default()),
    );
}
