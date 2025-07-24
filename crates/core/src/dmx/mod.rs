mod clock;
mod fixture;

use crate::{
    dmx::{
        fixture::{
            Fixture, FixtureGroup, FixtureOrientation, FixtureState, FixtureType, Light, MovingHead,
        },
    },
    event::{SystemEventBusConnection, SystemEventBusConnectionInst},
    msg::SystemMessage,
    routes::AppState,
};
use blaulicht_shared::{
    Color, ControlEvent, ControlEventMessage, EventOriginator, CONTROLS_REQUIRING_SELECTION
};
use crossbeam_channel::Sender;
use maplit::hashmap;
use serde::{Deserialize, Serialize};
use std::{
    borrow::Cow,
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    hash::Hash,
    sync::{Arc, RwLock, RwLockWriteGuard},
};

/// This module deals with applying events on fixtures to produce a continuous DMX output.

pub struct FixtureSelection {
    fixtures: Vec<(u8, u8)>,
}

impl Fixture {
    fn apply(&mut self, ev: ControlEvent) {
        match ev {
            ControlEvent::SetEnabled(enabled) => {
                todo!("not supported");
            }
            ControlEvent::SetBrightness(brightness) => {
                self.set_brightness(brightness);
            }
            ControlEvent::SetColor(clr) => {
                self.set_color(clr);
            }
            ControlEvent::MiscEvent { descriptor, value } => todo!(),
            _ => {}
        }
    }
}

// TODO: maybe fuse this together?

impl FixtureState {
    fn apply(&mut self, ev: ControlEvent) {
        match ev {
            ControlEvent::SetBrightness(brightness) => {
                self.brightness = brightness;
            }
            ControlEvent::SetColor(clr) => {
                self.color = clr.into();
            }
            _ => {}
        }
    }
}

impl FixtureSelection {
    fn apply(
        &mut self,
        state: &mut RwLockWriteGuard<'_, EngineState>,
        ev: ControlEvent,
    ) -> Option<&'static str> {
        for (group_id, fix_id) in &self.fixtures {
            println!("apply: {group_id} {fix_id}");
            let fixture = state
                .groups
                .get_mut(group_id)
                .unwrap()
                .fixtures
                .get_mut(fix_id)
                .unwrap();

            fixture.apply(ev);
        }

        state.control_buffer.apply(ev);

        None
    }
}

pub struct DmxEngine {
    // TODO: check if this is too slow.
    state_ref: Arc<AppState>,
    pub dmx: [u8; 513],      // Starting at 1
    dmx_previous: [u8; 513], // Starting at 1
    // TODO: add more, internal state.
    event_bus_connection: SystemEventBusConnectionInst,
    system_out: Sender<SystemMessage>,
}

impl DmxEngine {
    pub fn new(
        state_ref: Arc<AppState>,
        event_bus_connection: SystemEventBusConnectionInst,
        system_out: Sender<SystemMessage>,
    ) -> Self {
        Self {
            state_ref,
            dmx_previous: [0; 513],
            dmx: [0; 513],
            event_bus_connection,
            system_out,
        }
    }

    /// Returns whether the DMX buffer changed.
    fn tick_internal(&mut self) -> bool {
        // Read back events.
        let mut events = vec![];
        loop {
            match self.event_bus_connection.try_recv() {
                Some(ev) => events.push(ev),
                None => break,
            }
        }

        if !events.is_empty() {
            let mut state = self.state_ref.dmx_engine.write().unwrap();

            for ev in events {
                let (msg, event) = self.apply(&mut state, ev);

                if let Some(msg) = msg {
                    self.system_out
                        .send(SystemMessage::Log(msg.to_string()))
                        .unwrap();
                }

                if let Some(ev) = event {
                    self.event_bus_connection
                        .send(ControlEventMessage::new(EventOriginator::DmxEngine, ev));
                }
            }
        }

        // Only read if there were no events.
        self.update_dmx_buffer();
        // TODO: flatten to DMX buffer.

        // Return true if the dmx buffer changed.
        if self.dmx != self.dmx_previous {
            self.dmx_previous = self.dmx;
            true
        } else {
            false
        }
    }

    pub fn tick(&mut self) {
        if self.tick_internal() {
            println!("changed dmx...");

            // TODO: THIS might be too heavy.
            // self.system_out
            //     .send(SystemMessage::DMX(Box::new(self.dmx)))
            //     .unwrap();
        }
    }

    fn update_dmx_buffer(&mut self) {
        let state = self.state_ref.dmx_engine.read().unwrap();

        for group in &state.groups {
            for fixture in &group.1.fixtures {
                let fix = fixture.1;
                fix.write(&mut self.dmx);
            }
        }
    }

