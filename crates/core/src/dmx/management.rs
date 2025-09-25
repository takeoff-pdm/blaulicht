use crate::dmx::{EngineState, Fixture, FixtureGroup, FixtureState};
use log::debug;

impl EngineState {
    pub fn create_group(&mut self, name: String) -> u8 {
        let new_id = self.groups.len();
        debug_assert!(new_id < u8::MAX as usize);
        self.groups.insert(
            new_id as u8,
            FixtureGroup {
                name,
                ..FixtureGroup::default()
            },
        );
        new_id as u8
    }

    pub fn add_fixture_to_group(&mut self, group_id: u8, fixture: Fixture) -> u8 {
        let group = self.groups.get_mut(&group_id).unwrap();
        let new_id = group.fixtures.len();
        debug_assert!(new_id < u8::MAX as usize);
        group.fixtures.insert(new_id as u8, fixture);

        let fixture_group_key = (group_id, new_id as u8);

        // Patch the scenes.
        for (scene_id, scene) in self.scenes.iter_mut() {
            scene
                .sink
                .fixture_states
                .insert(fixture_group_key, FixtureState::default());

            for (_selection, anim) in scene.sink.active_animations.iter_mut() {
                for (_anim_id, anim) in anim.iter_mut() {
                    anim.fixture_timers.insert(
                        fixture_group_key,
                        crate::dmx::AnimationTimerState::default(),
                    );
                }
            }

            debug!("Patched scene {scene_id} with new fixture + anim state");
        }

        new_id as u8
    }
}
