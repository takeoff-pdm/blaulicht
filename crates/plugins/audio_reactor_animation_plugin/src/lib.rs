use blaulicht_plugin_framework as bpf;
use blaulicht_shared::{
    AnimationPropertyWrite, AnimationTickInput, AnimationTickOutput, AudioSourceStatus,
    ControlEvent, FixtureProperty, PluginUiEvent, SectionState, TickInput,
};
use bpf::{AnimationPluginDescriptor, BlaulichtAnimationPlugin};
use std::f64::consts::TAU;

const SENSITIVITY_ID: u8 = 1;
const SMOOTHING_ID: u8 = 2;
const SPREAD_ID: u8 = 3;
const MOTION_ID: u8 = 4;
const BASE_LEVEL_ID: u8 = 5;
const GATE_ID: u8 = 6;
const STROBE_THRESHOLD_ID: u8 = 7;
const MODE_ID: u8 = 8;
const PALETTE_ID: u8 = 9;
const MOVE_HEADS_ID: u8 = 10;
const STROBE_ID: u8 = 11;
const REVERSE_ID: u8 = 12;
const RESET_ANALYSIS_ID: u8 = 13;

const MODE_NAMES: [&str; 4] = [
    "Chromatic pulse",
    "Orbital chase",
    "Mirror storm",
    "Kinetic bloom",
];
const PALETTE_NAMES: [&str; 5] = [
    "Section aware",
    "Ultraviolet",
    "Inferno",
    "Arctic",
    "Full spectrum",
];

#[derive(Clone, Copy)]
struct AdaptiveRange {
    floor: f64,
    ceiling: f64,
    initialized: bool,
}

impl Default for AdaptiveRange {
    fn default() -> Self {
        Self {
            floor: 0.0,
            ceiling: 1.0,
            initialized: false,
        }
    }
}

impl AdaptiveRange {
    fn observe(&mut self, sample: f64, dt: f64) -> f64 {
        if !self.initialized {
            self.floor = (sample - 0.08).max(0.0);
            self.ceiling = (sample + 0.18).min(1.0);
            self.initialized = true;
        }

        let floor_rate = if sample < self.floor { 8.0 } else { 0.055 };
        let ceiling_rate = if sample > self.ceiling { 9.0 } else { 0.11 };
        self.floor += (sample - self.floor) * (1.0 - (-floor_rate * dt).exp());
        self.ceiling += (sample - self.ceiling) * (1.0 - (-ceiling_rate * dt).exp());
        self.floor = self.floor.clamp(0.0, 0.92);
        self.ceiling = self.ceiling.clamp(self.floor + 0.08, 1.0);
        ((sample - self.floor) / (self.ceiling - self.floor)).clamp(0.0, 1.0)
    }
}

#[derive(Default)]
struct AudioFeatures {
    volume: f64,
    bass: f64,
    transient: f64,
    beat: f64,
    onset: f64,
    phase: f64,
    confidence: f64,
    fresh: bool,
}

struct AudioReactor {
    elapsed: f64,
    beat_phase: f64,
    beat_period: f64,
    beat_impulse: f64,
    onset_impulse: f64,
    drop_impulse: f64,
    volume_envelope: f64,
    bass_envelope: f64,
    volume_range: AdaptiveRange,
    bass_range: AdaptiveRange,
    last_beat_id: u64,
    last_onset_id: u64,
    previous_section: SectionState,
    features: AudioFeatures,
    sensitivity: u8,
    smoothing: u8,
    spread: u8,
    motion: u8,
    base_level: u8,
    gate: u8,
    strobe_threshold: u8,
    mode: u8,
    palette: u8,
    move_heads: bool,
    strobe: bool,
    reverse: bool,
}

impl Default for AudioReactor {
    fn default() -> Self {
        Self {
            elapsed: 0.0,
            beat_phase: 0.0,
            beat_period: 0.5,
            beat_impulse: 0.0,
            onset_impulse: 0.0,
            drop_impulse: 0.0,
            volume_envelope: 0.0,
            bass_envelope: 0.0,
            volume_range: AdaptiveRange::default(),
            bass_range: AdaptiveRange::default(),
            last_beat_id: 0,
            last_onset_id: 0,
            previous_section: SectionState::Breakdown,
            features: AudioFeatures::default(),
            sensitivity: 150,
            smoothing: 90,
            spread: 145,
            motion: 125,
            base_level: 12,
            gate: 25,
            strobe_threshold: 205,
            mode: 0,
            palette: 0,
            move_heads: true,
            strobe: true,
            reverse: false,
        }
    }
}

