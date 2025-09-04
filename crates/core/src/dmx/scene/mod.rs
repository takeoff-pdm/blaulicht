use std::collections::{BTreeMap, HashMap, HashSet};

use blaulicht_shared::{ControlEvent, FixtureProperty};

use crate::dmx::{ActiveAnimation, FixtureGroup, FixtureSelection, FixtureState};

#[derive(Debug, Clone)]
pub struct Scene {
    pub sink: EngineSink,
    pub name: String,
}

// An `EngineSink` is a target to where the engine outputs the effects of control messages.
// So the selection is applied on this data type.
#[derive(Debug, Clone)]
pub struct EngineSink {
    pub fixture_states: BTreeMap<(u8, u8), FixtureState>,
    // These are the current active animations.
    // The BTreeMap maps an animation ID to an animation state.
    pub active_animations: HashMap<FixtureSelection, BTreeMap<u8, ActiveAnimation>>,

    // pub log: Vec<AtomicInstruction>,
    pub changeset: HashSet<FixtureSelector>,
}

impl EngineSink {
    pub fn apply_with_selection(
        &mut self,
        selection: &FixtureSelection,
        ev: ControlEvent,
    ) -> Vec<FixtureProperty> {
        // NOTE: ensure that panic below does not occur.
        debug_assert!(!selection.fixtures.is_empty());

        let properties = selection
            .fixtures
            .iter()
            .map(|selector| {
                println!("sink apply: (ev = {ev:?}) on {selector:?}");

                let fixture = self
                    .fixture_states
                    .get_mut(selector)
                    .expect("Expected scene sink to contain fixture {selector:?} but was missing");

                fixture.apply(ev.clone())
            })
            .last()
            .unwrap(); // NOTE: panics on empty selection, cannot happen

        properties
    }
}

#[derive(Debug, Clone)]
pub struct FixtureSelector {
    pub gid: u8,
    pub fid: u8,
    pub property: FixtureProperty,
}

// #[derive(Debug, Clone)]
// pub struct AtomicInstruction {
//     pub selector: FixtureSelector,
//     pub value: u8,
// }

impl EngineSink {
    pub fn from_groups(groups: &BTreeMap<u8, FixtureGroup>) -> Self {
        let fixture_states = groups
            .iter()
            .flat_map(|(group_id, group)| {
                group
                    .fixtures
                    .iter()
                    .map(|(fixture_id, _)| ((*group_id, *fixture_id), FixtureState::default()))
            })
            .collect();

        Self {
            fixture_states,
            active_animations: HashMap::new(),
            // log: vec![],
            changeset: HashSet::new(),
        }
    }
}
