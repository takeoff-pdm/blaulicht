pub mod phaser;

use crate::{dmx::DmxEngine, mainloop::DMX_TICK_TIME};
use blaulicht_audio_engine::CollectorOutput;
use blaulicht_shared::{AnimationSpec, AnimationSpecBody, CollectedAudioSnapshot, PhaserDuration};
pub use phaser::*;
use std::{cmp, time::Instant};

const NUMBER_OF_STEPS: f64 = 360.0;

impl DmxEngine {
    fn animation_base_time(
        audio_snapshot: CollectedAudioSnapshot,
        animation_spec: &AnimationSpec,
    ) -> f64 {
        let speed_per_step = {
            match &animation_spec.body {
                AnimationSpecBody::PhaserRush(_) => {
                    todo!("PANIC")
                }
                AnimationSpecBody::Phaser(body) => {
                    let speed_for_all_steps = match body.time_total {
                        PhaserDuration::Fixed(time) => time as f64,
                        PhaserDuration::Beat(beats) => {
                            audio_snapshot.time_between_beats_millis as f64 * beats.as_float()
                        }
                    };
                    speed_for_all_steps / NUMBER_OF_STEPS
                }
                AnimationSpecBody::AudioVolume(_)
                | AnimationSpecBody::BPMValue(_)
                | AnimationSpecBody::BeatClock(_)
                | AnimationSpecBody::AudioBeat(_)
                | AnimationSpecBody::AudioFrequencies(_) => {
                    // Reason for this is to 'always' update those animations.
                    // DMX_TICK_TIME.as_millis() as f64 / 2.0
                    0.0
                }
                AnimationSpecBody::Wasm(_) => todo!(),
            }
        };

        speed_per_step
    }

    fn generate_animation_value(
        &self,
        audio_snapshot: &CollectorOutput,
        spec: &AnimationSpec,
        fixture_time: u64,
        // When having a synced / offsetted animation 'group'
        fixture_index_in_selection: usize,
        fixtures_in_selection: usize,
    ) -> u16 {
        match &spec.body {
            AnimationSpecBody::PhaserRush(_) => todo!("ERROR"),
            AnimationSpecBody::Phaser(body) => phaser::generate(body, fixture_time),
            AnimationSpecBody::AudioVolume(_) => audio_snapshot.snapshot.volume as u16,
            AnimationSpecBody::BPMValue(_) => audio_snapshot.snapshot.bpm as u16,
            AnimationSpecBody::AudioFrequencies(freqs) => {
                // Create bins of size `fixtures_in_selection`
                // let bins = bin_spectrum_to_u8(&audio_snapshot.current_audio_colunn, fixtures_in_selection);

                // HACK: Give audio engine time to start.
                if audio_snapshot.current_audio_colunn.is_empty() {
                    return 0;
                }

                if fixtures_in_selection == 0 {
                    return 0;
                }

                // Quantize to reduce dimension of the audio column
                debug_assert!(audio_snapshot.current_audio_colunn.len() > fixtures_in_selection);

                // Apply per-animation frequency window, gate, and boost adjustments before binning.
                let processed_bins: Vec<u8> = {
                    const MAX_FREQ_HZ: f32 = 20_000.0;

                    let freq_min =
                        (freqs.freq_min.min(freqs.freq_max) as f32).clamp(0.0, MAX_FREQ_HZ);
                    let freq_max =
                        (freqs.freq_min.max(freqs.freq_max) as f32).clamp(0.0, MAX_FREQ_HZ);

                    let denom = (audio_snapshot.current_audio_colunn.len() - 1).max(1) as f32;

                    let gate_threshold = freqs.gate as f32;
                    let boost = freqs.boost;

                    audio_snapshot
                        .current_audio_colunn
                        .iter()
                        .enumerate()
                        .filter_map(|(idx, audio_bucket)| {
                            let bin_freq = (idx as f32 / denom) * MAX_FREQ_HZ;

                            if bin_freq < freq_min || bin_freq > freq_max {
                                return None;
                            }

                            let mut value = audio_bucket.volume as f32;

                            if value < gate_threshold {
                                return None;
                            }

                            if boost > 0 {
                                value = (value + boost as f32).min(u8::MAX as f32);
                            }

                            Some(value.round().clamp(0.0, u8::MAX as f32) as u8)
                        })
                        .collect()
                };

                if processed_bins.is_empty() {
                    return 0;
                }

                let chunk_size = cmp::max(processed_bins.len() / fixtures_in_selection, 1);
                let mut quantized_audio_bins: Vec<usize> = processed_bins
                    .chunks(chunk_size)
                    .map(|chunk| {
                        // Compute average
                        let max = chunk.iter().copied().map(|v| v as usize).max();
                        // sum / chunk.len()
                        max.unwrap_or(0)
                    })
                    .collect();

                while fixture_index_in_selection >= quantized_audio_bins.len() {
                    quantized_audio_bins.push(0); // NOOO: this adds 0 padding and bricks it
                }

                debug_assert!(fixture_index_in_selection < quantized_audio_bins.len());

                let fixture_value = quantized_audio_bins[fixture_index_in_selection];
                fixture_value as u16
            }
            AnimationSpecBody::AudioBeat(_) => audio_snapshot.snapshot.bass as u16,
            AnimationSpecBody::BeatClock(_) => (audio_snapshot.snapshot.beat_trigger as u16) * 255,
            AnimationSpecBody::Wasm(_) => todo!(),
        }
    }

