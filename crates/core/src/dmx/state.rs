use crate::dmx::{
    animation::{
        AnimationSpec, AnimationSpecBody, AnimationSpecBodyPhaser, MathematicalBaseFunction,
        MathematicalPhaser, PhaserDuration, PhaserKind,
    },
    fixture::{
        Fixture, FixtureGroup, FixtureOrientation, FixtureState, FixtureType, Light, MovingHead,
    },
    scene::{EngineSink, Scene},
    FixtureSelection,
};
use blaulicht_shared::{AnimationSpeedModifier, FixtureProperty, RGBColor};
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
pub type EngineGroups = BTreeMap<u8, FixtureGroup>;

// Animations.
//

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct AnimationTimerState {
    pub last_tick_time: u64,
    pub timer: u64, // Counts up continously
}

impl AnimationTimerState {
    // Advances the timer variable
    pub fn tick(&mut self, now: u64) {
        self.timer += 1;
        self.last_tick_time = now;
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ActiveAnimation {
    // pub animation_id: u8,
    // 1/16 | 1/8 | 1/4 | 1/2 | 1 | 2 | 4 | 8
    pub speed_factor: AnimationSpeedModifier,
    // TODO: override parameters
    pub enabled: bool,
    // pub selection: EngineSelection,
    pub fixture_timers: BTreeMap<(u8, u8), AnimationTimerState>,
    pub sync: u8, // TOOD: placeholder type for the sync mode
                  //
}

impl ActiveAnimation {
    pub fn new(fixtures: &[(u8, u8)]) -> Self {
        let mut fixture_timers = BTreeMap::new();

        for key in fixtures {
            fixture_timers.insert(*key, AnimationTimerState::default());
        }

        Self {
            speed_factor: AnimationSpeedModifier::_1,
            enabled: false,
            fixture_timers,
            sync: 0, // TODO
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineState {
    // Strores the actual output state of all fixtures.
    pub groups: EngineGroups,

    // These are the reusable base animations.
    pub animations: BTreeMap<u8, AnimationSpec>,

    // Selection.
    pub selection: EngineSelection,
    pub selection_stack: VecDeque<EngineSelection>,

    // This is a buffer where control events are also being written into before they get applied on
    // fixtures, this is mainly useful for UI.
    // TODO: will be migrated to hashmap from selection -> control buffer maybe
    pub control_buffer: FixtureState,

    // ID 0 is reserved for the 'empty' scene.
    pub scenes: BTreeMap<u8, Scene>,

    pub current_scene_focus: u8,

    // First scene is the least-significant.
    pub current_overlay_scenes: Vec<u8>,

    // Overrides a (universe, channel) -> value
    pub overrides: BTreeMap<(usize, usize), u8>,
}

impl<'engine> EngineState {
    pub fn get_selection(&self) -> FixtureSelection {
        let group_ids = self.selection.group_ids.clone();
        let fixtures_in_group = self.selection.fixtures_in_group.clone();

        // let g_fixtures_mut = &mut group.1.fixtures;

        let mut fixtures_to_add = vec![];

        let groups_clone = self.groups.clone();

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

        println!("GOT SELECTION: {:?}", fixtures_to_add);

        FixtureSelection {
            fixtures: fixtures_to_add,
        }
    }

    pub fn load_showfile(&mut self, other: EngineState) {
        // This is actually required because the timetamps need to be reset to 0.
        let scenes =
            other
                .scenes
                .into_iter()
                .map(|(k, scene)| {
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
                                                .map(|(k, a)| {
                                                    (
                                                    k,
                                                    ActiveAnimation {
                                                        fixture_timers: a
                                                            .fixture_timers.into_keys().map(|k| {
                                                                (k, AnimationTimerState::default())
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
                                ..scene.sink
                            },
                            ..scene
                        },
                    )
                })
                .collect();

        *self = Self { scenes, ..other };
    }

    pub fn groups(&self) -> &EngineGroups {
        &self.groups
    }
    pub fn selection(&self) -> &EngineSelection {
        &self.selection
    }

    pub fn curr_scene(&'engine self) -> &'engine Scene {
        let curr_scene_id = self.current_scene_focus;
        self.scenes.get(&curr_scene_id).unwrap()
    }

    pub fn curr_scene_mut(&'engine mut self) -> &'engine mut Scene {
        let curr_scene_id = self.current_scene_focus;
        self.scenes.get_mut(&curr_scene_id).unwrap()
    }

    pub fn new_scene(&mut self, name: String) {
        // TODO: this fails when there are too many scenes.
        let new_id = self.scenes.len();
        debug_assert!(new_id == new_id as u8 as usize);

        self.scenes.insert(
            new_id as u8,
            Scene {
                sink: EngineSink::from_groups(self.groups()),
                name: name.clone(),
            },
        );
    }
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct EngineSelection {
    pub group_ids: HashSet<u8>,
    // This is only populated if there is one element in the group selection.
    pub fixtures_in_group: HashSet<u8>,
}

impl EngineSelection {
    pub fn is_empty(&self) -> bool {
        debug_assert!(
            (self.group_ids.len() == 1)
                || (self.group_ids.len() != 1 && self.fixtures_in_group.is_empty())
        );

        // if self.group_ids.is_empty() {
        //     return true;
        // }

        // self.fixtures_in_group.is_empty()
        self.group_ids.is_empty()
    }

    pub fn clear(&mut self) {
        self.group_ids.clear();
        self.fixtures_in_group.clear();
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
        let groups: BTreeMap<u8, FixtureGroup> = hashmap! {
            0 => FixtureGroup {
                name: "Basic".to_string(),
                 fixtures: hashmap! {
                    0 => Fixture::new(0, 1, "G0 FooBar".into(), FixtureType::MovingHead(MovingHead::MartinMacAura)),
                    1 => Fixture::new(0, 20, "G0 BarQuux".into(),FixtureType::Light(Light::Generic3ChanNoAlpha)),
                 }.into_iter().collect(),
            },
            1 => FixtureGroup {
                name: "Strobes".to_string(),
                 fixtures: hashmap! {
                    0 => Fixture::new(0, 30, "G1".into(), FixtureType::MovingHead(MovingHead::MartinMacAura))
                 }.into_iter().collect(),
            },
            2 => FixtureGroup {
                name: "Strobes II".to_string(),
                 fixtures: hashmap! {
                    0 => Fixture::new(0, 50, "G2".into(), FixtureType::MovingHead(MovingHead::MartinMacAura))
                 }.into_iter().collect(),
            },
            3 => FixtureGroup {
                name: "Strobes III".to_string(),
                 fixtures: hashmap! {
                    0 => Fixture::new(0, 60, "G3".into(), FixtureType::MovingHead(MovingHead::MartinMacAura)),
                 }.into_iter().collect(),
            },
        }
        .into_iter()
        .collect();

        Self {
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
        }
    }
}
