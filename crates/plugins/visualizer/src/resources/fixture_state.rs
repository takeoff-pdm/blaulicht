use bevy_ecs::prelude::*;
use blaulicht_shared::EngineState;
use std::collections::HashMap;

#[derive(Resource, Default)]
pub struct CachedEngineState {
    pub state: EngineState,
    pub needs_sync: bool,
}

impl CachedEngineState {
    pub fn update(&mut self, new_state: EngineState) {
        self.state = new_state;
        self.needs_sync = true;
    }

    pub fn mark_synced(&mut self) {
        self.needs_sync = false;
    }
}

#[derive(Resource, Default)]
pub struct FixtureEntityMap {
    pub entities: HashMap<(u8, u8), Entity>,
}

impl FixtureEntityMap {
    pub fn insert(&mut self, group_id: u8, fixture_id: u8, entity: Entity) {
        self.entities.insert((group_id, fixture_id), entity);
    }

    pub fn get(&self, group_id: u8, fixture_id: u8) -> Option<Entity> {
        self.entities.get(&(group_id, fixture_id)).copied()
    }

    pub fn remove(&mut self, group_id: u8, fixture_id: u8) -> Option<Entity> {
        self.entities.remove(&(group_id, fixture_id))
    }

    pub fn clear(&mut self) {
        self.entities.clear();
    }
}