    pub fn animation_tick(&mut self, audio_snapshot: &CollectorOutput) {
        // // TODO: what's the plan for this?
        // //
        // // Go over all scenes and then over all selections for that scene.
        //
        let now = (Instant::now().duration_since(self.start_time)).as_millis() as u64;
        let mut state = self.state_ref.dmx_engine.write().unwrap();
        // let animations = state.0.animation_templates.clone();

        for (_scene_id, scene) in state.0.scenes.iter_mut() {
            let animation_speed_factor = scene.sink.master_speed;

            for (selection, scene_animations) in scene.sink.active_animations.iter_mut() {
                let fixtures_in_selection = selection.len();

                for (_animation_id, animation) in scene_animations.iter_mut() {
                    if !animation.enabled {
                        continue;
                    }

                    for (fixture_index_in_selection, (fixture_selec, fixture_anim_state)) in
                        animation.fixture_timers.iter_mut().enumerate()
                    {
                        let base_time = Self::animation_base_time(
                            audio_snapshot.snapshot.clone(),
                            &animation.spec_cloned,
                        );

                        let transition_time = base_time
                            * animation.speed_factor.as_float()
                            * animation_speed_factor.as_float();

                        // TODO: limited by tick speed

                        // println!("{}", now - animation.last_tick_time);

                        let mut num_ticks = 1;

                        let millis = DMX_TICK_TIME.as_millis();
                        if transition_time < millis as f64 {
                            num_ticks = (millis as f64 / transition_time) as usize;
                            // println!("NUM TICKS: {num_ticks} | millis = {millis} | trans = {transition_time} | factor = {}", animation.speed_factor.as_float());
                        }

                        // let spec = animations.get(animation_id).unwrap();

                        if fixture_anim_state.timer == 0 && animation.spec_cloned.is_beat_pinned() {
                            fixture_anim_state.needs_reset_on_beat = true;
                        }

                        if fixture_anim_state.needs_reset_on_beat
                            && audio_snapshot.snapshot.beat_trigger
                        {
                            fixture_anim_state.timer = 0;
                            fixture_anim_state.needs_reset_on_beat = false;
                        }

                        if transition_time == 0.0 || fixture_anim_state.needs_reset_on_beat {
                            continue;
                        }

                        // TODO: extremely naiive implementation
                        // FLAWS:
                        //  - beat-timing is not considered
                        //  - syncing between animations is also not considered
                        //      - Different sync modes
                        //          - No sync (when playing current animation, disregard everything and
                        //          start it)
                        //          - Group sync (sync with all other fixtures in the parent group that
                        //          also use this animation)
                        //          - Global (sync with ALL other fixtures (also from other groups)
                        //          that also use this animation)
                        //      - How is syncing done?
                        //      - when sync mode is changed, timing is reset to 0 for all fixtures and
                        //      the stepper logic uses the sync
                        //      - Syncing shall be displayed graphically
                        //      - Running animations shall also be displayed graphically
                        //      - Each phaser can be absolute / relative!
                        // println!("{}", fixture_anim_state.last_tick_time);
                        if now.saturating_sub(fixture_anim_state.last_tick_time)
                            >= transition_time as u64
                        {
                            for _ in 0..num_ticks {
                                fixture_anim_state.tick(now);
                            }

                            if animation.spec_cloned.is_beat_pinned()
                                && fixture_anim_state.timer >= 360
                            {
                                fixture_anim_state.needs_reset_on_beat = true;
                            }

                            let v = self.generate_animation_value(
                                audio_snapshot,
                                &animation.spec_cloned,
                                fixture_anim_state.timer,
                                fixture_index_in_selection,
                                fixtures_in_selection,
                            );

                            let fixture_state =
                                scene.sink.fixture_states.get_mut(fixture_selec).unwrap();

                            // TODO: support multiple values?
                            fixture_state.apply_value(v, animation.spec_cloned.property);
                        }
                    }
                }
            }
        }
    }
}
