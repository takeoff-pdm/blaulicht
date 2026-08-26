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

    /// Moves a fixture to another group and remaps all scene-local state that
    /// references the old fixture key. The moved fixture receives the first
    /// free fixture id in the destination group.
    pub fn move_fixture_to_group(
        &mut self,
        source_group_id: u8,
        fixture_id: u8,
        target_group_id: u8,
    ) -> Option<u8> {
        if source_group_id == target_group_id {
            return Some(fixture_id);
        }

        if !self.0.groups.contains_key(&target_group_id) {
            return None;
        }

        let new_fixture_id = self
            .0
            .groups
            .get(&target_group_id)
            .and_then(|group| (0..=u8::MAX).find(|id| !group.fixtures.contains_key(id)))?;

        let fixture = self
            .0
            .groups
            .get_mut(&source_group_id)
            .and_then(|group| group.fixtures.remove(&fixture_id))?;

        self.0
            .groups
            .get_mut(&target_group_id)
            .unwrap()
            .fixtures
            .insert(new_fixture_id, fixture);

        let old_key = (source_group_id, fixture_id);
        let new_key = (target_group_id, new_fixture_id);

        for scene in self.0.scenes.values_mut() {
            move_btree_entry(&mut scene.sink.fixture_states, old_key, new_key);
            move_btree_entry(&mut scene.sink.palette_assignments, old_key, new_key);

            let old_animations = std::mem::take(&mut scene.sink.active_animations);
            scene.sink.active_animations = old_animations
                .into_iter()
                .map(|(mut selection, mut animations)| {
                    for entry in selection.fixtures.iter_mut() {
                        if *entry == old_key {
                            *entry = new_key;
                        }
                    }
                    selection = selection.sorted();

                    for active in animations.values_mut() {
                        move_btree_entry(&mut active.fixture_timers, old_key, new_key);
                    }

                    (selection, animations)
                })
                .collect();

            let old_cs = std::mem::take(&mut scene.sink.changeset);
            scene.sink.changeset = old_cs
                .into_iter()
                .map(|mut sel| {
                    if sel.gid == source_group_id && sel.fid == fixture_id {
                        sel.gid = target_group_id;
                        sel.fid = new_fixture_id;
                    }
                    sel
                })
                .collect();
        }

        move_engine_selection(
            &mut self.0.selection,
            source_group_id,
            fixture_id,
            target_group_id,
            new_fixture_id,
        );
        for sel in self.0.selection_stack.iter_mut() {
            move_engine_selection(
                sel,
                source_group_id,
                fixture_id,
                target_group_id,
                new_fixture_id,
            );
        }

        Some(new_fixture_id)
    }

    /// Swaps two fixtures within a group, including all scene state, selection,
    /// palette assignments, animation timers and changeset entries that
    /// reference them.
    pub fn swap_fixtures_in_group(&mut self, group_id: u8, fid_a: u8, fid_b: u8) -> bool {
        if fid_a == fid_b {
            return false;
        }

        let Some(group) = self.0.groups.get_mut(&group_id) else {
            return false;
        };

        if !group.fixtures.contains_key(&fid_a) || !group.fixtures.contains_key(&fid_b) {
            return false;
        }

        let a = group.fixtures.remove(&fid_a).unwrap();
        let b = group.fixtures.remove(&fid_b).unwrap();
        group.fixtures.insert(fid_a, b);
        group.fixtures.insert(fid_b, a);

        let key_a = (group_id, fid_a);
        let key_b = (group_id, fid_b);

        for scene in self.0.scenes.values_mut() {
            swap_btree_entries(&mut scene.sink.fixture_states, key_a, key_b);
            swap_btree_entries(&mut scene.sink.palette_assignments, key_a, key_b);

            let old_animations = std::mem::take(&mut scene.sink.active_animations);
            scene.sink.active_animations = old_animations
                .into_iter()
                .map(|(mut selection, mut anims)| {
                    for entry in selection.fixtures.iter_mut() {
                        if *entry == key_a {
                            *entry = key_b;
                        } else if *entry == key_b {
                            *entry = key_a;
                        }
                    }
                    for active in anims.values_mut() {
                        swap_btree_entries(&mut active.fixture_timers, key_a, key_b);
                    }
                    (selection, anims)
                })
                .collect();

            let old_cs = std::mem::take(&mut scene.sink.changeset);
            scene.sink.changeset = old_cs
                .into_iter()
                .map(|mut sel| {
                    if sel.gid == group_id {
                        if sel.fid == fid_a {
                            sel.fid = fid_b;
                        } else if sel.fid == fid_b {
                            sel.fid = fid_a;
                        }
                    }
                    sel
                })
                .collect();
        }

        swap_in_u8_set(&mut self.0.selection.fixtures_in_group, fid_a, fid_b);
        for sel in self.0.selection_stack.iter_mut() {
            swap_in_u8_set(&mut sel.fixtures_in_group, fid_a, fid_b);
        }

        true
    }
}