impl AudioReactor {
    fn handle_ui(&mut self, common: &TickInput) {
        for message in &common.events.events {
            let ControlEvent::AnimationPluginUi(event, _, _) = message.body() else {
                continue;
            };
            match event {
                PluginUiEvent::Slider {
                    id: SENSITIVITY_ID,
                    value,
                } => self.sensitivity = value,
                PluginUiEvent::Slider {
                    id: SMOOTHING_ID,
                    value,
                } => self.smoothing = value,
                PluginUiEvent::Slider {
                    id: SPREAD_ID,
                    value,
                } => self.spread = value,
                PluginUiEvent::Slider {
                    id: MOTION_ID,
                    value,
                } => self.motion = value,
                PluginUiEvent::Slider {
                    id: BASE_LEVEL_ID,
                    value,
                } => self.base_level = value,
                PluginUiEvent::Slider { id: GATE_ID, value } => self.gate = value,
                PluginUiEvent::Slider {
                    id: STROBE_THRESHOLD_ID,
                    value,
                } => self.strobe_threshold = value,
                PluginUiEvent::ComboBox {
                    id: MODE_ID,
                    selected,
                } => self.mode = selected.min(3),
                PluginUiEvent::ComboBox {
                    id: PALETTE_ID,
                    selected,
                } => self.palette = selected.min(4),
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
                PluginUiEvent::Button {
                    id: RESET_ANALYSIS_ID,
                } => {
                    self.volume_range = AdaptiveRange::default();
                    self.bass_range = AdaptiveRange::default();
                    self.volume_envelope = 0.0;
                    self.bass_envelope = 0.0;
                }
                _ => {}
            }
        }
    }

    fn envelope(current: f64, target: f64, dt: f64, attack: f64, release: f64) -> f64 {
        let rate = if target > current { attack } else { release };
        current + (target - current) * (1.0 - (-rate * dt).exp())
    }

    fn analyze(&mut self, input: &AnimationTickInput, common: &TickInput) {
        let dt = (input.delta_ms as f64 / 1_000.0).clamp(0.001, 0.1);
        self.elapsed += dt;
        let audio = &common.audio_data;
        let fresh =
            matches!(audio.source_status, AudioSourceStatus::Active) && audio.frame_age_ms <= 250;
        let volume_raw = if fresh {
            audio.volume as f64 / 255.0
        } else {
            0.0
        };
        let bass_raw = if fresh {
            audio.bass as f64 / 255.0
        } else {
            0.0
        };
        let avg = audio.bass_avg as f64 / 255.0;
        let short_avg = audio.bass_avg_short as f64 / 255.0;
        let transient_raw = ((bass_raw - avg * 0.55 - short_avg * 0.25) * 3.2).clamp(0.0, 1.0);

        let smoothing = self.smoothing as f64 / 255.0;
        let attack = 30.0 - smoothing * 22.0;
        let release = 8.0 - smoothing * 6.8;
        self.volume_envelope =
            Self::envelope(self.volume_envelope, volume_raw, dt, attack, release);
        self.bass_envelope = Self::envelope(
            self.bass_envelope,
            bass_raw,
            dt,
            attack * 1.3,
            release * 1.5,
        );

        let sensitivity = 0.55 + self.sensitivity as f64 / 150.0;
        let volume =
            (self.volume_range.observe(self.volume_envelope, dt) * sensitivity).clamp(0.0, 1.0);
        let bass = (self.bass_range.observe(self.bass_envelope, dt) * sensitivity).clamp(0.0, 1.0);

        let beat_changed = audio.beat_event_id != 0 && audio.beat_event_id != self.last_beat_id;
        let onset_changed = audio.onset_event_id != 0 && audio.onset_event_id != self.last_onset_id;
        if beat_changed || (audio.beat_event_id == 0 && audio.beat_trigger) {
            self.beat_phase = 0.0;
            self.beat_impulse = 1.0;
            self.last_beat_id = audio.beat_event_id;
        }
        if onset_changed || (audio.onset_event_id == 0 && audio.actual_onset_peak) {
            self.onset_impulse = 1.0;
            self.last_onset_id = audio.onset_event_id;
        }
        if audio.section_state == SectionState::Drop && self.previous_section != SectionState::Drop
        {
            self.drop_impulse = 1.0;
        }
        self.previous_section = audio.section_state;

        let measured_period = if audio.time_between_beats_millis > 0 {
            Some(audio.time_between_beats_millis as f64 / 1_000.0)
        } else if audio.bpm > 1.0 {
            Some(60.0 / audio.bpm as f64)
        } else {
            None
        };
        if let Some(period) = measured_period {
            let confidence = (audio.bpm_confidence as f64).clamp(0.0, 1.0);
            let blend = (0.02 + confidence * 0.18).clamp(0.0, 0.25);
            self.beat_period += (period.clamp(0.2, 2.0) - self.beat_period) * blend;
        }
        self.beat_phase = (self.beat_phase + dt / self.beat_period.max(0.2)).rem_euclid(1.0);
        self.beat_impulse = (self.beat_impulse - dt * 5.2).max(0.0);
        self.onset_impulse = (self.onset_impulse - dt * 8.5).max(0.0);
        self.drop_impulse = (self.drop_impulse - dt * 0.65).max(0.0);

        self.features = AudioFeatures {
            volume,
            bass,
            transient: transient_raw,
            beat: self.beat_impulse,
            onset: self.onset_impulse,
            phase: self.beat_phase,
            confidence: (audio.bpm_confidence as f64).clamp(0.0, 1.0),
            fresh,
        };
    }

