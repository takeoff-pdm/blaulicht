pub mod phaser;

use crate::dmx::DmxEngine;
use blaulicht_audio_engine::CollectorOutput;
use blaulicht_shared::{
    AnimationSpec, AnimationSpecBody, AudioSourceStatus, PhaserDuration, palette::Palette,
};
pub use phaser::*;
use std::{
    collections::{BTreeMap, HashMap, hash_map::DefaultHasher},
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
    last_update_ms: u64,
    last_event_id: u64,
    initialized: bool,
}

impl MusicalBeatClock {
    fn update(&mut self, now_ms: u64, audio: &blaulicht_shared::CollectedAudioSnapshot) {
        if !self.initialized {
            self.last_update_ms = now_ms;
            self.initialized = true;
        }

        let elapsed_ms = now_ms.saturating_sub(self.last_update_ms) as f64;
        if let Some(period_ms) = self.period_ms.filter(|period| *period > 0.0) {
            self.beat_position += elapsed_ms / period_ms;
        }
        self.last_update_ms = now_ms;

        self.period_ms = if audio.source_status != AudioSourceStatus::Disconnected
            && audio.time_between_beats_millis > 0
        {
            Some(audio.time_between_beats_millis as f64)
        } else {
            None
        };

        if audio.beat_trigger
            && audio.beat_event_id != 0
            && audio.beat_event_id != self.last_event_id
        {
            self.beat_position = self.beat_position.round();
            self.last_event_id = audio.beat_event_id;
        }
    }
}

#[derive(Default)]
pub(crate) struct AnimationClockRuntime {
    fixture_phases: HashMap<AnimationRuntimeKey, FixturePhaseRuntime>,
    beat_clock: MusicalBeatClock,
    generation: u64,
    fixture_scratch: Vec<(u8, u8)>,
    phase_scratch: Vec<f64>,
}

impl AnimationClockRuntime {
    fn begin_tick(&mut self) -> u64 {
        self.generation = self.generation.wrapping_add(1).max(1);
        self.generation
    }

    fn retain_current_generation(&mut self) {
        let generation = self.generation;
        self.fixture_phases
            .retain(|_, runtime| runtime.seen_generation == generation);
    }
}

impl DmxEngine {
    fn animation_cycle_duration_ms(
        audio: &blaulicht_shared::CollectedAudioSnapshot,
        spec: &AnimationSpec,
        animation_speed: f64,
        scene_speed: f64,
    ) -> Option<f64> {
        let AnimationSpecBody::Phaser(body) = &spec.body else {
            return None;
        };
        let base_duration = match body.time_total {
            PhaserDuration::Fixed(milliseconds) => milliseconds as f64,
            PhaserDuration::Beat(beats) => {
                if audio.time_between_beats_millis == 0
                    || audio.source_status == AudioSourceStatus::Disconnected
                {
                    return None;
                }
                audio.time_between_beats_millis as f64 * beats.as_float()
            }
        };
        let duration = base_duration * animation_speed * scene_speed;
        duration
            .is_finite()
            .then_some(duration)
            .filter(|value| *value > 0.0)
    }

