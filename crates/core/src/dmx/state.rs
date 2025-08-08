use crate::dmx::{
    animation::{
        AnimationSpec, AnimationSpecBody, AnimationSpecBodyPhaser, MathematicalBaseFunction,
        MathematicalPhaser, PhaserDuration, PhaserKind,
    },
    fixture::{
        Fixture, FixtureGroup, FixtureOrientation, FixtureState, FixtureType, Light, MovingHead,
    },
};
use blaulicht_shared::{Color, FixtureProperty};
use maplit::hashmap;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet, VecDeque};

//
// State.
//

pub type EngineGroups = BTreeMap<u8, FixtureGroup>;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EngineState {
    // TODO: how to solve this?
    // fixtures: Vec<Fixture>,
    // Assigns an ID to a group.
    pub groups: EngineGroups,
    // Selection.
    pub selection: EngineSelection,
    pub selection_stack: VecDeque<EngineSelection>,

    // This is a buffer where control events are also being written into before they get applied on
    // fixtures.
    pub control_buffer: FixtureState,

    pub animations: BTreeMap<u8, AnimationSpec>,
}

impl EngineState {
    pub fn groups(&self) -> &EngineGroups {
        &self.groups
    }
    pub fn selection(&self) -> &EngineSelection {
        &self.selection
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
        Self {
            animations: hashmap! {
                0 => AnimationSpec {
                    name: "Brightness Animation 0".into(),
                    property: FixtureProperty::Brightness,
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
                    property: FixtureProperty::Brightness,
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
            groups: hashmap! {
                0 => FixtureGroup {
                     fixtures: hashmap! {
                        0 => Fixture {
                            name: "G0 FooBar".into(),
                             type_: FixtureType::MovingHead(MovingHead::MartinMacAura),
                              state: FixtureState {
                                start_addr: 42,
                                color: Color::default(),
                                alpha: 0,
                                orientation: FixtureOrientation::default(),
                                strobe_speed: 0,
                                animations: BTreeMap::new(),
                            },
                            pos: (1, 2).into(),
                        },
                        1 => Fixture {
                            name: "G0 BarQuux".into(),
                             type_: FixtureType::Light(Light::Generic3ChanNoAlpha),
                              state: FixtureState {
                                start_addr: 69,
                                color: Color::default(),
                                alpha: 0,
                                orientation: FixtureOrientation::default(),
                                strobe_speed: 0,
                                animations: BTreeMap::new(),
                            },
                            pos: (1, 3).into(),
                        }
                     }.into_iter().collect(),
                },
                1 => FixtureGroup {
                     fixtures: hashmap! {
                        0 => Fixture {
                            name: "G1".into(),
                             type_: FixtureType::MovingHead(MovingHead::MartinMacAura),
                              state: FixtureState {
                                start_addr: 142,
                                color: Color::default(),
                                alpha: 0,
                                orientation: FixtureOrientation::default(),
                                strobe_speed: 0,
                                animations: BTreeMap::new(),
                            },
                            pos: (1, 4).into(),
                        },
                     }.into_iter().collect(),
                },
                2 => FixtureGroup {
                     fixtures: hashmap! {
                        0 => Fixture {
                            name: "G2".into(),
                             type_: FixtureType::MovingHead(MovingHead::MartinMacAura),
                              state: FixtureState {
                                start_addr: 169,
                                color: Color::default(),
                                alpha: 0,
                                orientation: FixtureOrientation::default(),
                                strobe_speed: 0,
                                animations: BTreeMap::new(),
                            },
                            pos: (1, 5).into(),
                        },
                     }.into_iter().collect(),
                },
                3 => FixtureGroup {
                     fixtures: hashmap! {
                        0 => Fixture {
                            name: "G3".into(),
                             type_: FixtureType::MovingHead(MovingHead::MartinMacAura),
                              state: FixtureState {
                                start_addr: 242,
                                color: Color::default(),
                                alpha: 0,
                                orientation: FixtureOrientation::default(),
                                strobe_speed: 0,
                                animations: BTreeMap::new(),
                            },
                            pos: (1, 6).into(),
                        },
                     }.into_iter().collect(),
                },
            }
            .into_iter()
            .collect(),
            selection: Default::default(),
            selection_stack: VecDeque::new(),
            control_buffer: FixtureState::default(),
        }
    }
}
