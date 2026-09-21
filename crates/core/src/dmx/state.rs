use blaulicht_shared::{
    fixture::{state::FixtureState, value::FixtureValue},
    scene::{EngineSink, FixtureSelection, Scene, BLANK_SCENE_ID, BLANK_SCENE_NAME},
    view::View,
    AnimationSpec, AnimationSpecBody, AnimationSpecBodyPhaser, AnimationTemplate, EngineGroups,
    EngineSelection, FixtureProperty, MathematicalBaseFunction, MathematicalPhaser, PhaserDuration,
    PhaserKind, SyncMode,
};
use maplit::hashmap;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

//
// State.
//

//

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineState(pub blaulicht_shared::EngineState);

impl<'engine> EngineState {
    /// Cheap clone for read-only UI rendering: every scene except the
    /// currently focused one is replaced with an empty-sink placeholder
    /// that keeps only the scene name. This avoids the wholesale deep-clone
    /// of every scene's `fixture_states` (which is O(scenes * fixtures))
    /// on every frame.
    ///
    /// The returned `EngineState` must not be mutated — non-focused scenes
    /// have lost their state.
    pub fn clone_for_ui(&self) -> Self {
        let curr_id = self.0.current_scene_focus;

        let scenes = self
            .0
            .scenes
            .iter()
            .map(|(id, scene)| {
                if *id == curr_id {
                    (*id, scene.clone())
                } else {
                    (
                        *id,
                        Scene {
                            name: scene.name.clone(),
                            sink: EngineSink {
                                fixture_states: BTreeMap::new(),
                                active_animations: HashMap::new(),
                                changeset: HashSet::new(),
                                master_alpha_fader: scene.sink.master_alpha_fader,
                                master_speed: scene.sink.master_speed,
                                palette_assignments: BTreeMap::new(),
                            },
                        },
                    )
                }
            })
            .collect();

        Self(blaulicht_shared::EngineState {
            groups: self.0.groups.clone(),
            animation_templates: self.0.animation_templates.clone(),
            selection: self.0.selection.clone(),
            selection_stack: self.0.selection_stack.clone(),
            control_buffer: self.0.control_buffer.clone(),
            views: self.0.views.clone(),
            scenes,
            current_scene_focus: self.0.current_scene_focus,
            current_overlay_scenes: self.0.current_overlay_scenes.clone(),
            overrides: self.0.overrides.clone(),
            scene_graphs: self.0.scene_graphs.clone(),
            palettes: self.0.palettes.clone(),
            live_mode: self.0.live_mode,
        })
    }

    pub fn get_selection(&self) -> FixtureSelection {
        let group_ids = &self.0.selection.group_ids;
        let fixtures_in_group = &self.0.selection.fixtures_in_group;

        let mut fixtures_to_add = vec![];

        for (gid, group) in self.0.groups.iter().filter(|(k, _)| group_ids.contains(k)) {
            if group_ids.len() != 1 || fixtures_in_group.is_empty() {
                for fix_id in group.fixtures.keys() {
                    fixtures_to_add.push((*gid, *fix_id));
                }
            } else {
                for fix in fixtures_in_group {
                    if group.fixtures.contains_key(fix) {
                        fixtures_to_add.push((*gid, *fix));
                    }
                }
            }
        }

        FixtureSelection {
            fixtures: fixtures_to_add,
        }
    }

    /// Loads an engine state into the running engine.
    ///
    /// `migrate_legacy_base_scene` is for showfiles written before the base
    /// slot was handed to the ephemeral `BLANK` scene. Back then
    /// `current_scene_focus` *was* the rendered base layer, so it has to be
    /// folded into the overlay stack or the show would load dark. Newer
    /// showfiles pass `false`: there, focus is only the edit target and must
    /// not start playing just because it was selected when the file was saved.
    pub fn load_showfile(
        &mut self,
        mut other: blaulicht_shared::EngineState,
        migrate_legacy_base_scene: bool,
    ) {
        // Showfiles are external input: an out-of-range fixture address would
        // panic the DMX thread on the first render (unchecked universe indexing).
        for group in other.groups.values_mut() {
            for fixture in group.fixtures.values_mut() {
                let footprint = fixture.type_.footprint();
                if fixture.start_addr + footprint > 513 {
                    let clamped = 513usize.saturating_sub(footprint);
                    tracing::warn!(
                        "Fixture '{}' at address {} (footprint {footprint}) exceeds the universe; clamping to {clamped}",
                        fixture.name,
                        fixture.start_addr,
                    );
                    fixture.start_addr = clamped;
                }
            }
        }

        let groups = other.groups.clone();

        let overrides = self.0.overrides.clone();
        tracing::debug!("ov: {overrides:?}");

        let focus_is_valid = other.scenes.contains_key(&other.current_scene_focus)
            && other.current_scene_focus != BLANK_SCENE_ID;
        let current_scene_focus = match focus_is_valid {
            true => other.current_scene_focus,
            false => {
                tracing::debug!("Loading backup scene... | SCENES: {:?}", other.scenes);

                if other.scenes.is_empty() {
                    other.new_scene("Empty Scene".to_string());
                    tracing::debug!("CREATE BACKUP SCENE...");
                }

                let backup_id = other
                    .scenes
                    .keys()
                    .find(|id| **id != BLANK_SCENE_ID)
                    .unwrap();
                tracing::debug!("LOADED BACKUP ID: {backup_id}");
                *backup_id
            }
        };

        let spec_animations = other.animation_templates.clone();
        let valid_fixture_keys: HashSet<(u8, u8)> = groups
            .iter()
            .flat_map(|(gid, group)| group.fixtures.keys().map(move |fid| (*gid, *fid)))
            .collect();

        // This is actually required because the timetamps need to be reset to 0.
        let scenes: BTreeMap<u8, Scene> = other
            .scenes
            .into_iter()
            .map(|(k, mut scene)| {
                scene.sink.retain_fixture_keys(&valid_fixture_keys);
                let mut fixture_states = scene.sink.fixture_states;

                for (gid, group) in &groups {
                    for (fid, _) in &group.fixtures {
                        let selec = (*gid, *fid);
                        if fixture_states.get(&selec).is_none() {
                            tracing::debug!("============ FIX!!!");
                            fixture_states.insert(selec, FixtureState::default());
                        }
                    }
                }
                (
                    k,
                    Scene {
                        sink: EngineSink {
                            active_animations: scene
                                .sink
                                .active_animations
                                .into_iter()
                                .map(|(s, animations)| {
                                    (
                                        s,
                                        animations
                                            .into_iter()
                                            .filter_map(|(ak, a)| {
                                                let Some(anim_spec) = spec_animations.get(&ak) else {
                                                    tracing::warn!(
                                                        "Dropping active animation {ak}: template is missing"
                                                    );
                                                    return None;
                                                };
                                                let mut animation = a;
                                                animation.reset_timers(anim_spec.spec.sync_mode());
                                                Some((ak, animation))
                                            })
                                            .collect(),
                                    )
                                })
                                .collect(),
                            fixture_states,
                            ..scene.sink
                        },
                        ..scene
                    },
                )
            })
            .collect();

        // The focused scene may legitimately also be an overlay: focus is the
        // edit target, the base slot belongs to BLANK.
        other
            .current_overlay_scenes
            .retain(|scene_id| *scene_id != BLANK_SCENE_ID && scenes.contains_key(scene_id));

        if migrate_legacy_base_scene && !other.current_overlay_scenes.contains(&current_scene_focus)
        {
            other.current_overlay_scenes.insert(0, current_scene_focus);
        }

        let valid_scene_ids: HashSet<u8> = scenes.keys().copied().collect();
        other.scene_graphs.retain_scene_ids(&valid_scene_ids);

        *self = Self(blaulicht_shared::EngineState {
            selection: EngineSelection::default(),
            current_scene_focus,
            scenes,
            overrides,
            live_mode: false,
            ..other
        });

        // BLANK is ephemeral: it is absent from every showfile and is rebuilt
        // here against the freshly loaded fixture patch.
        self.0.ensure_blank_scene();
    }

    pub fn groups(&self) -> &EngineGroups {
        &self.0.groups
    }
    pub fn selection(&self) -> &EngineSelection {
        &self.0.selection
    }

    pub fn curr_scene(&'engine self) -> &'engine Scene {
        let curr_scene_id = self.0.current_scene_focus;
        self.0.scenes.get(&curr_scene_id).unwrap()
    }

    pub fn get_scene(&'engine self, id: u8) -> Option<&'engine Scene> {
        self.0.scenes.get(&id)
    }

    pub fn curr_scene_mut(&'engine mut self) -> &'engine mut Scene {
        let curr_scene_id = self.0.current_scene_focus;
        self.0.scenes.get_mut(&curr_scene_id).unwrap()
    }

    pub fn new_scene(&mut self, name: String) {
        self.0.new_scene(name)
    }

    pub fn clone_scene(&mut self, name: String) {
        self.0.clone_scene(name)
    }

    pub fn rename_scene(&mut self, scene_id: u8, name: String) -> bool {
        self.0.rename_scene(scene_id, name)
    }

    pub fn delete_scene(&mut self, scene_id: u8) -> bool {
        self.0.delete_scene(scene_id)
    }
}

// #[derive(Serialize, Deserialize, Debug)]
// pub struct FixtureGroupState {
//     pub fixtures: Vec<FixtureState>,
// }

// impl DmxEngine {
//     pub fn snapshot() -> EngineState {
//         todo!("Not implemented")
//     }
// }
//
//
//
//
//
//
//
//

impl Default for EngineState {
    fn default() -> Self {
        // let groups: BTreeMap<u8, FixtureGroup> = hashmap! {
        //     0 => FixtureGroup {
        //         name: "Basic".to_string(),
        //          fixtures: hashmap! {
        //             0 => Fixture::new(0, 1, "G0 FooBar".into(), FixtureType::MovingHead(MovingHead::MartinMac250E)),
        //             1 => Fixture::new(0, 20, "G0 BarQuux".into(),FixtureType::Light(Light::Generic3ChanNoAlpha)),
        //          }.into_iter().collect(),
        //     },
        //     1 => FixtureGroup {
        //         name: "Strobes".to_string(),
        //          fixtures: hashmap! {
        //             0 => Fixture::new(0, 30, "G1".into(), FixtureType::MovingHead(MovingHead::MartinMac250E))
        //          }.into_iter().collect(),
        //     },
        //     2 => FixtureGroup {
        //         name: "Strobes II".to_string(),
        //          fixtures: hashmap! {
        //             0 => Fixture::new(0, 50, "G2".into(), FixtureType::MovingHead(MovingHead::MartinMac250E))
        //          }.into_iter().collect(),
        //     },
        //     3 => FixtureGroup {
        //         name: "Strobes III".to_string(),
        //          fixtures: hashmap! {
        //             0 => Fixture::new(0, 60, "G3".into(), FixtureType::MovingHead(MovingHead::MartinMac250E)),
        //          }.into_iter().collect(),
        //     },
        // }
        // .into_iter()
        // .collect();
        //
        let groups = BTreeMap::new();

        let state = blaulicht_shared::EngineState {
            animation_templates: hashmap! {
                0 => AnimationSpec {
                    name: "Brightness Animation 0".into(),
                    property: FixtureProperty::Alpha,
                    body: AnimationSpecBody::Phaser(AnimationSpecBodyPhaser {
                        kind: PhaserKind::Mathematical(MathematicalPhaser {
                            base: MathematicalBaseFunction::Sin,
                            stretch_factor: 1.0,
                            amplitude_min: FixtureValue::Literal(0),
                            amplitude_max: FixtureValue::Literal(255)
                        }),
                        time_total: PhaserDuration::Fixed(1000),
                        pin_to_beat: false,
                        sync: SyncMode::Synced,
                        reverse_after_n_iterations: None,
                    }),
                },
                1 => AnimationSpec {
                    name: "Hue Animation 0".into(),
                    property: FixtureProperty::ColorHue,
                    body: AnimationSpecBody::Phaser(AnimationSpecBodyPhaser {
                        kind: PhaserKind::Mathematical(MathematicalPhaser {
                            base: MathematicalBaseFunction::Sin,
                            stretch_factor: 1.0,
                            amplitude_min: FixtureValue::Literal(0),
                            amplitude_max: FixtureValue::Literal(255)
                        }),
                        time_total: PhaserDuration::Fixed(1000),
                        pin_to_beat: false,
                        sync: SyncMode::Synced,
                        reverse_after_n_iterations: None,
                    }),
                },
                2 => AnimationSpec {
                    name: "Brightness Animation 1".into(),
                    property: FixtureProperty::Alpha,
                    body: AnimationSpecBody::Phaser(AnimationSpecBodyPhaser {
                        kind: PhaserKind::Mathematical(MathematicalPhaser {
                            base: MathematicalBaseFunction::EaseInOut,
                            stretch_factor: 1.0,
                            amplitude_min: FixtureValue::Literal(0),
                            amplitude_max: FixtureValue::Literal(255)
                        }),
                        time_total: PhaserDuration::Fixed(1000),
                        pin_to_beat: false,
                        sync: SyncMode::Synced,
                        reverse_after_n_iterations: None,
                    }),
                },
            }
            .into_iter()
            .map(|(k, spec)| (k, AnimationTemplate { spec }))
            .collect(),
            groups: groups.clone(),
            selection: Default::default(),
            selection_stack: VecDeque::new(),
            control_buffer: FixtureState::default(),
            views: hashmap! {
                0 => View::new("Default View".to_string(), vec![0])
            }
            .into_iter()
            .collect(),
            scenes: hashmap! {
               0 => Scene{
                   name: "Default Scene".to_string(),
                   sink: EngineSink::from_groups(&groups),
               },
               BLANK_SCENE_ID => Scene{
                   name: BLANK_SCENE_NAME.to_string(),
                   sink: EngineSink::from_groups(&groups),
               }
            }
            .into_iter()
            .collect(),
            current_scene_focus: 0,
            // The default scene plays as an overlay; BLANK holds the base slot.
            current_overlay_scenes: vec![0],
            overrides: BTreeMap::new(),
            scene_graphs: Default::default(),
            palettes: Default::default(),
            live_mode: false,
        };

        Self(state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use blaulicht_shared::fixture::{
        light::Light,
        state::{Fixture, FixtureGroup},
        FixtureType,
    };

    fn group_with_fixture() -> FixtureGroup {
        FixtureGroup {
            fixtures: BTreeMap::from([(
                0,
                Fixture::new(
                    0,
                    1,
                    "Fixture".to_string(),
                    FixtureType::from(Light::Generic3ChanNoAlpha),
                ),
            )]),
            ..FixtureGroup::default()
        }
    }

    #[test]
    fn multi_group_selection_ignores_single_group_fixture_filter() {
        let mut engine = EngineState::default();
        engine.0.groups.insert(0, group_with_fixture());
        engine.0.groups.insert(1, group_with_fixture());
        engine.0.selection.group_ids.extend([0, 1]);
        engine.0.selection.fixtures_in_group.insert(0);

        assert_eq!(engine.get_selection().fixtures, vec![(0, 0), (1, 0)]);
    }
}

#[cfg(test)]
mod blank_scene_load_tests {
    use super::*;
    use blaulicht_shared::fixture::{
        light::Light,
        state::{Fixture, FixtureGroup},
        FixtureType,
    };

    fn group_with_fixture() -> FixtureGroup {
        FixtureGroup {
            fixtures: BTreeMap::from([(
                0,
                Fixture::new(
                    0,
                    1,
                    "Fixture".to_string(),
                    FixtureType::from(Light::Generic3ChanNoAlpha),
                ),
            )]),
            ..FixtureGroup::default()
        }
    }

    fn showfile(focus: u8, overlays: Vec<u8>) -> blaulicht_shared::EngineState {
        let groups = BTreeMap::from([(0, group_with_fixture())]);
        blaulicht_shared::EngineState {
            scenes: BTreeMap::from([
                (
                    0,
                    Scene {
                        sink: EngineSink::from_groups(&groups),
                        name: "A".to_string(),
                    },
                ),
                (
                    1,
                    Scene {
                        sink: EngineSink::from_groups(&groups),
                        name: "B".to_string(),
                    },
                ),
            ]),
            groups,
            current_scene_focus: focus,
            current_overlay_scenes: overlays,
            ..Default::default()
        }
    }

    #[test]
    fn blank_scene_is_recreated_on_load_with_every_fixture() {
        let mut engine = EngineState::default();
        engine.load_showfile(showfile(0, vec![]), false);

        let blank = engine.0.scenes.get(&BLANK_SCENE_ID).expect("BLANK missing");
        assert_eq!(blank.name, BLANK_SCENE_NAME);
        assert!(blank.sink.fixture_states.contains_key(&(0, 0)));
        assert_eq!(engine.0.render_base_scene(), BLANK_SCENE_ID);
        assert!(!engine.0.live_mode);
    }

    #[test]
    fn legacy_showfile_promotes_its_base_scene_to_an_overlay() {
        // Pre-BLANK showfiles rendered `current_scene_focus` as the base. If it
        // did not become an overlay the show would load completely dark.
        let mut engine = EngineState::default();
        engine.load_showfile(showfile(1, vec![0]), true);

        assert_eq!(engine.0.current_overlay_scenes, vec![1, 0]);
        assert_eq!(engine.0.current_scene_focus, 1);
    }

    #[test]
    fn current_showfile_does_not_promote_the_edit_target() {
        let mut engine = EngineState::default();
        engine.load_showfile(showfile(1, vec![0]), false);

        // Focus is only what was being edited; it must not start playing.
        assert_eq!(engine.0.current_overlay_scenes, vec![0]);
        assert_eq!(engine.0.current_scene_focus, 1);
    }

    #[test]
    fn legacy_migration_does_not_duplicate_an_already_playing_base() {
        let mut engine = EngineState::default();
        engine.load_showfile(showfile(0, vec![0, 1]), true);
        assert_eq!(engine.0.current_overlay_scenes, vec![0, 1]);
    }

    #[test]
    fn focus_never_lands_on_blank_when_the_saved_focus_is_invalid() {
        let mut engine = EngineState::default();
        engine.load_showfile(showfile(200, vec![]), false);
        assert_ne!(engine.0.current_scene_focus, BLANK_SCENE_ID);
        assert!(engine.0.scenes.contains_key(&engine.0.current_scene_focus));
    }

    #[test]
    fn a_focused_scene_may_also_be_an_overlay() {
        // The whole point of the split: selecting a playing scene to edit it
        // must not pull it out of the live rig.
        let mut engine = EngineState::default();
        engine.load_showfile(showfile(0, vec![0, 1]), false);
        assert_eq!(engine.0.current_scene_focus, 0);
        assert!(engine.0.current_overlay_scenes.contains(&0));
    }
}
