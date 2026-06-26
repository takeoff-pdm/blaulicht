use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};

use strum::IntoEnumIterator;

use crate::{
    ActiveAnimation, AnimationSpeedModifier, ControlEvent, FixtureProperty,
    fixture::{
        state::{FixtureGroup, FixtureState},
        value::FixtureValue,
    },
    palette::Palette,
};

#[derive(Clone, PartialEq, Eq, Hash, Debug, Serialize, Deserialize, Encode, Decode)]
pub struct FixtureSelection {
    pub fixtures: Vec<(u8, u8)>,
}

impl FixtureSelection {
    pub fn sorted(&self) -> Self {
        let mut mapping = BTreeMap::new();

        for (gid, fid) in &self.fixtures {
            // gids.insert(gid);
            // fids.insert(fid);
            mapping.insert((gid, fid), ());
        }

        let mut sorted = vec![];

        for ((g, f), _) in &mapping {
            sorted.push((**g, **f));
        }

        Self { fixtures: sorted }
    }

    pub fn len(&self) -> usize {
        self.fixtures.len()
    }

    pub fn generate_instructions(&self) -> VecDeque<ControlEvent> {
        let mut gids = HashSet::new();
        let mut fids = HashSet::new(); // Maps fixture to a group.

        for (gid, fid) in &self.fixtures {
            gids.insert(gid);
            fids.insert(fid);
        }

        match (gids.len(), fids.len()) {
            (1, _) => {
                let mut instr = vec![ControlEvent::SelectGroup(**gids.iter().next().unwrap())];

                // Need to order the fixture ids.

                let mut ordered: Vec<_> = fids.iter().collect();
                ordered.sort();

                for fid in ordered {
                    // println!("FID: {fid}");
                    instr.push(ControlEvent::LimitSelectionToFixtureInCurrentGroup(**fid));
                }

                instr
            }
            .into(),
            (_, _) => {
                let mut group_instr = VecDeque::new();

                for gid in gids {
                    group_instr.push_back(ControlEvent::SelectGroup(*gid))
                }

                group_instr
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct Scene {
    pub sink: EngineSink,
    pub name: String,
}

impl Scene {
    pub fn get_fixture_changeset(&self, gid: u8, fid: u8) -> Vec<FixtureProperty> {
        let mut res = vec![];

        for change in &self.sink.changeset {
            if change.gid != gid || change.fid != fid {
                continue;
            }

            res.push(change.property);
        }

        res
    }
}

// An `EngineSink` is a target to where the engine outputs the effects of control messages.
// So the selection is applied on this data type.
#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct EngineSink {
    pub fixture_states: BTreeMap<(u8, u8), FixtureState>,
    // These are the current active animations.
    // The BTreeMap maps an animation ID to an animation state.
    pub active_animations: HashMap<FixtureSelection, BTreeMap<u8, ActiveAnimation>>,
    pub changeset: HashSet<FixtureSelector>,
    // Between 0-100 to multiply the alpha values of the scene's fixtures.
    pub master_alpha_fader: u8,
    // Multiply the speeds of all animations in this scene.
    pub master_speed: AnimationSpeedModifier,
    // Maps (group_id, fixture_id) -> list of assigned palette IDs.
    #[serde(default)]
    pub palette_assignments: BTreeMap<(u8, u8), Vec<u8>>,
}

impl EngineSink {
    /// Returns `true` if any targeted slot was rejected because it is
    /// currently bound to a palette (and therefore frozen).
    pub fn apply_with_selection(
        &mut self,
        selection: &FixtureSelection,
        ev: ControlEvent,
    ) -> bool {
        let mut any_rejected = false;
        for selector in &selection.fixtures {
            let fixture = self
                .fixture_states
                .get_mut(selector)
                .expect("Expected scene sink to contain fixture {selector:?} but was missing");

            let outcome = fixture.apply(ev.clone());
            if outcome.any_rejected() {
                any_rejected = true;
            }
            for property in outcome.changed {
                self.changeset.insert((*selector, property).into());
            }
        }
        any_rejected
    }

    pub fn active_animations(&self) -> &HashMap<FixtureSelection, BTreeMap<u8, ActiveAnimation>> {
        &self.active_animations
    }

    /// Re-syncs the property slots of every fixture state to mirror the
    /// palette IDs currently in `palette_assignments`. The "winning" palette
    /// for a property is the last one in the list that covers it (matching
    /// the render-time apply order). Properties no longer covered by any
    /// assigned palette are unbound to a literal of their resolved value.
    ///
    /// Maintains the invariant checked by `validate_palette_mapping_integrity`.
    pub fn sync_palette_bindings(&mut self, palettes: &BTreeMap<u8, Palette>) {
        let keys: Vec<(u8, u8)> = self.fixture_states.keys().copied().collect();
        for key in keys {
            let palette_ids = self
                .palette_assignments
                .get(&key)
                .cloned()
                .unwrap_or_default();
            let fixture_state = self.fixture_states.get_mut(&key).unwrap();
            sync_fixture_slots(fixture_state, &palette_ids, palettes);
        }
    }
}

fn sync_fixture_slots(
    fixture_state: &mut FixtureState,
    palette_ids: &[u8],
    palettes: &BTreeMap<u8, Palette>,
) {
    let mut winners: HashMap<FixtureProperty, u8> = HashMap::new();
    for palette_id in palette_ids {
        let Some(palette) = palettes.get(palette_id) else {
            continue;
        };
        for property in palette.kind.properties(palettes) {
            winners.insert(property, *palette_id);
        }
    }

    for property in FixtureProperty::iter() {
        match winners.get(&property) {
            Some(winner_id) => {
                *fixture_state.slot_mut(property) = FixtureValue::PalettePointer {
                    palette_id: *winner_id,
                    property: None,
                };
            }
            None => {
                if let FixtureValue::PalettePointer { .. } = fixture_state.slot(property) {
                    let resolved = fixture_state.slot(property).resolve(palettes, property);
                    *fixture_state.slot_mut(property) = FixtureValue::Literal(resolved);
                }
            }
        }
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize, Encode, Decode)]
pub struct FixtureSelector {
    pub gid: u8,
    pub fid: u8,
    pub property: FixtureProperty,
}

impl From<((u8, u8), FixtureProperty)> for FixtureSelector {
    fn from(value: ((u8, u8), FixtureProperty)) -> Self {
        Self {
            gid: value.0.0,
            fid: value.0.1,
            property: value.1,
        }
    }
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
            changeset: HashSet::new(),
            master_alpha_fader: 100,
            master_speed: AnimationSpeedModifier::_1,
            palette_assignments: BTreeMap::new(),
        }
    }
}
