pub mod state;
use std::time::Instant;

use blaulicht_shared::{AnimationSpec, AnimationSpecBody, CollectedAudioSnapshot, PhaserDuration};
pub use state::*;
pub mod phaser;
pub use phaser::*;

use crate::{dmx::DmxEngine, mainloop::DMX_TICK_TIME};

impl DmxEngine {
    pub fn build_animations_cache(&mut self, audio_snapshot: CollectedAudioSnapshot) {
        // Only build every 100ms or so?

        let animations = &self.state_ref.dmx_engine.read().unwrap().0.animations;

        for (anim_id, anim) in animations {
            let speed_per_step = {
                let animation_spec = animations.get(anim_id).unwrap();
                match &animation_spec.body {
                    AnimationSpecBody::Phaser(body) => {
                        let speed_for_all_steps = match body.time_total {
                            PhaserDuration::Fixed(time) => time as f64,
                            PhaserDuration::Beat(beats) => {
                                (audio_snapshot.time_between_beats_millis as f64 * beats.as_float())
                            }
                        };

                        let speed_per_step = speed_for_all_steps / 360.0;

                        // match speed_per_step {
                        //     0 => 1,
                        //     v => v,
                        // }
                        speed_per_step
                    }
                    AnimationSpecBody::AudioVolume(_)
                    | AnimationSpecBody::BeatClock(_)
                    | AnimationSpecBody::AudioBeat(_) => DMX_TICK_TIME.as_millis() as f64,
                    AnimationSpecBody::Wasm(animation_spec_body_wasm) => todo!(),
                }
            };

            self.animation_base_times.insert(*anim_id, speed_per_step);
        }
    }

    fn generate_animation_value(
        &self,
        audio_snapshot: CollectedAudioSnapshot,
        spec: &AnimationSpec,
        id: u8,
        time: u64,
    ) -> u16 {
        // let animations = &self.state_ref.dmx_engine.read().unwrap().animations;
        // let animation = animations.get(&id).unwrap();

        match &spec.body {
            AnimationSpecBody::Phaser(body) => phaser::generate(body, time as f32),
            AnimationSpecBody::AudioVolume(animation_spec_body_audio_volume) => {
                audio_snapshot.volume as u16
            }
            AnimationSpecBody::AudioBeat(animation_spec_body_beat) => {
                audio_snapshot.bass_avg_short as u16
            }
            AnimationSpecBody::BeatClock(animation_spec_body_beat) => {
                (audio_snapshot.beat_trigger as u16) * 255
            }
            AnimationSpecBody::Wasm(animation_spec_body_wasm) => todo!(),
        }
    }

    pub fn animation_tick(&mut self, audio_snapshot: CollectedAudioSnapshot) {
        // // TODO: what's the plan for this?
        // //
        // // Go over all scenes and then over all selections for that scene.
        //
        let now = (Instant::now().duration_since(self.start_time)).as_millis() as u64;
        let mut state = self.state_ref.dmx_engine.write().unwrap();
        let animations = state.0.animations.clone();

        for (scene_id, scene) in state.0.scenes.iter_mut() {
            for (selection, scene_animations) in scene.sink.active_animations.iter_mut() {
                // println!("scene anim: {scene_animations:?}");

                for (animation_id, animation) in scene_animations.iter_mut() {
                    if !animation.enabled {
                        continue;
                    }

                    for (fixture_selec, fixture_anim_state) in animation.fixture_timers.iter_mut() {
                        let transition_time =
                            (*self.animation_base_times.get(animation_id).unwrap()) as f64
                                * animation.speed_factor.as_float();

                        // TODO: limited by tick speed

                        // println!("{}", now - animation.last_tick_time);

                        let mut num_ticks = 1;

                        let millis = DMX_TICK_TIME.as_millis();
                        if transition_time < millis as f64 {
                            num_ticks = (millis as f64 / transition_time) as usize;
                            println!("NUM TICKS: {num_ticks} | millis = {millis} | trans = {transition_time} | factor = {}", animation.speed_factor.as_float());
                        }

                        let spec = animations.get(animation_id).unwrap();

                        if fixture_anim_state.timer == 0 && spec.is_beat_pinned() {
                            fixture_anim_state.needs_reset_on_beat = true;
                        }

                        if fixture_anim_state.needs_reset_on_beat && audio_snapshot.beat_trigger {
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
                        if now - fixture_anim_state.last_tick_time >= transition_time as u64 {
                            for _ in 0..num_ticks {
                                fixture_anim_state.tick(now);
                            }

                            if spec.is_beat_pinned() && fixture_anim_state.timer >= 360 {
                                fixture_anim_state.needs_reset_on_beat = true;
                            }

                            let v = self.generate_animation_value(
                                audio_snapshot,
                                spec,
                                *animation_id,
                                fixture_anim_state.timer,
                            );

                            let fixture_state =
                                scene.sink.fixture_states.get_mut(fixture_selec).unwrap();

                            fixture_state.apply_value(v, spec.property);

                            // println!("update animation");
                        }
                    }
                }
            }
        }

        //
        // let fixtures = state
        //     .groups
        //     .iter_mut()
        //     .flat_map(|(_, g)| g.fixtures.values_mut());
        //
        // for fixture in fixtures {
        // }
    }
}
