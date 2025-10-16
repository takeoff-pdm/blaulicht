use blaulicht_shared::{
    fixture::{
        light::Light,
        moving_head::MovingHead,
        state::{Fixture, FixtureGroup, FixtureState},
        FixtureType,
    },
    scene::{EngineSink, FixtureSelection, Scene},
    ActiveAnimation, AnimationSpec, AnimationSpecBody, AnimationSpecBodyPhaser,
    AnimationSpeedModifier, AnimationTimerState, EngineGroups, EngineSelection, FixtureProperty,
    MathematicalBaseFunction, MathematicalPhaser, PhaserDuration, PhaserKind, RGBColor, SyncMode,
};
use maplit::hashmap;
use serde::{Deserialize, Serialize};
use serialport::BreakDuration;
use std::{
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    time::Instant,
};

//
// State.
//

//

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineState(pub blaulicht_shared::EngineState);

// {
//     // Strores the actual output state of all fixtures.
//     pub groups: EngineGroups,
//
//     // These are the reusable base animations.
//     pub animations: BTreeMap<u8, AnimationSpec>,
//
//     // Selection.
//     pub selection: EngineSelection,
//     pub selection_stack: VecDeque<EngineSelection>,
//
//     // This is a buffer where control events are also being written into before they get applied on
//     // fixtures, this is mainly useful for UI.
//     // TODO: will be migrated to hashmap from selection -> control buffer maybe
//     pub control_buffer: FixtureState,
//
//     // ID 0 is reserved for the 'empty' scene.
//     pub scenes: BTreeMap<u8, Scene>,
//
//     pub current_scene_focus: u8,
//
//     // First scene is the least-significant.
//     pub current_overlay_scenes: Vec<u8>,
//
//     // Overrides a (universe, channel) -> value
//     pub overrides: BTreeMap<(usize, usize), u8>,
// }

impl<'engine> EngineState {
    pub fn get_selection(&self) -> FixtureSelection {
        let group_ids = self.0.selection.group_ids.clone();
        let fixtures_in_group = self.0.selection.fixtures_in_group.clone();

        // let g_fixtures_mut = &mut group.1.fixtures;

        let mut fixtures_to_add = vec![];

        let groups_clone = self.0.groups.clone();

        for group in groups_clone.iter().filter(|(k, _)| group_ids.contains(&k)) {
            if fixtures_in_group.is_empty() {
                for (fix_id, _) in &group.1.fixtures {
                    fixtures_to_add.push((*group.0, *fix_id));
                }
            } else {
                // let group_fixture = g_fixtures.values_mut();
                // fixtures.extend(group_fixture);
                for fix in fixtures_in_group.clone() {
                    fixtures_to_add.push((*group.0, fix));
                }
            }
        }

        FixtureSelection {
            fixtures: fixtures_to_add,
        }
    }

    pub fn load_showfile(&mut self, mut other: blaulicht_shared::EngineState) {
        let groups = other.groups.clone();

        let overrides = self.0.overrides.clone();
        println!("ov: {overrides:?}");

        let current_scene_focus = match &other.scenes.contains_key(&other.current_scene_focus) {
            true => other.current_scene_focus,
            false => {
                println!("Loading backup scene... | SCENES: {:?}", other.scenes);

                if other.scenes.is_empty() {
                    other.new_scene("Empty Scene".to_string());
                    println!("CREATE BACKUP SCENE...");
                }

                let backup_id = other.scenes.keys().next().unwrap();
                println!("LOADED BACKUP ID: {backup_id}");
                *backup_id
            }
        };

        let spec_animations = other.animations.clone();

        // This is actually required because the timetamps need to be reset to 0.
        let scenes = other
            .scenes
            .into_iter()
            .map(|(k, scene)| {
                let mut fixture_states = scene.sink.fixture_states;

                for (gid, group) in &groups {
                    for (fid, _) in &group.fixtures {
                        let selec = (*gid, *fid);
                        if fixture_states.get(&selec).is_none() {
                            println!("============ FIX!!!");
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
                                            .map(|(ak, a)| {
                                                let anim_spec = spec_animations.get(&ak).unwrap();
                                                let amount = a.fixture_timers.len();

                                                (
                                                    ak,
                                                    ActiveAnimation {
                                                        fixture_timers: a
                                                            .fixture_timers
                                                            .into_iter()
                                                            .enumerate()
                                                            .map(|(counter, (k, v))| {
                                                                let timer = match anim_spec.sync {
                                                                    SyncMode::Synced => 0,
                                                                    SyncMode::StretchedEven => {
                                                                        ((360.0 / amount as f32)
                                                                            * counter as f32)
                                                                            as u64
                                                                    }
                                                                    SyncMode::StretchedHalfHalf => {
                                                                        (180 * (counter % 2)) as u64
                                                                    }
                                                                };

                                                                (
                                                                    k,
                                                                    AnimationTimerState {
                                                                        last_tick_time: 0,
                                                                        timer,
                                                                    },
                                                                )
                                                            })
                                                            .collect(),
                                                        ..a
                                                    },
                                                )
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

        *self = Self(blaulicht_shared::EngineState {
            selection: EngineSelection::default(),
            current_scene_focus,
            scenes,
            overrides,
            ..other
        });
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
            animations: hashmap! {
                0 => AnimationSpec {
                    name: "Brightness Animation 0".into(),
                    property: FixtureProperty::Alpha,
                    body: AnimationSpecBody::Phaser(AnimationSpecBodyPhaser {
                        kind: PhaserKind::Mathematical(MathematicalPhaser {
                            base: MathematicalBaseFunction::Sin,
                            stretch_factor: 1.0,
                            amplitude_min: 0,
                            amplitude_max: 255
                        }),
                        time_total: PhaserDuration::Fixed(1000),
                    }),
                    sync: SyncMode::Synced,
                },
                1 => AnimationSpec {
                    name: "Hue Animation 0".into(),
                    property: FixtureProperty::ColorHue,
                    body: AnimationSpecBody::Phaser(AnimationSpecBodyPhaser {
                        kind: PhaserKind::Mathematical(MathematicalPhaser {
                            base: MathematicalBaseFunction::Sin,
                            stretch_factor: 1.0,
                            amplitude_min: 0,
                            amplitude_max: 255
                        }),
                        time_total: PhaserDuration::Fixed(1000),
                    }),
                    sync: SyncMode::Synced,
                },
                2 => AnimationSpec {
                    name: "Brightness Animation 1".into(),
                    property: FixtureProperty::Alpha,
                    body: AnimationSpecBody::Phaser(AnimationSpecBodyPhaser {
                        kind: PhaserKind::Mathematical(MathematicalPhaser {
                            base: MathematicalBaseFunction::EaseInOut,
                            stretch_factor: 1.0,
                            amplitude_min: 0,
                            amplitude_max: 255
                        }),
                        time_total: PhaserDuration::Fixed(1000),
                    }),
                    sync: SyncMode::Synced,
                },
            }
            .into_iter()
            .collect(),
            groups: groups.clone(),
            selection: Default::default(),
            selection_stack: VecDeque::new(),
            control_buffer: FixtureState::default(),
            // active_animations: HashMap::new(),
            scenes: hashmap! {
               0 => Scene{
                   name: "Default Scene".to_string(),
                   sink: EngineSink::from_groups(&groups),
               }
            }
            .into_iter()
            .collect(),
            current_scene_focus: 0,
            current_overlay_scenes: vec![],
            overrides: BTreeMap::new(),
            plugin_state: std::collections::HashMap::new(),
        };

        Self(state)
    }
}