    fn generate_animation_value(
        audio_snapshot: &CollectorOutput,
        spec: &AnimationSpec,
        fixture_phase: f64,
        fixture_index_in_selection: usize,
        fixtures_in_selection: usize,
        palettes: &BTreeMap<u8, Palette>,
    ) -> u16 {
        match &spec.body {
            AnimationSpecBody::Phaser(body) => {
                phaser::generate(body, fixture_phase, spec.property, palettes)
            }
            AnimationSpecBody::AudioVolume(_) => audio_snapshot.snapshot.volume as u16,
            AnimationSpecBody::BPMValue(_) => audio_snapshot.snapshot.bpm as u16,
            AnimationSpecBody::AudioFrequencies(freqs) => {
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
                        bin_freq >= freq_min
                            && bin_freq <= freq_max
                            && bucket.volume as f32 >= gate_threshold
                    })
                    .count();
                if valid == 0 {
                    return 0;
                }
                let target = fixture_index_in_selection * valid / fixtures_in_selection;
                let mut seen = 0;
                for (idx, bucket) in audio_snapshot.current_audio_colunn.iter().enumerate() {
                    let bin_freq = (idx as f32 / denom) * MAX_FREQ_HZ;
                    if bin_freq < freq_min
                        || bin_freq > freq_max
                        || (bucket.volume as f32) < gate_threshold
                    {
                        continue;
                    }
                    if seen == target {
                        return (bucket.volume as u16 + freqs.boost as u16).min(u8::MAX as u16);
                    }
                    seen += 1;
                }
                0
            }
            AnimationSpecBody::AudioBeat(_) => audio_snapshot.snapshot.bass as u16,
            AnimationSpecBody::BeatClock(_) => (audio_snapshot.snapshot.beat_trigger as u16) * 255,
            AnimationSpecBody::Wasm(_) => 0,
        }
    }

    pub fn animation_tick(&mut self, audio_snapshot: &CollectorOutput) {
        let now_ms = self.start_time.elapsed().as_millis().min(u64::MAX as u128) as u64;
        self.animation_clock
            .beat_clock
            .update(now_ms, &audio_snapshot.snapshot);
        let beat_position = self.animation_clock.beat_clock.beat_position;
        let generation = self.animation_clock.begin_tick();
        let mut state = self.state_ref.dmx_engine.write().unwrap();
        let palettes_snapshot = state.0.palettes.clone();

        for (scene_id, scene) in state.0.scenes.iter_mut() {
            let scene_speed = scene.sink.master_speed.as_float();
            for (selection, scene_animations) in scene.sink.active_animations.iter_mut() {
                let mut selection_hasher = DefaultHasher::new();
                selection.fixtures.hash(&mut selection_hasher);
                let selection_fingerprint = selection_hasher.finish();
                for (animation_id, animation) in scene_animations.iter_mut() {
                    if !animation.enabled {
                        continue;
                    }

                    self.animation_clock.fixture_scratch.clear();
                    self.animation_clock.fixture_scratch.extend(
                        selection
                            .fixtures
                            .iter()
                            .copied()
                            .filter(|key| animation.fixture_timers.contains_key(key)),
                    );
                    let fixture_count = self.animation_clock.fixture_scratch.len();
                    if fixture_count == 0 {
                        continue;
                    }

                    let cycle_duration = Self::animation_cycle_duration_ms(
                        &audio_snapshot.snapshot,
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

                    self.animation_clock.phase_scratch.clear();
                    let mut representative_crossings = 0_u64;
                    for fixture_index in 0..fixture_count {
                        let fixture_key = self.animation_clock.fixture_scratch[fixture_index];
                        let key = AnimationRuntimeKey {
                            scene_id: *scene_id,
                            selection_fingerprint,
                            animation_id: *animation_id,
                            fixture: fixture_key,
                        };
                        let timer = animation.fixture_timers.get_mut(&fixture_key).unwrap();
                        let runtime = self.animation_clock.fixture_phases.entry(key).or_default();
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
                        representative_crossings = crossings;
                        timer.timer = runtime.phase_degrees.floor().max(0.0) as u64;
                        timer.last_tick_time = now_ms.max(1);
                        timer.needs_reset_on_beat = false;
                        self.animation_clock
                            .phase_scratch
                            .push(runtime.phase_degrees);
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

                    for fixture_index in 0..fixture_count {
                        let fixture_phase = self.animation_clock.phase_scratch[fixture_index];
                        let target_index = if animation.reversed {
                            fixture_count - 1 - fixture_index
                        } else {
                            fixture_index
                        };
                        let value = Self::generate_animation_value(
                            audio_snapshot,
                            &animation.spec_cloned,
                            fixture_phase,
                            fixture_index,
                            fixture_count,
                            &palettes_snapshot,
                        );
                        let target_fixture = self.animation_clock.fixture_scratch[target_index];
                        if let Some(fixture_state) =
                            scene.sink.fixture_states.get_mut(&target_fixture)
                        {
                            fixture_state.apply_value(value, animation.spec_cloned.property);
                        }
                    }
                }
            }
        }
        self.animation_clock.retain_current_generation();
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
        assert_eq!(clock.beat_position, 1.0);

        audio.beat_trigger = false;
        audio.source_status = AudioSourceStatus::Disconnected;
        clock.update(500, &audio);
        let paused = clock.beat_position;
        clock.update(1_500, &audio);
        assert_eq!(clock.beat_position, paused);
    }

    #[test]
    fn pinned_phase_uses_global_beat_position_and_fixture_offset() {
        let mut phase = FixturePhaseRuntime::default();
        phase.reset(0, 90.0);
        phase.align_to_beat_clock(500, 2.0, 2.0);
        assert_eq!(phase.phase_degrees, 450.0);
    }

    #[test]
    fn intentional_beat_division_mapping_is_unchanged() {
        use blaulicht_shared::AnimationSpeedModifier;

        assert_eq!(AnimationSpeedModifier::_2.as_float(), 0.5);
        assert_eq!(AnimationSpeedModifier::_1_16.as_float(), 16.0);
    }
}
