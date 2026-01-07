use crate::dmx::{EngineState, FixtureState};
use blaulicht_shared::{
    fixture::state::{Fixture, FixtureGroup},
    scene::{EngineSink, Scene},
    AnimationTimerState,
};
use log::debug;

impl EngineState {
    pub fn delete_animation(&mut self, id: u8) {
        // Find all dependent animations.
        todo!("Not implemented")
    }

    pub fn delete_group(&mut self, id: u8) {
        self.0.groups.remove(&id);
        self.0.selection.clear();

        // Fix any scene animations which include an illegal group.
        self.0.scenes = self
            .0
            .scenes
            .clone()
            .into_iter()
            .map(|(id, scene)| {
                (
                    id,
                    Scene {
                        sink: EngineSink {
                            active_animations: scene
                                .sink
                                .active_animations
                                .into_iter()
                                .filter(|(selection, _)| {
                                    let illegal_animation =
                                        selection.fixtures.iter().any(|(group_id, _)| {
                                            !self
                                                .0
                                                .groups
                                                .iter()
                                                .any(|(cmp_id, _)| cmp_id == group_id)
                                        });

                                    if illegal_animation {
                                        debug!(
                                        "Delete scene: purged illegal animation with group: {id}"
                                    );
                                    }

                                    !illegal_animation
                                })
                                .collect(),
                            ..scene.sink
                        },
                        ..scene
                    },
                )
            })
            .collect();
    }

    pub fn create_group(&mut self, name: String) -> u8 {
        let new_id = self.0.groups.len();
        debug_assert!(new_id < u8::MAX as usize);
        self.0.groups.insert(
            new_id as u8,
            FixtureGroup {
                name,
                ..FixtureGroup::default()
            },
        );
        new_id as u8
    }

    pub fn add_fixture_to_group(&mut self, group_id: u8, fixture: Fixture) -> u8 {
        let group = self.0.groups.get_mut(&group_id).unwrap();
        let new_id = group.fixtures.len();
        debug_assert!(new_id < u8::MAX as usize);
        group.fixtures.insert(new_id as u8, fixture);

        let fixture_group_key = (group_id, new_id as u8);

        // Patch the scenes.
        for (scene_id, scene) in self.0.scenes.iter_mut() {
            scene
                .sink
                .fixture_states
                .insert(fixture_group_key, FixtureState::default());

            for (_selection, anim) in scene.sink.active_animations.iter_mut() {
                for (_anim_id, anim) in anim.iter_mut() {
                    anim.fixture_timers
                        .insert(fixture_group_key, AnimationTimerState::default());
                }
            }

            debug!("Patched scene {scene_id} with new fixture + anim state");
        }

        new_id as u8
    }
}
