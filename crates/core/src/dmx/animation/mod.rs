pub mod audio;
pub mod phaser;

use crate::dmx::DmxEngine;
use audio::{AudioModulationContribution, AudioModulationRuntime};
use blaulicht_audio_engine::CollectorOutput;
use blaulicht_shared::{
    fixture::{state::FixtureState, value::FixtureValue},
    palette::Palette,
    AnimationSpec, AnimationSpecBody, AudioModulationSignal, AudioSourceStatus, FixtureProperty,
    FlashAnimationSpec, FlashWindowLayout, PhaserDuration,
};
pub use phaser::*;
use rand::seq::SliceRandom;
use std::{
    collections::{hash_map::DefaultHasher, BTreeMap, HashMap},
    hash::{Hash, Hasher},
};

const DEGREES_PER_CYCLE: f64 = 360.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct AnimationRuntimeKey {
    scene_id: u8,
    selection_fingerprint: u64,
    animation_id: u8,
    fixture: (u8, u8),
}

/// Identifies one animation layer, independent of the fixtures it drives.
/// Continuous audio signals produce a single uniform value per layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct AnimationLayerKey {
    scene_id: u8,
    selection_fingerprint: u64,
    animation_id: u8,
}

/// Where a transient animation output lands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct ModulationTarget {
    pub scene_id: u8,
    pub fixture: (u8, u8),
    pub property: FixtureProperty,
}

/// One frame's accumulated animation output for a single scene property.
///
/// This lives entirely in engine runtime state — authored `fixture_states` are
/// never written by the animation tick, so pausing, removing or retargeting an
/// animation simply stops contributing on the next frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PropertyModulation {
    /// Phaser or legacy-compatibility absolute output. Last write wins, and
    /// animations are visited in ascending animation-ID order.
    pub absolute: Option<u16>,
    pub scale: f64,
    pub add: f64,
}

impl Default for PropertyModulation {
    fn default() -> Self {
        Self {
            absolute: None,
            scale: 1.0,
            add: 0.0,
        }
    }
}

impl PropertyModulation {
    /// Resolves the final value: the authored/palette-resolved base (or the
    /// absolute override) scaled by every scale layer and offset by every
    /// additive layer, then clamped to the property's range.
    pub fn resolve(&self, base: u16, property: FixtureProperty) -> u16 {
        let source = self.absolute.unwrap_or(base) as f64;
        let value = source * self.scale + self.add;
        let max = match property {
            FixtureProperty::ColorHue => 360.0,
            _ => 255.0,
        };
        value.clamp(0.0, max).round() as u16
    }
}

/// Transient animation output for the frame, keyed by scene property.
#[derive(Default)]
pub(crate) struct ModulationOutputs {
    entries: HashMap<ModulationTarget, PropertyModulation>,
}

impl ModulationOutputs {
    fn clear(&mut self) {
        self.entries.clear();
    }

    fn entry(&mut self, target: ModulationTarget) -> &mut PropertyModulation {
        self.entries.entry(target).or_default()
    }

    #[cfg(test)]
    pub fn get(&self, target: &ModulationTarget) -> Option<&PropertyModulation> {
        self.entries.get(target)
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Applies this frame's output for `scene_id`/`fixture` onto a
    /// palette-resolved fixture state, before normal scene merging.
    pub fn apply_to_fixture(
        &self,
        state: &mut FixtureState,
        scene_id: u8,
        fixture: (u8, u8),
        palettes: &BTreeMap<u8, Palette>,
    ) {
        if self.entries.is_empty() {
            return;
        }
        for property in ALL_PROPERTIES {
            let target = ModulationTarget {
                scene_id,
                fixture,
                property,
            };
            let Some(modulation) = self.entries.get(&target) else {
                continue;
            };
            let base = state.resolved_value(property, palettes);
            *state.slot_mut(property) = FixtureValue::Literal(modulation.resolve(base, property));
        }
    }
}

const ALL_PROPERTIES: [FixtureProperty; 8] = [
    FixtureProperty::Alpha,
    FixtureProperty::Strobe,
    FixtureProperty::Focus,
    FixtureProperty::ColorHue,
    FixtureProperty::ColorSaturation,
    FixtureProperty::ColorValue,
    FixtureProperty::Tilt,
    FixtureProperty::Pan,
];

#[derive(Clone, Debug, Default)]
struct FixturePhaseRuntime {
    phase_degrees: f64,
    phase_offset_degrees: f64,
    last_update_ms: u64,
    seen_generation: u64,
}

impl FixturePhaseRuntime {
    fn reset(&mut self, now_ms: u64, phase_degrees: f64) {
        self.phase_degrees = phase_degrees;
        self.phase_offset_degrees = phase_degrees;
        self.last_update_ms = now_ms;
    }

    fn advance_local(&mut self, now_ms: u64, cycle_duration_ms: Option<f64>) -> u64 {
        let elapsed_ms = now_ms.saturating_sub(self.last_update_ms) as f64;
        self.last_update_ms = now_ms;
        let Some(cycle_duration_ms) = cycle_duration_ms else {
            return 0;
        };
        if !cycle_duration_ms.is_finite() || cycle_duration_ms <= 0.0 {
            return 0;
        }

        let previous_iteration = (self.phase_degrees / DEGREES_PER_CYCLE).floor().max(0.0);
        self.phase_degrees += elapsed_ms * DEGREES_PER_CYCLE / cycle_duration_ms;
        let next_iteration = (self.phase_degrees / DEGREES_PER_CYCLE).floor().max(0.0);
        (next_iteration - previous_iteration).max(0.0) as u64
    }

    fn align_to_beat_clock(
        &mut self,
        now_ms: u64,
        beat_position: f64,
        beats_per_cycle: f64,
    ) -> u64 {
        self.last_update_ms = now_ms;
        if !beats_per_cycle.is_finite() || beats_per_cycle <= 0.0 {
            return 0;
        }
        let previous_iteration = (self.phase_degrees / DEGREES_PER_CYCLE).floor().max(0.0);
        self.phase_degrees =
            self.phase_offset_degrees + beat_position * DEGREES_PER_CYCLE / beats_per_cycle;
        let next_iteration = (self.phase_degrees / DEGREES_PER_CYCLE).floor().max(0.0);
        (next_iteration - previous_iteration).max(0.0) as u64
    }
}

#[derive(Clone, Debug, Default)]
struct MusicalBeatClock {
    beat_position: f64,
    period_ms: Option<f64>,
    pending_phase_correction: f64,
    last_update_ms: u64,
    last_event_id: u64,
    initialized: bool,
}

const TEMPO_PERIOD_DEADBAND: f64 = 0.005;
const MAX_PERIOD_SLEW_PER_SECOND: f64 = 0.04;
const MAX_PHASE_CORRECTION_BEATS: f64 = 0.25;
const PHASE_CORRECTION_SPEED_FRACTION: f64 = 0.15;

impl MusicalBeatClock {
    fn update(&mut self, now_ms: u64, audio: &blaulicht_shared::CollectedAudioSnapshot) {
        if !self.initialized {
            self.last_update_ms = now_ms;
            self.initialized = true;
        }

        let elapsed_ms = now_ms.saturating_sub(self.last_update_ms) as f64;
        if let Some(period_ms) = self.period_ms.filter(|period| *period > 0.0) {
            let natural_advance = elapsed_ms / period_ms;
            // Phase correction is deliberately slower than forward motion, so
            // a late beat can never make an animation jump or run backwards.
            let maximum_correction = natural_advance * PHASE_CORRECTION_SPEED_FRACTION;
            let correction = self
                .pending_phase_correction
                .clamp(-maximum_correction, maximum_correction);
            self.beat_position += natural_advance + correction;
            self.pending_phase_correction -= correction;
        }
        self.last_update_ms = now_ms;

        let incoming_period = if audio.source_status != AudioSourceStatus::Disconnected
            && audio.time_between_beats_millis > 0
        {
            Some(audio.time_between_beats_millis as f64)
        } else {
            None
        };
        match (self.period_ms, incoming_period) {
            (_, None) => {
                self.period_ms = None;
                self.pending_phase_correction = 0.0;
            }
            (None, Some(incoming)) => {
                self.period_ms = Some(incoming);
            }
            (Some(current), Some(incoming)) => {
                let aligned = align_period(current, incoming);
                let relative_change = (aligned - current).abs() / current;
                let target = if relative_change <= TEMPO_PERIOD_DEADBAND {
                    current
                } else {
                    aligned
                };
                let maximum_change = current * MAX_PERIOD_SLEW_PER_SECOND * (elapsed_ms / 1_000.0);
                self.period_ms =
                    Some(current + (target - current).clamp(-maximum_change, maximum_change));
            }
        }

        if audio.beat_trigger
            && audio.beat_event_id != 0
            && audio.beat_event_id != self.last_event_id
        {
            self.pending_phase_correction = (self.beat_position.round() - self.beat_position)
                .clamp(-MAX_PHASE_CORRECTION_BEATS, MAX_PHASE_CORRECTION_BEATS);
            self.last_event_id = audio.beat_event_id;
        }
    }
}

fn align_period(reference: f64, candidate: f64) -> f64 {
    [candidate / 2.0, candidate, candidate * 2.0]
        .into_iter()
        .min_by(|left, right| {
            (left - reference)
                .abs()
                .total_cmp(&(right - reference).abs())
        })
        .unwrap_or(candidate)
}

#[derive(Default)]
pub(crate) struct AnimationClockRuntime {
    fixture_phases: HashMap<AnimationRuntimeKey, FixturePhaseRuntime>,
    modulation_layers: HashMap<AnimationLayerKey, AudioModulationRuntime>,
    flash_layers: HashMap<AnimationLayerKey, FlashAnimationRuntime>,
    beat_clock: MusicalBeatClock,
    generation: u64,
    fixture_scratch: Vec<(u8, u8)>,
    phase_scratch: Vec<f64>,
    pub(crate) outputs: ModulationOutputs,
}

impl AnimationClockRuntime {
    fn begin_tick(&mut self) -> u64 {
        self.generation = self.generation.wrapping_add(1).max(1);
        self.outputs.clear();
        self.generation
    }