    fn get_selection<'engine>(
        &self,
        state: &'engine mut RwLockWriteGuard<'_, EngineState>,
    ) -> FixtureSelection {
        let group_ids = state.selection.group_ids.clone();
        let fixtures_in_group = state.selection.fixtures_in_group.clone();

        // let g_fixtures_mut = &mut group.1.fixtures;

        let mut fixtures_to_add = vec![];

        let groups_clone = state.groups.clone();

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

    pub fn apply(
        &self,
        state: &mut RwLockWriteGuard<'_, EngineState>,
        ev: ControlEventMessage,
    ) -> (Option<&'static str>, Option<ControlEvent>) {
        // TODO: is this valid?
        if ev.originator() == EventOriginator::DmxEngine {
            return (None, None);
        }

        // Require selection.
        let requires_selection = ev.requires_selection();
        if requires_selection && state.selection.is_empty() {
            return (Some("No selected object(s)"), None);
        }

        // Empties the fixture state buffer on selecion events.
        if !requires_selection {
            state.control_buffer = FixtureState::default();
        }

        let mut selection = self.get_selection(state);

        // Match event.
        match ev.body() {
            ControlEvent::SelectGroup(group_id) => {
                if !state.groups.contains_key(&group_id) {
                    return (
                        Some("Illegal group"),
                        Some(ControlEvent::DeSelectGroup(group_id)),
                    );
                }

                if !state.selection.group_ids.insert(group_id) {
                    (Some("Already selected"), None)
                } else {
                    (None, None)
                }
            }
            ControlEvent::DeSelectGroup(group_id) => {
                if !state.groups.contains_key(&group_id) {
                    return (
                        Some("Illegal group"),
                        Some(ControlEvent::DeSelectGroup(group_id)),
                    );
                }

                if !state.selection.group_ids.remove(&group_id) {
                    (Some("Not selected"), None)
                } else {
                    (None, None)
                }
            }
            ControlEvent::LimitSelectionToFixtureInCurrentGroup(fixture_id) => {
                if state.selection.group_ids.len() != 1 {
                    return (
                        Some("Exactly 1 group shall be selected"),
                        Some(ControlEvent::UnLimitSelectionToFixtureInCurrentGroup(
                            fixture_id,
                        )),
                    );
                }

                if !state.selection.fixtures_in_group.insert(fixture_id) {
                    (Some("Already selected"), None)
                } else {
                    (None, None)
                }
            }
            ControlEvent::UnLimitSelectionToFixtureInCurrentGroup(fixture_id) => {
                if state.selection.group_ids.len() != 1 {
                    return (Some("Exactly 1 group shall be selected"), None);
                }

                if !state.selection.fixtures_in_group.remove(&fixture_id) {
                    (Some("Not selected"), None)
                } else {
                    (None, None)
                }
            }
            ControlEvent::RemoveSelection => {
                if state.selection.is_empty() {
                    (Some("No selection"), None)
                } else {
                    (None, None)
                }
            }
            ControlEvent::RemoveAllSelection => {
                state.selection.clear();
                (None, None)
            }
            ControlEvent::PushSelection => {
                let selection = state.selection.clone();

                if let Some(top) = state.selection_stack.front() {
                    if selection.is_empty() && top.is_empty() {
                        return (Some("Push to empty selection"), None);
                    }
                }

                state.selection_stack.push_front(selection);
                state.selection.clear();
                (None, None)
            }
            ControlEvent::PopSelection => {
                if let Some(top) = state.selection_stack.pop_front() {
                    state.selection = top;
                    (None, None)
                } else {
                    (Some("Selection stack empty"), None)
                }
            }
            ControlEvent::MiscEvent { descriptor, value } => {
                todo!("Not implemented");
            }
            CONTROLS_REQUIRING_SELECTION!() => (selection.apply(state, ev.body()), None),
        }
    }
}

//
// State.
//

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EngineState {
    // TODO: how to solve this?
    // fixtures: Vec<Fixture>,
    // Assigns an ID to a group.
    groups: BTreeMap<u8, FixtureGroup>,
    // Selection.
    selection: EngineSelection,
    selection_stack: VecDeque<EngineSelection>,

    // This is a buffer where control events are also being written into before they get applied on
    // fixtures.
    pub control_buffer: FixtureState,
}

impl Default for EngineState {
    fn default() -> Self {
        Self {
            groups: hashmap! {
                0 => FixtureGroup {
                     fixtures: hashmap! {
                        0 => Fixture {
                            name: "FooBar".into(),
                             type_: FixtureType::MovingHead(MovingHead::MartinMacAura),
                              state: FixtureState {
                                start_addr: 42,
                                brightness: 0,
                                color: Color::default(),
                                alpha: 0,
                                orientation: FixtureOrientation::default(),
                                strobe_speed: 0,
                            }
                        },
                        1 => Fixture {
                            name: "BarQuux".into(),
                             type_: FixtureType::Light(Light::Generic3ChanNoAlpha),
                              state: FixtureState {
                                start_addr: 69,
                                brightness: 0,
                                color: Color::default(),
                                alpha: 0,
                                orientation: FixtureOrientation::default(),
                                strobe_speed: 0,
                            }
                        }
                     }.into_iter().collect(),
                },
                1 => FixtureGroup {
                     fixtures: hashmap! {
                        0 => Fixture {
                            name: "G2".into(),
                             type_: FixtureType::MovingHead(MovingHead::MartinMacAura),
                              state: FixtureState {
                                start_addr: 42,
                                brightness: 0,
                                color: Color::default(),
                                alpha: 0,
                                orientation: FixtureOrientation::default(),
                                strobe_speed: 0,
                            }
                        },
                     }.into_iter().collect(),
                },
                2 => FixtureGroup {
                     fixtures: hashmap! {
                        0 => Fixture {
                            name: "G2".into(),
                             type_: FixtureType::MovingHead(MovingHead::MartinMacAura),
                              state: FixtureState {
                                start_addr: 42,
                                brightness: 0,
                                color: Color::default(),
                                alpha: 0,
                                orientation: FixtureOrientation::default(),
                                strobe_speed: 0,
                            }
                        },
                     }.into_iter().collect(),
                },
                3 => FixtureGroup {
                     fixtures: hashmap! {
                        0 => Fixture {
                            name: "G2".into(),
                             type_: FixtureType::MovingHead(MovingHead::MartinMacAura),
                              state: FixtureState {
                                start_addr: 42,
                                brightness: 0,
                                color: Color::default(),
                                alpha: 0,
                                orientation: FixtureOrientation::default(),
                                strobe_speed: 0,
                            }
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

impl EngineState {
    pub fn groups(&self) -> &BTreeMap<u8, FixtureGroup> {
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