    fn circular_distance(a: f64, b: f64) -> f64 {
        let distance = (a - b).abs().rem_euclid(1.0);
        distance.min(1.0 - distance)
    }

    fn spatial_shape(&self, position: f64) -> f64 {
        let phase = if self.reverse {
            1.0 - self.features.phase
        } else {
            self.features.phase
        };
        let width = 0.04 + (255 - self.spread) as f64 / 1_500.0;
        match self.mode {
            0 => 0.62 + 0.38 * (position * TAU * 2.0 - phase * TAU).sin().max(0.0),
            1 => (-Self::circular_distance(position, phase).powi(2) / (2.0 * width.powi(2))).exp(),
            2 => {
                let mirrored = (position - 0.5).abs() * 2.0;
                (-Self::circular_distance(mirrored, phase).powi(2) / (2.0 * width.powi(2))).exp()
            }
            _ => {
                let radius = (position - 0.5).abs() * 2.0;
                (-(radius - phase).powi(2) / (2.0 * width.powi(2))).exp()
            }
        }
    }

    fn base_hue(&self, section: SectionState) -> f64 {
        match self.palette {
            1 => 268.0,
            2 => 8.0,
            3 => 188.0,
            4 => self.elapsed * 42.0,
            _ => match section {
                SectionState::Breakdown => 224.0,
                SectionState::Drop => 322.0,
                SectionState::ActiveBeat => 24.0,
            },
        }
    }

    fn push(
        writes: &mut Vec<AnimationPropertyWrite>,
        index: usize,
        property: FixtureProperty,
        value: u16,
    ) {
        writes.push(AnimationPropertyWrite {
            fixture_index: index as u32,
            property,
            value,
        });
    }
}

impl BlaulichtAnimationPlugin for AudioReactor {
    fn run(&mut self, input: &AnimationTickInput, common: &TickInput) -> AnimationTickOutput {
        self.handle_ui(common);
        self.analyze(input, common);
        if input.paused || input.fixtures.is_empty() {
            return AnimationTickOutput::default();
        }

        let feature = &self.features;
        let gate = self.gate as f64 / 255.0;
        let signal = feature.volume * 0.28 + feature.bass * 0.42 + feature.transient * 0.3;
        let gated = if signal <= gate {
            0.0
        } else {
            (signal - gate) / (1.0 - gate).max(0.01)
        };
        let section = common.audio_data.section_state;
        let section_energy = match section {
            SectionState::Breakdown => 0.72,
            SectionState::Drop => 1.18,
            SectionState::ActiveBeat => 1.0,
        };
        let speed_scale = input.speed_factor.as_float() * input.scene_speed_factor.as_float();
        let motion = self.motion as f64 / 255.0;
        let fixture_count = input.fixtures.len();
        let mut writes = Vec::with_capacity(fixture_count * 8);

        for index in 0..fixture_count {
            let position = index as f64 / fixture_count as f64;
            let spatial = self.spatial_shape(position);
            let ripple =
                ((position * TAU * 3.0 - self.elapsed * (0.5 + motion * 4.0) * speed_scale).sin()
                    * 0.5
                    + 0.5)
                    * feature.volume;
            let impact = feature.beat * spatial + feature.onset * (0.35 + ripple * 0.65);
            let brightness = (self.base_level as f64 / 255.0
                + gated * (0.22 + spatial * 0.58)
                + ripple * 0.18
                + impact * 0.75
                + self.drop_impulse * spatial * 0.55)
                * section_energy;
            let brightness = brightness.clamp(0.0, 1.0);
            let hue = (self.base_hue(section)
                + position * (45.0 + self.spread as f64 * 1.05)
                + feature.bass * 52.0
                + feature.onset * 95.0
                + spatial * self.drop_impulse * 150.0)
                .rem_euclid(360.0);
            let saturation =
                (205.0 + feature.bass * 50.0 - self.drop_impulse * 35.0).clamp(0.0, 255.0);

            Self::push(
                &mut writes,
                index,
                FixtureProperty::Alpha,
                (brightness * 255.0) as u16,
            );
            Self::push(&mut writes, index, FixtureProperty::ColorHue, hue as u16);
            Self::push(
                &mut writes,
                index,
                FixtureProperty::ColorSaturation,
                saturation as u16,
            );
            Self::push(&mut writes, index, FixtureProperty::ColorValue, 255);
            Self::push(
                &mut writes,
                index,
                FixtureProperty::Focus,
                ((0.25 + spatial * 0.55 + feature.transient * 0.2) * 255.0) as u16,
            );

            if self.strobe {
                let trigger =
                    (feature.onset * 0.65 + feature.beat * 0.55 + feature.transient * 0.35) * 255.0;
                let strobe = if trigger >= self.strobe_threshold as f64 {
                    (90.0 + trigger * 0.64).min(255.0) as u16
                } else {
                    0
                };
                Self::push(&mut writes, index, FixtureProperty::Strobe, strobe);
            }

            if self.move_heads {
                let orbit = self.elapsed * (0.18 + motion * 1.2) * speed_scale
                    + position * TAU
                    + feature.phase * TAU;
                let pan = ((orbit.sin() * (0.28 + feature.bass * 0.22) + 0.5) * 255.0)
                    .clamp(0.0, 255.0) as u16;
                let tilt = ((orbit.cos() * (0.20 + feature.volume * 0.25) + 0.5) * 255.0)
                    .clamp(0.0, 255.0) as u16;
                Self::push(&mut writes, index, FixtureProperty::Pan, pan);
                Self::push(&mut writes, index, FixtureProperty::Tilt, tilt);
            }
        }

        AnimationTickOutput { writes }
    }

