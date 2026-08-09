use crate::dmx::{EngineState, FixtureState};
use blaulicht_shared::fixture::state::{Fixture, FixtureGroup};
use std::collections::HashSet;
use tracing::debug;

impl EngineState {
    pub fn delete_animation(&mut self, id: u8) {
        let removed_template = self.0.animation_templates.remove(&id).is_some();

        for (scene_id, scene) in self.0.scenes.iter_mut() {
            let mut selections_to_purge = Vec::new();

            for (selection, active_animations) in scene.sink.active_animations.iter_mut() {
                if active_animations.remove(&id).is_some() {
                    debug!("Delete animation: removed template {id} from scene {scene_id}");
                }

                if active_animations.is_empty() {
                    selections_to_purge.push(selection.clone());
                }
            }

            for selection in selections_to_purge {
                scene.sink.active_animations.remove(&selection);
                debug!(
                    "Delete animation: removed empty selection {:?} from scene {scene_id}",
                    selection
                );
            }
        }

        if !removed_template {
            debug!("Delete animation: template {id} not found in engine state");
        }
    }

    pub fn delete_group(&mut self, id: u8) {
        self.0.groups.remove(&id);
        self.0.selection.clear();
        self.0.selection_stack.clear();

        self.purge_fixture_state();
    }

    fn purge_fixture_state(&mut self) {
        let valid_keys: HashSet<(u8, u8)> = self
            .0
            .groups
            .iter()
            .flat_map(|(gid, group)| group.fixtures.keys().map(move |fid| (*gid, *fid)))
            .collect();

        for (scene_id, scene) in self.0.scenes.iter_mut() {
            scene.sink.retain_fixture_keys(&valid_keys);
            debug!("Reconciled fixture state for scene {scene_id}");
        }
    }

    pub fn create_group(&mut self, name: String) -> Option<u8> {
        let new_id = (0..=u8::MAX).find(|id| !self.0.groups.contains_key(id))?;

        self.0.groups.insert(
            new_id,
            FixtureGroup {
                name,
                ..FixtureGroup::default()
            },
        );
        Some(new_id)
    }

    pub fn add_fixture_to_group(&mut self, group_id: u8, fixture: Fixture) -> Option<u8> {
        let new_id = {
            let group = self.0.groups.get_mut(&group_id)?;
            let new_id = (0..=u8::MAX).find(|id| !group.fixtures.contains_key(id))?;
            group.fixtures.insert(new_id, fixture);
            new_id
        };

        let fixture_group_key = (group_id, new_id);

        // Patch the scenes.
        for (scene_id, scene) in self.0.scenes.iter_mut() {
            scene
                .sink
                .fixture_states
                .insert(fixture_group_key, FixtureState::default());

            debug!("Patched scene {scene_id} with new fixture + anim state");
        }

        Some(new_id)
    }

    pub fn delete_fixture_from_group(&mut self, group_id: u8, fixture_id: u8) -> bool {
        let removed = self
            .0
            .groups
            .get_mut(&group_id)
            .and_then(|group| group.fixtures.remove(&fixture_id))
            .is_some();
        if !removed {
            return false;
        }

        self.0.selection.clear();
        self.0.selection_stack.clear();
        self.purge_fixture_state();

        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use blaulicht_shared::{
        fixture::{light::Light, FixtureType},
        scene::FixtureSelection,
        ActiveAnimation, AnimationSpec,
    };
    use std::collections::BTreeMap;

    #[test]
    fn adding_fixture_does_not_reindex_running_animation_timers() {
        let mut engine = EngineState(blaulicht_shared::EngineState::default());
        engine.0.groups.insert(
            0,
            FixtureGroup {
                name: "Group".to_string(),
                fixtures: BTreeMap::new(),
            },
        );
        engine.add_fixture_to_group(
            0,
            Fixture::new(
                0,
                1,
                "Existing".to_string(),
                FixtureType::from(Light::Generic3ChanNoAlpha),
            ),
        );
        engine.0.new_scene("Scene".to_string());

        let selection = FixtureSelection {
            fixtures: vec![(0, 0)],
        };
        let mut animation = ActiveAnimation::new(&selection.fixtures, AnimationSpec::empty());
        animation.enabled = true;
        animation.fixture_timers.get_mut(&(0, 0)).unwrap().timer = 123;
        engine
            .0
            .scenes
            .get_mut(&0)
            .unwrap()
            .sink
            .active_animations
            .insert(selection, BTreeMap::from([(7, animation)]));

        let new_id = engine.add_fixture_to_group(
            0,
            Fixture::new(
                0,
                4,
                "Added".to_string(),
                FixtureType::from(Light::Generic3ChanNoAlpha),
            ),
        );

        assert_eq!(new_id, Some(1));
        let active = &engine
            .0
            .scenes
            .get(&0)
            .unwrap()
            .sink
            .active_animations
            .values()
            .next()
            .unwrap()[&7];
        assert_eq!(
            active.fixture_timers.keys().copied().collect::<Vec<_>>(),
            vec![(0, 0)]
        );
        assert_eq!(active.fixture_timers[&(0, 0)].timer, 123);
        assert!(engine
            .0
            .scenes
            .get(&0)
            .unwrap()
            .sink
            .fixture_states
            .contains_key(&(0, 1)));
    }
}
