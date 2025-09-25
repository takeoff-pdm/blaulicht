use crate::dmx::{EngineState, Fixture, FixtureGroup};

impl EngineState {
    pub fn create_group(&mut self) -> u8 {
        let new_id = self.groups.len();
        debug_assert!(new_id < u8::MAX as usize);
        self.groups.insert(new_id as u8, FixtureGroup::default());
        new_id as u8
    }

    pub fn add_fixture_to_group(&mut self, group_id: u8, fixture: Fixture) -> u8 {
        let group = self.groups.get_mut(&group_id).unwrap();
        let new_key = group.fixtures.len();
        debug_assert!(new_key < u8::MAX as usize);
        group.fixtures.insert(new_key as u8, fixture);
        new_key as u8
    }
}