    fn ui(&mut self, _input: &AnimationTickInput, common: &TickInput) {
        bpf::ui::begin();
        bpf::ui::label_styled("AUDIO REACTOR X", 22, true);
        let status = if self.features.fresh {
            "LIVE"
        } else {
            "NO FRESH AUDIO"
        };
        bpf::ui::label(&format!(
            "{status}  |  {:.1} BPM  |  confidence {:>3}%  |  phase {:>3}%",
            common.audio_data.bpm,
            (self.features.confidence * 100.0) as u8,
            (self.features.phase * 100.0) as u8,
        ));
        bpf::ui::label(&format!(
            "level {:>3}%  bass {:>3}%  transient {:>3}%  beat {:>3}%  onset {:>3}%",
            (self.features.volume * 100.0) as u8,
            (self.features.bass * 100.0) as u8,
            (self.features.transient * 100.0) as u8,
            (self.features.beat * 100.0) as u8,
            (self.features.onset * 100.0) as u8,
        ));
        bpf::ui::separator();
        bpf::ui::combo_box(
            "Choreography",
            MODE_ID,
            &MODE_NAMES.map(str::to_owned),
            self.mode,
        );
        bpf::ui::combo_box(
            "Palette",
            PALETTE_ID,
            &PALETTE_NAMES.map(str::to_owned),
            self.palette,
        );
        bpf::ui::slider("Sensitivity", SENSITIVITY_ID, 0, 255, self.sensitivity);
        bpf::ui::slider("Envelope smoothing", SMOOTHING_ID, 0, 255, self.smoothing);
        bpf::ui::slider("Spatial spread", SPREAD_ID, 0, 255, self.spread);
        bpf::ui::slider("Motion", MOTION_ID, 0, 255, self.motion);
        bpf::ui::slider("Ambient floor", BASE_LEVEL_ID, 0, 255, self.base_level);
        bpf::ui::slider("Noise gate", GATE_ID, 0, 240, self.gate.min(240));
        bpf::ui::slider(
            "Strobe threshold",
            STROBE_THRESHOLD_ID,
            0,
            255,
            self.strobe_threshold,
        );
        bpf::ui::checkbox("Kinetic pan/tilt", MOVE_HEADS_ID, self.move_heads);
        bpf::ui::checkbox("Transient strobe", STROBE_ID, self.strobe);
        bpf::ui::switch("Reverse choreography", REVERSE_ID, self.reverse);
        bpf::ui::button("Reset adaptive analysis", RESET_ANALYSIS_ID);
    }
}

#[no_mangle]
extern "C" fn main() {
    bpf::hook_animation_plugin(
        AnimationPluginDescriptor::new("org.blaulicht.audio-reactor-x", "Audio Reactor X"),
        || Box::new(AudioReactor::default()),
    );
}
