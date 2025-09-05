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

#[derive(Debug, Clone)]
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
}

impl<'engine> EngineState {
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
                 fixtures: hashmap! {
                    0 => Fixture {
                        name: "G0 FooBar".into(),
                         type_: FixtureType::MovingHead(MovingHead::MartinMacAura),
                        //   state: FixtureState {
                        //     start_addr: 42,
                        //     color: Color::default(),
                        //     alpha: 0,
                        //     orientation: FixtureOrientation::default(),
                        //     strobe_speed: 0,
                        // },
                        pos: (1, 2).into(),
                    },
                    1 => Fixture {
                        name: "G0 BarQuux".into(),
                         type_: FixtureType::Light(Light::Generic3ChanNoAlpha),
                        //   state: FixtureState {
                        //     start_addr: 69,
                        //     color: Color::default(),
                        //     alpha: 0,
                        //     orientation: FixtureOrientation::default(),
                        //     strobe_speed: 0,
                        // },
                        pos: (1, 3).into(),
                    }
                 }.into_iter().collect(),
            },
            1 => FixtureGroup {
                 fixtures: hashmap! {
                    0 => Fixture {
                        name: "G1".into(),
                         type_: FixtureType::MovingHead(MovingHead::MartinMacAura),
                        //   state: FixtureState {
                        //     start_addr: 142,
                        //     color: Color::default(),
                        //     alpha: 0,
                        //     orientation: FixtureOrientation::default(),
                        //     strobe_speed: 0,
                        // },
                        pos: (1, 4).into(),
                    },
                 }.into_iter().collect(),
            },
            2 => FixtureGroup {
                 fixtures: hashmap! {
                    0 => Fixture {
                        name: "G2".into(),
                         type_: FixtureType::MovingHead(MovingHead::MartinMacAura),
                        //   state: FixtureState {
                        //     start_addr: 169,
                        //     color: Color::default(),
                        //     alpha: 0,
                        //     orientation: FixtureOrientation::default(),
                        //     strobe_speed: 0,
                        // },
                        pos: (1, 5).into(),
                    },
                 }.into_iter().collect(),
            },
            3 => FixtureGroup {
                 fixtures: hashmap! {
                    0 => Fixture {
                        name: "G3".into(),
                         type_: FixtureType::MovingHead(MovingHead::MartinMacAura),
                        //   state: FixtureState {
                        //     start_addr: 242,
                        //     color: Color::default(),
                        //     alpha: 0,
                        //     orientation: FixtureOrientation::default(),
                        //     strobe_speed: 0,
                        // },
                        pos: (1, 6).into(),
                    },
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
        }
    }
}