fn move_btree_entry<K: Ord, V>(map: &mut std::collections::BTreeMap<K, V>, old_key: K, new_key: K) {
    if let Some(value) = map.remove(&old_key) {
        map.insert(new_key, value);
    }
}

fn swap_btree_entries<K: Ord, V>(map: &mut std::collections::BTreeMap<K, V>, a: K, b: K) {
    let va = map.remove(&a);
    let vb = map.remove(&b);
    if let Some(vb) = vb {
        map.insert(a, vb);
    }
    if let Some(va) = va {
        map.insert(b, va);
    }
}

fn swap_in_u8_set(set: &mut HashSet<u8>, a: u8, b: u8) {
    let has_a = set.contains(&a);
    let has_b = set.contains(&b);
    if has_a && !has_b {
        set.remove(&a);
        set.insert(b);
    } else if has_b && !has_a {
        set.remove(&b);
        set.insert(a);
    }
}

fn move_engine_selection(
    selection: &mut blaulicht_shared::EngineSelection,
    source_group_id: u8,
    fixture_id: u8,
    target_group_id: u8,
    new_fixture_id: u8,
) {
    if selection.group_ids.len() == 1 && selection.group_ids.contains(&source_group_id) {
        if selection.fixtures_in_group.remove(&fixture_id) {
            selection.group_ids.clear();
            selection.group_ids.insert(target_group_id);
            selection.fixtures_in_group.insert(new_fixture_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use blaulicht_shared::{
        fixture::{light::Light, FixtureType},
        scene::{FixtureSelection, FixtureSelector},
        ActiveAnimation, AnimationSpec, AnimationTemplate, FixtureProperty, SaveEngineState,
    };
    use std::collections::BTreeMap;

    #[test]
    fn five_led_strip_fixture_workflow_survives_showfile_round_trip() {
        const STRIPS: usize = 5;
        const PIXELS_PER_STRIP: usize = 100;

        let mut engine = EngineState::default();
        for strip in 0..STRIPS {
            let group_id = engine
                .create_group(format!("LED strip {}", strip + 1))
                .expect("five groups fit in the engine");
            for pixel in 0..PIXELS_PER_STRIP {
                let fixture_id = engine.add_fixture_to_group(
                    group_id,
                    Fixture::new(
                        strip,
                        1 + pixel * 3,
                        format!("Pixel {}", pixel + 1),
                        FixtureType::from(Light::Generic3ChanNoAlpha),
                    ),
                );
                assert_eq!(fixture_id, Some(pixel as u8));
            }
            engine.0.selection.group_ids.insert(group_id);
        }

        let selection = engine.get_selection().sorted();
        assert_eq!(selection.fixtures.len(), STRIPS * PIXELS_PER_STRIP);

        let animation_id = 42;
        let spec = AnimationSpec::empty();
        engine
            .0
            .animation_templates
            .insert(animation_id, AnimationTemplate { spec: spec.clone() });
        let mut animation = ActiveAnimation::new(&selection.fixtures, spec);
        animation.enabled = true;
        engine.curr_scene_mut().sink.active_animations.insert(
            selection.clone(),
            BTreeMap::from([(animation_id, animation)]),
        );

        let json = serde_json::to_string(&SaveEngineState::from(engine.0.clone())).unwrap();
        let saved: SaveEngineState = serde_json::from_str(&json).unwrap();
        let restored = blaulicht_shared::EngineState::try_from(saved).unwrap();

        assert_eq!(
            restored
                .groups
                .values()
                .map(|group| group.fixtures.len())
                .sum::<usize>(),
            STRIPS * PIXELS_PER_STRIP
        );
        let restored_animation =
            &restored.scenes[&0].sink.active_animations[&selection][&animation_id];
        assert!(restored_animation.enabled);
        assert_eq!(
            restored_animation.fixture_timers.len(),
            STRIPS * PIXELS_PER_STRIP
        );
        for universe in 0..STRIPS {
            let group = &restored.groups[&(universe as u8)];
            assert_eq!(group.fixtures[&99].universe_no, universe);
            assert_eq!(group.fixtures[&99].start_addr, 298);
        }
    }

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

    #[test]
    fn moving_fixture_to_group_remaps_scene_state_and_selection() {
        let mut engine = EngineState(blaulicht_shared::EngineState::default());
        engine.0.groups.insert(
            0,
            FixtureGroup {
                name: "Source".to_string(),
                fixtures: BTreeMap::new(),
            },
        );
        engine.0.groups.insert(
            1,
            FixtureGroup {
                name: "Target".to_string(),
                fixtures: BTreeMap::new(),
            },
        );

        engine.add_fixture_to_group(
            0,
            Fixture::new(
                0,
                1,
                "Moved".to_string(),
                FixtureType::from(Light::Generic3ChanNoAlpha),
            ),
        );
        engine.add_fixture_to_group(
            1,
            Fixture::new(
                0,
                4,
                "Existing target".to_string(),
                FixtureType::from(Light::Generic3ChanNoAlpha),
            ),
        );
        engine.0.new_scene("Scene".to_string());

        let old_key = (0, 0);
        let new_key = (1, 1);
        let selection = FixtureSelection {
            fixtures: vec![old_key],
        };
        let mut animation = ActiveAnimation::new(&selection.fixtures, AnimationSpec::empty());
        animation.fixture_timers.get_mut(&old_key).unwrap().timer = 456;

        let scene = engine.0.scenes.get_mut(&0).unwrap();
        scene.sink.palette_assignments.insert(old_key, vec![3]);
        scene.sink.changeset.insert(FixtureSelector {
            gid: old_key.0,
            fid: old_key.1,
            property: FixtureProperty::Alpha,
        });
        scene
            .sink
            .active_animations
            .insert(selection, BTreeMap::from([(9, animation)]));

        engine.0.selection.group_ids.insert(0);
        engine.0.selection.fixtures_in_group.insert(0);

        assert_eq!(engine.move_fixture_to_group(0, 0, 1), Some(1));

        assert!(!engine.0.groups[&0].fixtures.contains_key(&0));
        assert_eq!(engine.0.groups[&1].fixtures[&1].name, "Moved");

        let scene = engine.0.scenes.get(&0).unwrap();
        assert!(!scene.sink.fixture_states.contains_key(&old_key));
        assert!(scene.sink.fixture_states.contains_key(&new_key));
        assert!(!scene.sink.palette_assignments.contains_key(&old_key));
        assert_eq!(scene.sink.palette_assignments[&new_key], vec![3]);
        assert!(scene.sink.changeset.contains(&FixtureSelector {
            gid: new_key.0,
            fid: new_key.1,
            property: FixtureProperty::Alpha,
        }));

        let (selection, animations) = scene.sink.active_animations.iter().next().unwrap();
        assert_eq!(selection.fixtures, vec![new_key]);
        assert_eq!(animations[&9].fixture_timers[&new_key].timer, 456);
        assert!(!animations[&9].fixture_timers.contains_key(&old_key));

        assert_eq!(engine.0.selection.group_ids, [1].into_iter().collect());
        assert_eq!(
            engine.0.selection.fixtures_in_group,
            [1].into_iter().collect()
        );
    }
}