    fn retain_current_generation(&mut self) {
        let generation = self.generation;
        self.fixture_phases
            .retain(|_, runtime| runtime.seen_generation == generation);
        self.modulation_layers
            .retain(|_, runtime| runtime.seen_generation == generation);
        self.flash_layers
            .retain(|_, runtime| runtime.seen_generation == generation);
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum FlashPhase {
    #[default]
    Off,
    On,
}

#[derive(Debug, Default)]
struct FlashAnimationRuntime {
    phase: FlashPhase,
    phase_started_ms: u64,
    phase_started_beats: f64,
    initialized: bool,
    window_cursor: usize,
    windows: Vec<Vec<usize>>,
    window_order: Vec<usize>,
    window_size: usize,
    window_layout: FlashWindowLayout,
    random_order: bool,
    beat_aligned: bool,
    reverse_after_n_iterations: Option<u32>,
    reversed: bool,
    completed_sweeps: u32,
    fixture_count: usize,
    seen_generation: u64,
}

fn flash_windows(
    fixture_count: usize,
    window_size: usize,
    layout: FlashWindowLayout,
) -> Vec<Vec<usize>> {
    if fixture_count == 0 {
        return Vec::new();
    }
    let window_size = window_size.max(1).min(fixture_count);
    let window_count = fixture_count.div_ceil(window_size);
    let mut windows = vec![Vec::new(); window_count];

    match layout {
        FlashWindowLayout::Contiguous => {
            for fixture_index in 0..fixture_count {
                windows[fixture_index / window_size].push(fixture_index);
            }
        }
        FlashWindowLayout::Spaced => {
            for fixture_index in 0..fixture_count {
                windows[fixture_index % window_count].push(fixture_index);
            }
        }
    }
    windows
}

impl FlashAnimationRuntime {
    fn reset(
        &mut self,
        now_ms: u64,
        beat_position: Option<f64>,
        fixture_count: usize,
        spec: &FlashAnimationSpec,
        gap_speed: f64,
    ) {
        self.phase = FlashPhase::Off;
        self.phase_started_ms = now_ms;
        self.phase_started_beats = beat_position.unwrap_or_default();
        self.initialized = true;
        self.window_cursor = 0;
        self.fixture_count = fixture_count;
        self.window_size = usize::from(spec.window_size.max(1));
        self.window_layout = spec.window_layout;
        self.random_order = spec.random_order;
        self.beat_aligned = spec.beat_aligned;
        self.reverse_after_n_iterations = spec.reverse_after_n_iterations;
        self.reversed = false;
        self.completed_sweeps = 0;
        self.windows = flash_windows(fixture_count, self.window_size, self.window_layout);
        self.window_order = (0..self.windows.len()).collect();
        self.prepare_sweep();

        if spec.beat_aligned {
            let off_beats = (spec.off_time_beats.as_float() * gap_speed).max(f64::EPSILON);
            if let Some(position) = beat_position {
                let next_boundary = ((position / off_beats).floor() + 1.0) * off_beats;
                self.phase_started_beats = next_boundary - off_beats;
            }
        }
    }

    fn prepare_sweep(&mut self) {
        self.window_order.clear();
        self.window_order.extend(0..self.windows.len());
        if self.random_order && self.window_order.len() > 1 {
            self.window_order.shuffle(&mut rand::rng());
        } else if self.reversed {
            self.window_order.reverse();
        }
    }

    fn advance_phase(&mut self) {
        match self.phase {
            FlashPhase::Off => self.phase = FlashPhase::On,
            FlashPhase::On => {
                self.phase = FlashPhase::Off;
                self.window_cursor += 1;
                if self.window_cursor >= self.window_order.len() {
                    self.window_cursor = 0;
                    self.completed_sweeps = self.completed_sweeps.saturating_add(1);
                    if !self.random_order {
                        if let Some(n) = self.reverse_after_n_iterations.filter(|n| *n > 0) {
                            if self.completed_sweeps % n == 0 {
                                self.reversed = !self.reversed;
                            }
                        }
                    }
                    self.prepare_sweep();
                }
            }
        }
    }

    fn tick(
        &mut self,
        now_ms: u64,
        beat_position: Option<f64>,
        fixture_count: usize,
        spec: &FlashAnimationSpec,
        gap_speed: f64,
    ) -> Option<&[usize]> {
        let structural_change = self.fixture_count != fixture_count
            || self.window_size != usize::from(spec.window_size.max(1))
            || self.window_layout != spec.window_layout
            || self.random_order != spec.random_order
            || self.beat_aligned != spec.beat_aligned
            || self.reverse_after_n_iterations != spec.reverse_after_n_iterations;
        if !self.initialized || structural_change {
            self.reset(now_ms, beat_position, fixture_count, spec, gap_speed);
        }

        let mut transitions = 0;
        if spec.beat_aligned {
            let Some(beat_position) = beat_position else {
                self.initialized = false;
                self.phase = FlashPhase::Off;
                return None;
            };
            let off_duration = (spec.off_time_beats.as_float() * gap_speed).max(f64::EPSILON);
            let on_duration = spec.on_time_beats.as_float().max(f64::EPSILON);
            loop {
                let duration = match self.phase {
                    FlashPhase::Off => off_duration,
                    FlashPhase::On => on_duration,
                };
                if beat_position - self.phase_started_beats + f64::EPSILON < duration {
                    break;
                }
                self.phase_started_beats += duration;
                self.advance_phase();
                transitions += 1;
                if transitions >= 10_000 {
                    self.reset(now_ms, Some(beat_position), fixture_count, spec, gap_speed);
                    break;
                }
            }
        } else {
            let off_duration = ((spec.off_time_ms.max(1) as f64) * gap_speed)
                .round()
                .max(1.0) as u64;
            let on_duration = spec.on_time_ms.max(1);
            loop {
                let duration = match self.phase {
                    FlashPhase::Off => off_duration,
                    FlashPhase::On => on_duration,
                };
                if now_ms.saturating_sub(self.phase_started_ms) < duration {
                    break;
                }
                self.phase_started_ms = self.phase_started_ms.saturating_add(duration);
                self.advance_phase();
                transitions += 1;
                if transitions >= 10_000 {
                    self.reset(now_ms, None, fixture_count, spec, gap_speed);
                    break;
                }
            }
        }

        if self.phase == FlashPhase::On {
            self.window_order
                .get(self.window_cursor)
                .and_then(|window_index| self.windows.get(*window_index))
                .map(Vec::as_slice)
        } else {
            None
        }
    }
}

fn animation_cycle_duration_ms(
    tempo_period_ms: Option<f64>,
    spec: &AnimationSpec,
    animation_speed: f64,
    scene_speed: f64,
) -> Option<f64> {
    let AnimationSpecBody::Phaser(body) = &spec.body else {
        return None;
    };
    let base_duration = match body.time_total {
        PhaserDuration::Fixed(milliseconds) => milliseconds as f64,
        PhaserDuration::Beat(beats) => tempo_period_ms? * beats.as_float(),
    };
    let duration = base_duration * animation_speed * scene_speed;
    duration
        .is_finite()
        .then_some(duration)
        .filter(|value| *value > 0.0)
}

/// The absolute output of a layer: phasers, and the hidden compatibility
/// variants that replay the pre-modulation audio modes bit for bit.
///
/// Returns `None` for layers that contribute through the scale/offset path
/// instead of replacing the base value.
fn generate_absolute_value(
    audio_snapshot: &CollectorOutput,
    spec: &AnimationSpec,
    fixture_phase: f64,
    fixture_index_in_selection: usize,
    fixtures_in_selection: usize,
    palettes: &BTreeMap<u8, Palette>,
) -> Option<u16> {
    match &spec.body {
        AnimationSpecBody::Phaser(body) => Some(phaser::generate(
            body,
            fixture_phase,
            spec.property,
            palettes,
        )),
        AnimationSpecBody::AudioModulation(modulation) => {
            if !modulation.blend.is_compatibility() {
                return None;
            }
            Some(legacy_absolute_value(
                audio_snapshot,
                &modulation.signal,
                fixture_index_in_selection,
                fixtures_in_selection,
            ))
        }
        // Pre-migration bodies. Loading rewrites these, so reaching them
        // means an in-memory spec that never went through a showfile.
        AnimationSpecBody::AudioVolume(_) => Some(audio_snapshot.snapshot.volume as u16),
        AnimationSpecBody::BPMValue(_) => Some(audio_snapshot.snapshot.bpm as u16),
        AnimationSpecBody::AudioFrequencies(freqs) => Some(legacy_spectrum_value(
            audio_snapshot,
            freqs,
            fixture_index_in_selection,
            fixtures_in_selection,
        )),
        AnimationSpecBody::AudioBeat(_) => Some(audio_snapshot.snapshot.bass as u16),
        AnimationSpecBody::BeatClock(_) => {
            Some((audio_snapshot.snapshot.beat_trigger as u16) * 255)
        }
        AnimationSpecBody::Wasm(_) => Some(0),
        AnimationSpecBody::WasmPlugin(_) => None,
        // Flash animations are generated once per selection in `tick`, not
        // independently from each fixture's phase.
        AnimationSpecBody::FlashAnimation(_) => None,
    }
}

fn legacy_absolute_value(
    audio_snapshot: &CollectorOutput,
    signal: &AudioModulationSignal,
    fixture_index_in_selection: usize,
    fixtures_in_selection: usize,
) -> u16 {
    match signal {
        AudioModulationSignal::LegacyVolume => audio_snapshot.snapshot.volume as u16,
        AudioModulationSignal::LegacyBpm => audio_snapshot.snapshot.bpm as u16,
        AudioModulationSignal::LegacyBass => audio_snapshot.snapshot.bass as u16,
        AudioModulationSignal::LegacyBeatClock => {
            (audio_snapshot.snapshot.beat_trigger as u16) * 255
        }
        AudioModulationSignal::LegacySpectrum(freqs) => legacy_spectrum_value(
            audio_snapshot,
            freqs,
            fixture_index_in_selection,
            fixtures_in_selection,
        ),
        // A non-legacy signal paired with the absolute output is not a
        // combination the editor can produce.
        AudioModulationSignal::Energy
        | AudioModulationSignal::Band { .. }
        | AudioModulationSignal::BeatPulse => 0,
    }
}

/// Faithful replay of the old `AudioFrequencies` mode, including its
/// index-based bin-to-frequency mapping and its spatial spread of buckets
/// across the selection.
fn legacy_spectrum_value(
    audio_snapshot: &CollectorOutput,
    freqs: &blaulicht_shared::AnimationSpecBodyFrequencies,
    fixture_index_in_selection: usize,
    fixtures_in_selection: usize,
) -> u16 {
    if audio_snapshot.current_audio_colunn.is_empty() || fixtures_in_selection == 0 {
        return 0;
    }

    const MAX_FREQ_HZ: f32 = 20_000.0;
    let freq_min = (freqs.freq_min.min(freqs.freq_max) as f32).clamp(0.0, MAX_FREQ_HZ);
    let freq_max = (freqs.freq_min.max(freqs.freq_max) as f32).clamp(0.0, MAX_FREQ_HZ);
    let denom = (audio_snapshot.current_audio_colunn.len() - 1).max(1) as f32;
    let gate_threshold = freqs.gate as f32;
    let valid = audio_snapshot
        .current_audio_colunn
        .iter()
        .enumerate()
        .filter(|(idx, bucket)| {
            let bin_freq = (*idx as f32 / denom) * MAX_FREQ_HZ;
            bin_freq >= freq_min && bin_freq <= freq_max && bucket.volume as f32 >= gate_threshold
        })
        .count();
    if valid == 0 {
        return 0;
    }
    let target = fixture_index_in_selection * valid / fixtures_in_selection;
    let mut seen = 0;
    for (idx, bucket) in audio_snapshot.current_audio_colunn.iter().enumerate() {
        let bin_freq = (idx as f32 / denom) * MAX_FREQ_HZ;
        if bin_freq < freq_min || bin_freq > freq_max || (bucket.volume as f32) < gate_threshold {
            continue;
        }
        if seen == target {
            return (bucket.volume as u16 + freqs.boost as u16).min(u8::MAX as u16);
        }
        seen += 1;
    }
    0
}

impl DmxEngine {
    pub fn animation_tick(&mut self, audio_snapshot: &CollectorOutput) {
        let now_ms = self.start_time.elapsed().as_millis().min(u64::MAX as u128) as u64;
        let wasm_outputs = self
            .state_ref
            .wasm_animation_outputs
            .read()
            .unwrap()
            .clone();
        let mut state = self.state_ref.dmx_engine.write().unwrap();
        self.animation_clock
            .tick_with_wasm(now_ms, &mut state.0, audio_snapshot, &wasm_outputs);
    }
}

impl AnimationClockRuntime {
    /// Advances every enabled animation in `state` and rebuilds this frame's
    /// transient output. Authored `fixture_states` are only read, never written
    /// — the write borrow is for the per-fixture phase timers.
    pub(crate) fn tick_with_wasm(
        &mut self,
        now_ms: u64,
        state: &mut blaulicht_shared::EngineState,
        audio_snapshot: &CollectorOutput,
        wasm_outputs: &HashMap<
            crate::state::WasmAnimationInstanceKey,
            blaulicht_shared::AnimationTickOutput,
        >,
    ) {
        self.beat_clock.update(now_ms, &audio_snapshot.snapshot);
        let beat_position = self.beat_clock.beat_position;
        let tempo_period_ms = self.beat_clock.period_ms;
        let generation = self.begin_tick();
        let palettes_snapshot = state.palettes.clone();

        for (scene_id, scene) in state.scenes.iter_mut() {
            let scene_speed = scene.sink.master_speed.as_float();
            for (selection, scene_animations) in scene.sink.active_animations.iter_mut() {
                let mut selection_hasher = DefaultHasher::new();
                selection.fixtures.hash(&mut selection_hasher);
                let selection_fingerprint = selection_hasher.finish();
                for (animation_id, animation) in scene_animations.iter_mut() {
                    if !animation.enabled {
                        continue;
                    }

                    self.fixture_scratch.clear();
                    self.fixture_scratch.extend(
                        selection
                            .fixtures
                            .iter()
                            .copied()
                            .filter(|key| animation.fixture_timers.contains_key(key)),
                    );
                    let fixture_count = self.fixture_scratch.len();
                    if fixture_count == 0 {
                        continue;
                    }

                    if matches!(animation.spec_cloned.body, AnimationSpecBody::WasmPlugin(_)) {
                        self.fixture_scratch.sort_unstable();
                        let instance_key = crate::state::WasmAnimationInstanceKey {
                            scene_id: *scene_id,
                            animation_id: *animation_id,
                            fixtures: self.fixture_scratch.clone(),
                        };
                        if let Some(output) = wasm_outputs.get(&instance_key) {
                            for write in &output.writes {
                                let Some(target_fixture) = self
                                    .fixture_scratch
                                    .get(write.fixture_index as usize)
                                    .copied()
                                else {
                                    continue;
                                };
                                let Some(fixture_state) =
                                    scene.sink.fixture_states.get(&target_fixture)
                                else {
                                    continue;
                                };
                                if fixture_state.slot(write.property).is_frozen() {
                                    continue;
                                }
                                let maximum = if write.property == FixtureProperty::ColorHue {
                                    360
                                } else {
                                    255
                                };
                                self.outputs
                                    .entry(ModulationTarget {
                                        scene_id: *scene_id,
                                        fixture: target_fixture,
                                        property: write.property,
                                    })
                                    .absolute = Some(write.value.min(maximum));
                            }
                        }
                        continue;
                    }

                    if let AnimationSpecBody::FlashAnimation(spec) = &animation.spec_cloned.body {
                        self.fixture_scratch.sort_unstable();
                        let layer_key = AnimationLayerKey {
                            scene_id: *scene_id,
                            selection_fingerprint,
                            animation_id: *animation_id,
                        };
                        let runtime = self.flash_layers.entry(layer_key).or_default();
                        runtime.seen_generation = generation;
                        let gap_speed = animation.speed_factor.as_float() * scene_speed;
                        let needs_reset = animation
                            .fixture_timers
                            .values()
                            .any(|timer| timer.last_tick_time == 0);
                        if needs_reset {
                            runtime.reset(
                                now_ms,
                                tempo_period_ms.map(|_| beat_position),
                                fixture_count,
                                spec,
                                gap_speed,
                            );
                        }
                        let active_fixture_indices = runtime
                            .tick(
                                now_ms,
                                tempo_period_ms.map(|_| beat_position),
                                fixture_count,
                                spec,
                                gap_speed,
                            )
                            .map(<[usize]>::to_vec)
                            .unwrap_or_default();
                        for timer in animation.fixture_timers.values_mut() {
                            timer.last_tick_time = now_ms.max(1);
                            timer.timer = now_ms.saturating_sub(runtime.phase_started_ms);
                            timer.needs_reset_on_beat = false;
                        }
                        let property = animation.spec_cloned.property;
                        let min = spec.amplitude_min.resolve(&palettes_snapshot, property);
                        let max = spec.amplitude_max.resolve(&palettes_snapshot, property);

                        for fixture_index in 0..fixture_count {
                            let target_fixture = self.fixture_scratch[fixture_index];
                            if !scene.sink.fixture_states.contains_key(&target_fixture) {
                                continue;
                            }
                            let value = if active_fixture_indices.contains(&fixture_index) {
                                max
                            } else {
                                min
                            };
                            self.outputs
                                .entry(ModulationTarget {
                                    scene_id: *scene_id,
                                    fixture: target_fixture,
                                    property,
                                })
                                .absolute = Some(value);
                        }
                        continue;
                    }

                    let cycle_duration = animation_cycle_duration_ms(
                        tempo_period_ms,
                        &animation.spec_cloned,
                        animation.speed_factor.as_float(),
                        scene_speed,
                    );
                    let pinned_beats = match &animation.spec_cloned.body {
                        AnimationSpecBody::Phaser(body) if body.pin_to_beat => {
                            match body.time_total {
                                PhaserDuration::Beat(beats) => Some(
                                    beats.as_float()
                                        * animation.speed_factor.as_float()
                                        * scene_speed,
                                ),
                                PhaserDuration::Fixed(_) => None,
                            }
                        }
                        _ => None,
                    };

                    self.phase_scratch.clear();
                    let mut representative_crossings = 0_u64;
                    for fixture_index in 0..fixture_count {
                        let fixture_key = self.fixture_scratch[fixture_index];
                        let key = AnimationRuntimeKey {
                            scene_id: *scene_id,
                            selection_fingerprint,
                            animation_id: *animation_id,
                            fixture: fixture_key,
                        };
                        let timer = animation.fixture_timers.get_mut(&fixture_key).unwrap();
                        let runtime = self.fixture_phases.entry(key).or_default();
                        runtime.seen_generation = generation;
                        let was_reset = timer.last_tick_time == 0;
                        if was_reset {
                            runtime.reset(now_ms, timer.timer as f64);
                        }

                        let mut crossings = if let Some(beats_per_cycle) = pinned_beats {
                            runtime.align_to_beat_clock(now_ms, beat_position, beats_per_cycle)
                        } else {
                            runtime.advance_local(now_ms, cycle_duration)
                        };
                        if was_reset {
                            crossings = 0;
                        }
                        // Aggregate over the selection: taking only the last
                        // fixture's value could miss a cycle crossing when
                        // per-fixture phase offsets straddle the boundary.
                        representative_crossings = representative_crossings.max(crossings);
                        timer.timer = runtime.phase_degrees.floor().max(0.0) as u64;
                        timer.last_tick_time = now_ms.max(1);
                        timer.needs_reset_on_beat = false;
                        self.phase_scratch.push(runtime.phase_degrees);
                    }

                    if representative_crossings > 0 {
                        let previous_count = animation.iteration_count;
                        animation.iteration_count = animation
                            .iteration_count
                            .saturating_add(representative_crossings.min(u32::MAX as u64) as u32);
                        if let AnimationSpecBody::Phaser(phaser) = &animation.spec_cloned.body {
                            if let Some(n) = phaser.reverse_after_n_iterations.filter(|n| *n > 0) {
                                let reversals = animation.iteration_count / n - previous_count / n;
                                if reversals % 2 == 1 {
                                    animation.reversed = !animation.reversed;
                                }
                            }
                        }
                    }

                    // Continuous audio layers produce one uniform envelope for
                    // the whole selection, so they tick once per animation.
                    let modulation_contribution = match &animation.spec_cloned.body {
                        AnimationSpecBody::AudioModulation(spec)
                            if !spec.blend.is_compatibility() =>
                        {
                            let layer_key = AnimationLayerKey {
                                scene_id: *scene_id,
                                selection_fingerprint,
                                animation_id: *animation_id,
                            };
                            let runtime = self.modulation_layers.entry(layer_key).or_default();
                            runtime.seen_generation = generation;
                            let sample = runtime.tick(now_ms, spec, audio_snapshot);
                            audio::contribution(&spec.blend, sample.envelope)
                        }
                        _ => None,
                    };

                    let property = animation.spec_cloned.property;
                    for fixture_index in 0..fixture_count {
                        let fixture_phase = self.phase_scratch[fixture_index];
                        let target_index = if animation.reversed {
                            fixture_count - 1 - fixture_index
                        } else {
                            fixture_index
                        };
                        let target_fixture = self.fixture_scratch[target_index];
                        if !scene.sink.fixture_states.contains_key(&target_fixture) {
                            continue;
                        }
                        let target = ModulationTarget {
                            scene_id: *scene_id,
                            fixture: target_fixture,
                            property,
                        };

                        match modulation_contribution {
                            Some(AudioModulationContribution::Scale(factor)) => {
                                self.outputs.entry(target).scale *= factor;
                            }
                            Some(AudioModulationContribution::Add(offset)) => {
                                self.outputs.entry(target).add += offset;
                            }
                            None => {
                                // Phaser and compatibility-absolute layers keep
                                // the ascending animation-ID / last-wins order.
                                let Some(value) = generate_absolute_value(
                                    audio_snapshot,
                                    &animation.spec_cloned,
                                    fixture_phase,
                                    fixture_index,
                                    fixture_count,
                                    &palettes_snapshot,
                                ) else {
                                    continue;
                                };
                                self.outputs.entry(target).absolute = Some(value);
                            }
                        }
                    }
                }
            }
        }
        self.retain_current_generation();
    }

    #[cfg(test)]
    fn tick(
        &mut self,
        now_ms: u64,
        state: &mut blaulicht_shared::EngineState,
        audio_snapshot: &CollectorOutput,
    ) {
        self.tick_with_wasm(now_ms, state, audio_snapshot, &HashMap::new());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_phase_uses_actual_elapsed_time_for_all_cycle_lengths() {
        for cycle_ms in [500.0, 1_000.0, 2_000.0, 4_000.0, 8_000.0, 10_000.0] {
            let mut phase = FixturePhaseRuntime::default();
            phase.reset(0, 0.0);
            phase.advance_local(cycle_ms as u64, Some(cycle_ms));
            assert!((phase.phase_degrees - 360.0).abs() < 0.001, "{cycle_ms}");
        }
    }

    #[test]
    fn scheduler_stall_crosses_iterations_without_catch_up_loop() {
        let mut phase = FixturePhaseRuntime::default();
        phase.reset(1_000, 0.0);
        let crossings = phase.advance_local(6_000, Some(500.0));
        assert_eq!(crossings, 10);
        assert!((phase.phase_degrees - 3_600.0).abs() < 0.001);
    }

    #[test]
    fn duration_change_preserves_current_phase() {
        let mut phase = FixturePhaseRuntime::default();
        phase.reset(0, 0.0);
        phase.advance_local(250, Some(1_000.0));
        assert!((phase.phase_degrees - 90.0).abs() < 0.001);
        phase.advance_local(500, Some(500.0));
        assert!((phase.phase_degrees - 270.0).abs() < 0.001);
    }

    #[test]
    fn missing_duration_pauses_phase() {
        let mut phase = FixturePhaseRuntime::default();
        phase.reset(100, 45.0);
        phase.advance_local(1_000, None);
        assert_eq!(phase.phase_degrees, 45.0);
        phase.advance_local(1_500, Some(500.0));
        assert_eq!(phase.phase_degrees, 405.0);
    }

    #[test]
    fn musical_clock_aligns_to_events_and_pauses_without_tempo() {
        let mut clock = MusicalBeatClock::default();
        let mut audio = blaulicht_shared::CollectedAudioSnapshot {
            time_between_beats_millis: 500,
            source_status: AudioSourceStatus::Active,
            ..Default::default()
        };
        clock.update(0, &audio);
        clock.update(250, &audio);
        assert!((clock.beat_position - 0.5).abs() < 0.001);

        audio.beat_trigger = true;
        audio.beat_event_id = 1;
        clock.update(475, &audio);
        assert!((clock.beat_position - 0.95).abs() < 0.001);
        assert!((clock.pending_phase_correction - 0.05).abs() < 0.001);

        audio.beat_trigger = false;
        audio.source_status = AudioSourceStatus::Disconnected;
        clock.update(500, &audio);
        let paused = clock.beat_position;
        clock.update(1_500, &audio);
        assert_eq!(clock.beat_position, paused);
    }

    #[test]
    fn musical_clock_slews_tempo_without_a_rate_jump() {
        let mut clock = MusicalBeatClock::default();
        let mut audio = blaulicht_shared::CollectedAudioSnapshot {
            time_between_beats_millis: 500,
            source_status: AudioSourceStatus::Active,
            ..Default::default()
        };
        clock.update(0, &audio);
        audio.time_between_beats_millis = 400;
        clock.update(100, &audio);

        assert!((clock.period_ms.unwrap() - 498.0).abs() < 0.001);
        assert!((clock.beat_position - 0.2).abs() < 0.001);
    }

    #[test]
    fn musical_clock_keeps_period_for_half_and_double_time_inputs() {
        let mut clock = MusicalBeatClock::default();
        let mut audio = blaulicht_shared::CollectedAudioSnapshot {
            time_between_beats_millis: 500,
            source_status: AudioSourceStatus::Active,
            ..Default::default()
        };
        clock.update(0, &audio);

        audio.time_between_beats_millis = 1_000;
        clock.update(100, &audio);
        assert_eq!(clock.period_ms, Some(500.0));

        audio.time_between_beats_millis = 250;
        clock.update(200, &audio);
        assert_eq!(clock.period_ms, Some(500.0));
    }

    #[test]
    fn beat_phase_correction_is_smooth_and_monotonic() {
        let mut clock = MusicalBeatClock::default();
        let mut audio = blaulicht_shared::CollectedAudioSnapshot {
            time_between_beats_millis: 500,
            source_status: AudioSourceStatus::Active,
            ..Default::default()
        };
        clock.update(0, &audio);
        clock.update(600, &audio);
        assert!((clock.beat_position - 1.2).abs() < 0.001);

        audio.beat_trigger = true;
        audio.beat_event_id = 1;
        clock.update(600, &audio);
        let before_correction = clock.beat_position;
        audio.beat_trigger = false;
        clock.update(700, &audio);

        assert!(clock.beat_position > before_correction);
        assert!(clock.beat_position < before_correction + 0.2);
    }

    #[test]
    fn pinned_phase_uses_global_beat_position_and_fixture_offset() {
        let mut phase = FixturePhaseRuntime::default();
        phase.reset(0, 90.0);
        phase.align_to_beat_clock(500, 2.0, 2.0);
        assert_eq!(phase.phase_degrees, 450.0);
    }

    #[test]
    fn flash_windows_partition_contiguously_without_repeats() {
        assert_eq!(
            flash_windows(5, 2, FlashWindowLayout::Contiguous),
            vec![vec![0, 1], vec![2, 3], vec![4]]
        );
        assert_eq!(
            flash_windows(3, 99, FlashWindowLayout::Contiguous),
            vec![vec![0, 1, 2]]
        );
    }

    #[test]
    fn flash_windows_space_opposite_fixtures_and_keep_a_smaller_remainder() {
        assert_eq!(
            flash_windows(8, 2, FlashWindowLayout::Spaced),
            vec![vec![0, 4], vec![1, 5], vec![2, 6], vec![3, 7]]
        );
        assert_eq!(
            flash_windows(5, 2, FlashWindowLayout::Spaced),
            vec![vec![0, 3], vec![1, 4], vec![2]]
        );
    }

    #[test]
    fn flash_runtime_starts_dark_and_keeps_on_time_independent_of_gap_speed() {
        let spec = FlashAnimationSpec {
            off_time_ms: 100,
            on_time_ms: 50,
            window_size: 1,
            ..Default::default()
        };
        let mut runtime = FlashAnimationRuntime::default();

        assert_eq!(runtime.tick(0, None, 2, &spec, 2.0), None);
        assert_eq!(runtime.tick(199, None, 2, &spec, 2.0), None);
        assert_eq!(runtime.tick(200, None, 2, &spec, 2.0), Some(&[0][..]));
        assert_eq!(runtime.tick(249, None, 2, &spec, 2.0), Some(&[0][..]));
        assert_eq!(runtime.tick(250, None, 2, &spec, 2.0), None);
        assert_eq!(runtime.tick(449, None, 2, &spec, 2.0), None);
        assert_eq!(runtime.tick(450, None, 2, &spec, 2.0), Some(&[1][..]));
    }

    #[test]
    fn random_flash_sweeps_are_permutations_without_replacement() {
        let spec = FlashAnimationSpec {
            random_order: true,
            window_size: 1,
            ..Default::default()
        };
        let mut runtime = FlashAnimationRuntime::default();
        runtime.reset(0, None, 8, &spec, 1.0);
        let mut order = runtime.window_order.clone();
        order.sort_unstable();
        assert_eq!(order, (0..8).collect::<Vec<_>>());
    }

    #[test]
    fn beat_aligned_flash_uses_sub_beat_boundaries_and_waits_for_tempo() {
        let spec = FlashAnimationSpec {
            beat_aligned: true,
            off_time_beats: blaulicht_shared::AnimationSpeedModifier::_2,
            on_time_beats: blaulicht_shared::AnimationSpeedModifier::_4,
            window_size: 1,
            ..Default::default()
        };
        let mut runtime = FlashAnimationRuntime::default();

        assert_eq!(runtime.tick(0, None, 2, &spec, 1.0), None);
        assert_eq!(runtime.tick(0, Some(0.10), 2, &spec, 1.0), None);
        assert_eq!(runtime.tick(0, Some(0.49), 2, &spec, 1.0), None);
        assert_eq!(runtime.tick(0, Some(0.50), 2, &spec, 1.0), Some(&[0][..]));
        assert_eq!(runtime.tick(0, Some(0.74), 2, &spec, 1.0), Some(&[0][..]));
        assert_eq!(runtime.tick(0, Some(0.75), 2, &spec, 1.0), None);
        assert_eq!(runtime.tick(0, Some(1.25), 2, &spec, 1.0), Some(&[1][..]));
    }

    #[test]
    fn sequential_flash_reverses_after_configured_sweeps() {
        let spec = FlashAnimationSpec {
            beat_aligned: true,
            off_time_beats: blaulicht_shared::AnimationSpeedModifier::_4,
            on_time_beats: blaulicht_shared::AnimationSpeedModifier::_4,
            window_size: 1,
            reverse_after_n_iterations: Some(1),
            ..Default::default()
        };
        let mut runtime = FlashAnimationRuntime::default();

        assert_eq!(runtime.tick(0, Some(0.0), 3, &spec, 1.0), None);
        assert_eq!(runtime.tick(0, Some(0.25), 3, &spec, 1.0), Some(&[0][..]));
        assert_eq!(runtime.tick(0, Some(0.75), 3, &spec, 1.0), Some(&[1][..]));
        assert_eq!(runtime.tick(0, Some(1.25), 3, &spec, 1.0), Some(&[2][..]));
        assert_eq!(runtime.tick(0, Some(1.75), 3, &spec, 1.0), Some(&[2][..]));
    }

    #[test]
    fn intentional_beat_division_mapping_is_unchanged() {
        use blaulicht_shared::AnimationSpeedModifier;

        assert_eq!(AnimationSpeedModifier::_2.as_float(), 0.5);
        assert_eq!(AnimationSpeedModifier::_1_16.as_float(), 16.0);
    }
}

#[cfg(test)]
mod modulation_tests {
    use super::*;
    use blaulicht_audio_engine::AudioBucket;
    use blaulicht_shared::{
        fixture::{light::Light, state::Fixture, FixtureType},
        scene::FixtureSelection,
        ActiveAnimation, AnimationSpecBodyFrequencies, AnimationSpecBodyPhaser,
        AudioModulationBlend, AudioModulationShaping, AudioModulationSpec, AudioSourceStatus,
        CollectedAudioSnapshot, MathematicalBaseFunction, MathematicalPhaser, PhaserKind, SyncMode,
    };

    const SCENE: u8 = 0;
    const FIXTURE_A: (u8, u8) = (0, 0);
    const FIXTURE_B: (u8, u8) = (0, 1);

    fn engine_with_two_fixtures() -> blaulicht_shared::EngineState {
        let mut state = blaulicht_shared::EngineState::default();
        let mut group = blaulicht_shared::fixture::state::FixtureGroup {
            name: "Group".to_string(),
            fixtures: BTreeMap::new(),
        };
        for (index, name) in ["A", "B"].into_iter().enumerate() {
            group.fixtures.insert(
                index as u8,
                Fixture::new(
                    0,
                    index + 1,
                    name.to_string(),
                    FixtureType::from(Light::Generic3ChanNoAlpha),
                ),
            );
        }
        state.groups.insert(0, group);

        // Replace the default scene with one sized to this group.
        state.scenes.clear();
        state.scenes.insert(
            SCENE,
            blaulicht_shared::scene::Scene {
                name: "Scene".to_string(),
                sink: blaulicht_shared::scene::EngineSink::from_groups(&state.groups),
            },
        );
        state.current_scene_focus = SCENE;
        state
    }

    fn add_animation(
        state: &mut blaulicht_shared::EngineState,
        scene_id: u8,
        animation_id: u8,
        fixtures: Vec<(u8, u8)>,
        spec: AnimationSpec,
    ) {
        let selection = FixtureSelection { fixtures };
        let mut animation = ActiveAnimation::new(&selection.fixtures, spec);
        animation.enabled = true;
        state
            .scenes
            .get_mut(&scene_id)
            .unwrap()
            .sink
            .active_animations
            .entry(selection)
            .or_default()
            .insert(animation_id, animation);
    }

    fn modulation_spec(
        signal: AudioModulationSignal,
        blend: AudioModulationBlend,
    ) -> AnimationSpec {
        AnimationSpec {
            name: "Layer".to_string(),
            property: FixtureProperty::Alpha,
            body: AnimationSpecBody::AudioModulation(AudioModulationSpec {
                signal,
                // Instant response keeps these tests about composition, not smoothing.
                shaping: AudioModulationShaping {
                    attack_ms: 0,
                    release_ms: 0,
                    ..Default::default()
                },
                blend,
            }),
        }
    }

    #[test]
    fn wasm_animation_writes_multiple_properties_and_clamps_values() {
        let mut state = engine_with_two_fixtures();
        let fixtures = vec![FIXTURE_A, FIXTURE_B];
        add_animation(
            &mut state,
            SCENE,
            7,
            fixtures.clone(),
            AnimationSpec {
                name: "WASM".to_string(),
                property: FixtureProperty::Alpha,
                body: AnimationSpecBody::WasmPlugin(blaulicht_shared::WasmAnimationSpec {
                    plugin_key: "test.animation".to_string(),
                }),
            },
        );
        let key = crate::state::WasmAnimationInstanceKey {
            scene_id: SCENE,
            animation_id: 7,
            fixtures,
        };
        let wasm = HashMap::from([(
            key,
            blaulicht_shared::AnimationTickOutput {
                writes: vec![
                    blaulicht_shared::AnimationPropertyWrite {
                        fixture_index: 0,
                        property: FixtureProperty::Alpha,
                        value: 999,
                    },
                    blaulicht_shared::AnimationPropertyWrite {
                        fixture_index: 1,
                        property: FixtureProperty::ColorHue,
                        value: 999,
                    },
                ],
            },
        )]);
        let mut runtime = AnimationClockRuntime::default();
        runtime.tick_with_wasm(25, &mut state, &CollectorOutput::default(), &wasm);

        assert_eq!(
            runtime
                .outputs
                .get(&ModulationTarget {
                    scene_id: SCENE,
                    fixture: FIXTURE_A,
                    property: FixtureProperty::Alpha,
                })
                .unwrap()
                .absolute,
            Some(255)
        );
        assert_eq!(
            runtime
                .outputs
                .get(&ModulationTarget {
                    scene_id: SCENE,
                    fixture: FIXTURE_B,
                    property: FixtureProperty::ColorHue,
                })
                .unwrap()
                .absolute,
            Some(360)
        );
    }

    fn phaser_spec(property: FixtureProperty, min: u16, max: u16) -> AnimationSpec {
        AnimationSpec {
            name: "Phaser".to_string(),
            property,
            body: AnimationSpecBody::Phaser(AnimationSpecBodyPhaser {
                kind: PhaserKind::Mathematical(MathematicalPhaser {
                    base: MathematicalBaseFunction::Square1_2,
                    stretch_factor: 1.0,
                    amplitude_min: FixtureValue::Literal(min),
                    amplitude_max: FixtureValue::Literal(max),
                }),
                time_total: PhaserDuration::Fixed(1_000),
                pin_to_beat: false,
                sync: SyncMode::Synced,
                reverse_after_n_iterations: None,
            }),
        }
    }

    fn loud_audio() -> CollectorOutput {
        CollectorOutput {
            snapshot: CollectedAudioSnapshot {
                source_status: AudioSourceStatus::Active,
                volume: 255,
                bass: 200,
                bpm: 128.0,
                beat_trigger: true,
                ..Default::default()
            },
            debug_data: Default::default(),
            current_audio_colunn: vec![
                AudioBucket {
                    volume: 255,
                    freq_bound_lower: 0,
                    freq_bound_upper: 200,
                },
                AudioBucket {
                    volume: 0,
                    freq_bound_lower: 200,
                    freq_bound_upper: 20_000,
                },
            ],
        }
    }

    fn silent_disconnected_audio() -> CollectorOutput {
        CollectorOutput {
            snapshot: CollectedAudioSnapshot {
                source_status: AudioSourceStatus::Disconnected,
                ..Default::default()
            },
            debug_data: Default::default(),
            current_audio_colunn: Vec::new(),
        }
    }

    fn output_for(
        clock: &AnimationClockRuntime,
        fixture: (u8, u8),
        property: FixtureProperty,
    ) -> PropertyModulation {
        clock
            .outputs
            .get(&ModulationTarget {
                scene_id: SCENE,
                fixture,
                property,
            })
            .copied()
            .unwrap_or_default()
    }

    /// Resolves a fixture the way `render_universes` does.
    fn rendered_alpha(
        clock: &AnimationClockRuntime,
        state: &blaulicht_shared::EngineState,
        scene_id: u8,
        fixture: (u8, u8),
    ) -> u16 {
        let mut fixture_state = state.scenes[&scene_id].sink.fixture_states[&fixture].clone();
        clock
            .outputs
            .apply_to_fixture(&mut fixture_state, scene_id, fixture, &state.palettes);
        fixture_state.resolved_value(FixtureProperty::Alpha, &state.palettes)
    }

    fn set_alpha(state: &mut blaulicht_shared::EngineState, fixture: (u8, u8), value: u16) {
        state
            .scenes
            .get_mut(&SCENE)
            .unwrap()
            .sink
            .fixture_states
            .get_mut(&fixture)
            .unwrap()
            .alpha = FixtureValue::Literal(value);
    }

    #[test]
    fn animation_tick_leaves_authored_fixture_state_untouched() {
        let mut state = engine_with_two_fixtures();
        set_alpha(&mut state, FIXTURE_A, 100);
        add_animation(
            &mut state,
            SCENE,
            0,
            vec![FIXTURE_A],
            modulation_spec(
                AudioModulationSignal::Energy,
                AudioModulationBlend::Add { depth: 100.0 },
            ),
        );

        let authored_before = state.clone();
        let mut clock = AnimationClockRuntime::default();
        clock.tick(0, &mut state, &loud_audio());

        assert_eq!(
            state.scenes[&SCENE].sink.fixture_states[&FIXTURE_A].alpha,
            authored_before.scenes[&SCENE].sink.fixture_states[&FIXTURE_A].alpha
        );
        // ...while the transient output does change.
        assert_eq!(rendered_alpha(&clock, &state, SCENE, FIXTURE_A), 200);
    }

    #[test]
    fn palette_bound_state_is_read_as_base_and_left_bound() {
        use blaulicht_shared::palette::{Palette, PaletteKind};

        let mut state = engine_with_two_fixtures();
        state.palettes.insert(
            1,
            Palette {
                name: "Dim".to_string(),
                kind: PaletteKind::Single(FixtureProperty::Alpha, 80),
            },
        );
        {
            let sink = &mut state.scenes.get_mut(&SCENE).unwrap().sink;
            sink.palette_assignments.insert(FIXTURE_A, vec![1]);
            let palettes = state.palettes.clone();
            sink.sync_palette_bindings(&palettes);
        }

        add_animation(
            &mut state,
            SCENE,
            0,
            vec![FIXTURE_A],
            modulation_spec(
                AudioModulationSignal::Energy,
                AudioModulationBlend::Scale {
                    peak_percent: 100.0,
                },
            ),
        );

        let mut clock = AnimationClockRuntime::default();
        clock.tick(0, &mut state, &loud_audio());

        // The slot is still a palette pointer after the tick.
        assert!(matches!(
            state.scenes[&SCENE].sink.fixture_states[&FIXTURE_A].alpha,
            FixtureValue::PalettePointer { palette_id: 1, .. }
        ));
        // The palette value is what the layer scales.
        assert_eq!(rendered_alpha(&clock, &state, SCENE, FIXTURE_A), 80);
    }

    #[test]
    fn additive_layers_sum_and_scale_layers_multiply() {
        let mut state = engine_with_two_fixtures();
        set_alpha(&mut state, FIXTURE_A, 100);

        add_animation(
            &mut state,
            SCENE,
            0,
            vec![FIXTURE_A],
            modulation_spec(
                AudioModulationSignal::Energy,
                AudioModulationBlend::Add { depth: 30.0 },
            ),
        );
        add_animation(
            &mut state,
            SCENE,
            1,
            vec![FIXTURE_A],
            modulation_spec(
                AudioModulationSignal::Energy,
                AudioModulationBlend::Add { depth: 20.0 },
            ),
        );
        add_animation(
            &mut state,
            SCENE,
            2,
            vec![FIXTURE_A],
            modulation_spec(
                AudioModulationSignal::Energy,
                AudioModulationBlend::Scale {
                    peak_percent: 200.0,
                },
            ),
        );

        let mut clock = AnimationClockRuntime::default();
        clock.tick(0, &mut state, &loud_audio());

        let modulation = output_for(&clock, FIXTURE_A, FixtureProperty::Alpha);
        assert_eq!(modulation.add, 50.0);
        assert_eq!(modulation.scale, 2.0);
        // base 100 * 2 + 50
        assert_eq!(rendered_alpha(&clock, &state, SCENE, FIXTURE_A), 250);
    }

    #[test]
    fn phaser_output_is_the_base_that_modulation_then_scales() {
        let mut state = engine_with_two_fixtures();
        set_alpha(&mut state, FIXTURE_A, 10);

        // Square1_2 at phase 0 sits at its maximum.
        add_animation(
            &mut state,
            SCENE,
            0,
            vec![FIXTURE_A],
            phaser_spec(FixtureProperty::Alpha, 0, 200),
        );
        add_animation(
            &mut state,
            SCENE,
            1,
            vec![FIXTURE_A],
            modulation_spec(
                AudioModulationSignal::Energy,
                AudioModulationBlend::Scale { peak_percent: 50.0 },
            ),
        );

        let mut clock = AnimationClockRuntime::default();
        clock.tick(0, &mut state, &loud_audio());

        let modulation = output_for(&clock, FIXTURE_A, FixtureProperty::Alpha);
        assert_eq!(modulation.absolute, Some(200));
        // The phaser output, not the authored 10, is what gets scaled.
        assert_eq!(rendered_alpha(&clock, &state, SCENE, FIXTURE_A), 100);
    }

    #[test]
    fn absolute_layers_keep_ascending_animation_id_last_wins() {
        let mut state = engine_with_two_fixtures();
        add_animation(
            &mut state,
            SCENE,
            9,
            vec![FIXTURE_A],
            phaser_spec(FixtureProperty::Alpha, 0, 50),
        );
        add_animation(
            &mut state,
            SCENE,
            3,
            vec![FIXTURE_A],
            phaser_spec(FixtureProperty::Alpha, 0, 200),
        );

        let mut clock = AnimationClockRuntime::default();
        clock.tick(0, &mut state, &loud_audio());

        // ID 9 is visited last, so its value survives.
        assert_eq!(
            output_for(&clock, FIXTURE_A, FixtureProperty::Alpha).absolute,
            Some(50)
        );
    }

    #[test]
    fn output_is_clamped_per_property() {
        let mut state = engine_with_two_fixtures();
        set_alpha(&mut state, FIXTURE_A, 255);
        add_animation(
            &mut state,
            SCENE,
            0,
            vec![FIXTURE_A],
            modulation_spec(
                AudioModulationSignal::Energy,
                AudioModulationBlend::Add { depth: 300.0 },
            ),
        );

        let mut clock = AnimationClockRuntime::default();
        clock.tick(0, &mut state, &loud_audio());
        assert_eq!(rendered_alpha(&clock, &state, SCENE, FIXTURE_A), 255);

        // Hue keeps its own 0..360 ceiling.
        let hue = PropertyModulation {
            absolute: None,
            scale: 1.0,
            add: 1_000.0,
        };
        assert_eq!(hue.resolve(0, FixtureProperty::ColorHue), 360);
        assert_eq!(hue.resolve(0, FixtureProperty::Alpha), 255);

        // Negative depth cannot drive a property below zero.
        let negative = PropertyModulation {
            absolute: None,
            scale: 1.0,
            add: -500.0,
        };
        assert_eq!(negative.resolve(100, FixtureProperty::Alpha), 0);
    }

    #[test]
    fn band_energy_is_uniform_across_the_selection() {
        let mut state = engine_with_two_fixtures();
        set_alpha(&mut state, FIXTURE_A, 100);
        set_alpha(&mut state, FIXTURE_B, 100);
        add_animation(
            &mut state,
            SCENE,
            0,
            vec![FIXTURE_A, FIXTURE_B],
            modulation_spec(
                AudioModulationSignal::Band {
                    freq_min_hz: 40,
                    freq_max_hz: 180,
                },
                AudioModulationBlend::Add { depth: 100.0 },
            ),
        );

        let mut clock = AnimationClockRuntime::default();
        clock.tick(0, &mut state, &loud_audio());

        let a = output_for(&clock, FIXTURE_A, FixtureProperty::Alpha);
        let b = output_for(&clock, FIXTURE_B, FixtureProperty::Alpha);
        assert_eq!(a.add, b.add);
        assert_eq!(a.add, 100.0);
    }

    #[test]
    fn outputs_are_scoped_per_scene_so_overlays_stay_independent() {
        let mut state = engine_with_two_fixtures();
        state.new_scene("Overlay".to_string());
        let overlay_id = *state.scenes.keys().max().unwrap();
        assert_ne!(overlay_id, SCENE);

        add_animation(
            &mut state,
            overlay_id,
            0,
            vec![FIXTURE_A],
            modulation_spec(
                AudioModulationSignal::Energy,
                AudioModulationBlend::Add { depth: 60.0 },
            ),
        );

        let mut clock = AnimationClockRuntime::default();
        clock.tick(0, &mut state, &loud_audio());

        // The base scene sees nothing; the overlay scene carries the layer.
        assert_eq!(
            output_for(&clock, FIXTURE_A, FixtureProperty::Alpha).add,
            0.0
        );
        assert_eq!(
            clock
                .outputs
                .get(&ModulationTarget {
                    scene_id: overlay_id,
                    fixture: FIXTURE_A,
                    property: FixtureProperty::Alpha,
                })
                .unwrap()
                .add,
            60.0
        );
    }

    #[test]
    fn pausing_and_removing_clears_the_output_on_the_next_frame() {
        let mut state = engine_with_two_fixtures();
        add_animation(
            &mut state,
            SCENE,
            0,
            vec![FIXTURE_A],
            modulation_spec(
                AudioModulationSignal::Energy,
                AudioModulationBlend::Add { depth: 60.0 },
            ),
        );

        let mut clock = AnimationClockRuntime::default();
        clock.tick(0, &mut state, &loud_audio());
        assert_eq!(
            output_for(&clock, FIXTURE_A, FixtureProperty::Alpha).add,
            60.0
        );

        // Pause.
        for animations in state
            .scenes
            .get_mut(&SCENE)
            .unwrap()
            .sink
            .active_animations
            .values_mut()
        {
            animations.get_mut(&0).unwrap().enabled = false;
        }
        clock.tick(16, &mut state, &loud_audio());
        assert!(clock.outputs.is_empty());

        // Remove.
        state
            .scenes
            .get_mut(&SCENE)
            .unwrap()
            .sink
            .active_animations
            .clear();
        clock.tick(32, &mut state, &loud_audio());
        assert!(clock.outputs.is_empty());
    }

    #[test]
    fn audio_dropout_decays_the_layer_to_zero() {
        let mut state = engine_with_two_fixtures();
        set_alpha(&mut state, FIXTURE_A, 200);
        add_animation(
            &mut state,
            SCENE,
            0,
            vec![FIXTURE_A],
            modulation_spec(
                AudioModulationSignal::Energy,
                AudioModulationBlend::Scale {
                    peak_percent: 100.0,
                },
            ),
        );

        let mut clock = AnimationClockRuntime::default();
        clock.tick(0, &mut state, &loud_audio());
        assert_eq!(rendered_alpha(&clock, &state, SCENE, FIXTURE_A), 200);

        clock.tick(16, &mut state, &silent_disconnected_audio());
        assert_eq!(rendered_alpha(&clock, &state, SCENE, FIXTURE_A), 0);
    }

    /// The pre-migration implementation, kept verbatim so the compatibility
    /// path can be compared against it.
    fn legacy_reference_value(
        audio: &CollectorOutput,
        body: &AnimationSpecBody,
        fixture_index: usize,
        fixture_count: usize,
    ) -> u16 {
        match body {
            AnimationSpecBody::AudioVolume(_) => audio.snapshot.volume as u16,
            AnimationSpecBody::BPMValue(_) => audio.snapshot.bpm as u16,
            AnimationSpecBody::AudioBeat(_) => audio.snapshot.bass as u16,
            AnimationSpecBody::BeatClock(_) => (audio.snapshot.beat_trigger as u16) * 255,
            AnimationSpecBody::AudioFrequencies(freqs) => {
                if audio.current_audio_colunn.is_empty() || fixture_count == 0 {
                    return 0;
                }
                const MAX_FREQ_HZ: f32 = 20_000.0;
                let freq_min = (freqs.freq_min.min(freqs.freq_max) as f32).clamp(0.0, MAX_FREQ_HZ);
                let freq_max = (freqs.freq_min.max(freqs.freq_max) as f32).clamp(0.0, MAX_FREQ_HZ);
                let denom = (audio.current_audio_colunn.len() - 1).max(1) as f32;
                let gate = freqs.gate as f32;
                let valid = audio
                    .current_audio_colunn
                    .iter()
                    .enumerate()
                    .filter(|(idx, bucket)| {
                        let bin_freq = (*idx as f32 / denom) * MAX_FREQ_HZ;
                        bin_freq >= freq_min && bin_freq <= freq_max && bucket.volume as f32 >= gate
                    })
                    .count();
                if valid == 0 {
                    return 0;
                }
                let target = fixture_index * valid / fixture_count;
                let mut seen = 0;
                for (idx, bucket) in audio.current_audio_colunn.iter().enumerate() {
                    let bin_freq = (idx as f32 / denom) * MAX_FREQ_HZ;
                    if bin_freq < freq_min || bin_freq > freq_max || (bucket.volume as f32) < gate {
                        continue;
                    }
                    if seen == target {
                        return (bucket.volume as u16 + freqs.boost as u16).min(u8::MAX as u16);
                    }
                    seen += 1;
                }
                0
            }
            other => panic!("not a legacy audio body: {other:?}"),
        }
    }

    #[test]
    fn migrated_legacy_modes_reproduce_their_former_absolute_output() {
        let audio = loud_audio();
        let legacy_bodies = [
            AnimationSpecBody::AudioVolume(Default::default()),
            AnimationSpecBody::BPMValue(Default::default()),
            AnimationSpecBody::AudioBeat(Default::default()),
            AnimationSpecBody::BeatClock(Default::default()),
            AnimationSpecBody::AudioFrequencies(AnimationSpecBodyFrequencies {
                gate: 10,
                boost: 5,
                freq_min: 0,
                freq_max: 20_000,
                normalization: Default::default(),
            }),
        ];

        for legacy in legacy_bodies {
            let mut migrated = legacy.clone();
            assert!(migrated.migrate_legacy_audio(), "{legacy:?}");

            for fixture_index in 0..2 {
                let expected = legacy_reference_value(&audio, &legacy, fixture_index, 2);
                let actual = generate_absolute_value(
                    &audio,
                    &AnimationSpec {
                        name: "x".to_string(),
                        property: FixtureProperty::Alpha,
                        body: migrated.clone(),
                    },
                    0.0,
                    fixture_index,
                    2,
                    &BTreeMap::new(),
                );
                assert_eq!(actual, Some(expected), "{legacy:?} fixture {fixture_index}");
            }
        }
    }

    #[test]
    fn compatibility_layers_replace_the_base_instead_of_scaling_it() {
        let mut state = engine_with_two_fixtures();
        set_alpha(&mut state, FIXTURE_A, 42);

        let mut body = AnimationSpecBody::AudioVolume(Default::default());
        body.migrate_legacy_audio();
        add_animation(
            &mut state,
            SCENE,
            0,
            vec![FIXTURE_A],
            AnimationSpec {
                name: "Legacy".to_string(),
                property: FixtureProperty::Alpha,
                body,
            },
        );

        let mut clock = AnimationClockRuntime::default();
        clock.tick(0, &mut state, &loud_audio());

        assert_eq!(
            output_for(&clock, FIXTURE_A, FixtureProperty::Alpha).absolute,
            Some(255)
        );
        assert_eq!(rendered_alpha(&clock, &state, SCENE, FIXTURE_A), 255);
    }

    #[test]
    fn flash_animation_drives_any_property_and_inserts_dark_gaps_between_windows() {
        let mut state = engine_with_two_fixtures();
        add_animation(
            &mut state,
            SCENE,
            0,
            vec![FIXTURE_A, FIXTURE_B],
            AnimationSpec {
                name: "Hue flash".to_string(),
                property: FixtureProperty::ColorHue,
                body: AnimationSpecBody::FlashAnimation(FlashAnimationSpec {
                    amplitude_min: FixtureValue::Literal(10),
                    amplitude_max: FixtureValue::Literal(300),
                    off_time_ms: 100,
                    on_time_ms: 100,
                    beat_aligned: false,
                    off_time_beats: blaulicht_shared::AnimationSpeedModifier::_1,
                    on_time_beats: blaulicht_shared::AnimationSpeedModifier::_1,
                    window_size: 1,
                    window_layout: FlashWindowLayout::Contiguous,
                    random_order: false,
                    reverse_after_n_iterations: None,
                }),
            },
        );

        let mut clock = AnimationClockRuntime::default();
        let audio = silent_disconnected_audio();
        clock.tick(0, &mut state, &audio);
        assert_eq!(
            output_for(&clock, FIXTURE_A, FixtureProperty::ColorHue).absolute,
            Some(10)
        );
        assert_eq!(
            output_for(&clock, FIXTURE_B, FixtureProperty::ColorHue).absolute,
            Some(10)
        );

        clock.tick(100, &mut state, &audio);
        assert_eq!(
            output_for(&clock, FIXTURE_A, FixtureProperty::ColorHue).absolute,
            Some(300)
        );
        assert_eq!(
            output_for(&clock, FIXTURE_B, FixtureProperty::ColorHue).absolute,
            Some(10)
        );

        clock.tick(200, &mut state, &audio);
        assert_eq!(
            output_for(&clock, FIXTURE_A, FixtureProperty::ColorHue).absolute,
            Some(10)
        );
        assert_eq!(
            output_for(&clock, FIXTURE_B, FixtureProperty::ColorHue).absolute,
            Some(10)
        );

        clock.tick(300, &mut state, &audio);
        assert_eq!(
            output_for(&clock, FIXTURE_A, FixtureProperty::ColorHue).absolute,
            Some(10)
        );
        assert_eq!(
            output_for(&clock, FIXTURE_B, FixtureProperty::ColorHue).absolute,
            Some(300)
        );

        let animation = state
            .scenes
            .get_mut(&SCENE)
            .unwrap()
            .sink
            .active_animations
            .values_mut()
            .next()
            .unwrap()
            .get_mut(&0)
            .unwrap();
        animation.reset_timers(SyncMode::Synced);
        clock.tick(350, &mut state, &audio);
        assert_eq!(
            output_for(&clock, FIXTURE_A, FixtureProperty::ColorHue).absolute,
            Some(10)
        );
        assert_eq!(
            output_for(&clock, FIXTURE_B, FixtureProperty::ColorHue).absolute,
            Some(10)
        );
    }
}
