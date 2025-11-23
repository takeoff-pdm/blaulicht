use crate::{
    AnimationSpeedModifier, FixtureProperty, SyncMode,
    fixture::state::{FixtureGroup, FixtureState},
    scene::{EngineSink, Scene},
    view::View,
};
use bincode::{Decode, Encode, config};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    fmt::Display,
};
use strum::EnumIter;

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct SharedFixtureGroup {
    pub fixtures: Vec<u8>,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone, Encode, Decode)]
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

pub type EngineGroups = BTreeMap<u8, FixtureGroup>;

fn next_scene_id(scenes: &BTreeMap<u8, Scene>) -> Option<u8> {
    (0..=u8::MAX).find(|id| !scenes.contains_key(id))
}

#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode, Default)]
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

    pub views: BTreeMap<u8, View>,

    // ID 0 is reserved for the 'empty' scene.
    pub scenes: BTreeMap<u8, Scene>,

    pub current_scene_focus: u8,

    // First scene is the least-significant.
    pub current_overlay_scenes: Vec<u8>,

    // Overrides a (universe, channel) -> value
    pub overrides: BTreeMap<(usize, usize), u8>,

    #[serde(default)]
    pub plugin_state: HashMap<String, String>,
}

impl EngineState {
    pub fn new_scene(&mut self, name: String) {
        if let Some(new_id) = next_scene_id(&self.scenes) {
            self.scenes.insert(
                new_id,
                Scene {
                    sink: EngineSink::from_groups(&self.groups),
                    name,
                },
            );
        }
    }

    pub fn clone_scene(&mut self, name: String) {
        if let Some(new_id) = next_scene_id(&self.scenes) {
            if let Some(curr_scene) = self.scenes.get(&self.current_scene_focus) {
                self.scenes.insert(
                    new_id,
                    Scene {
                        sink: curr_scene.sink.clone(),
                        name,
                    },
                );
            }
        }
    }

    pub fn rename_scene(&mut self, scene_id: u8, name: String) -> bool {
        if let Some(scene) = self.scenes.get_mut(&scene_id) {
            scene.name = name;
            true
        } else {
            false
        }
    }

    pub fn delete_scene(&mut self, scene_id: u8) -> bool {
        if self.scenes.len() <= 1 {
            return false;
        }

        let removed = self.scenes.remove(&scene_id);
        if removed.is_none() {
            return false;
        }

        self.current_overlay_scenes
            .retain(|overlay_id| *overlay_id != scene_id);

        if self.current_scene_focus == scene_id {
            if let Some((&new_focus, _)) = self.scenes.iter().next() {
                self.current_scene_focus = new_focus;
            }
        } else if !self.scenes.contains_key(&self.current_scene_focus) {
            if let Some((&new_focus, _)) = self.scenes.iter().next() {
                self.current_scene_focus = new_focus;
            }
        }

        true
    }

    pub fn serialize(&self) -> Vec<u8> {
        bincode::encode_to_vec(self, config::standard()).unwrap()
    }

    pub fn deserialize(buf: &[u8]) -> Self {
        let res = bincode::decode_from_slice(buf, config::standard());

        let data = match res {
            Ok((data, _)) => data,
            Err(err) => {
                // Panic handler is usually not registered yet.
                panic!("Deserialize error: {}", err);
            }
        };

        data
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Encode, Decode)]
pub struct AnimationSpec {
    pub name: String,
    pub body: AnimationSpecBody,
    pub property: FixtureProperty,
    pub sync: SyncMode,
}

impl AnimationSpec {
    pub fn is_beat_pinned(&self) -> bool {
        match &self.body {
            AnimationSpecBody::Phaser(animation_spec_body_phaser) => {
                return animation_spec_body_phaser.pin_to_beat;
            }
            AnimationSpecBody::AudioVolume(_)
            | AnimationSpecBody::AudioBeat(_)
            | AnimationSpecBody::BeatClock(_)
            | AnimationSpecBody::Wasm(_) => false,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Encode, Decode)]
pub enum AnimationSpecBody {
    /// Phaser operates on a degree (0-360 DEG) an the amount is increased in time steps.
    Phaser(AnimationSpecBodyPhaser),
    AudioVolume(AnimationSpecBodyAudioVolume),
    AudioBeat(AnimationSpecBodyBeat),
    BeatClock(AnimationSpecBodyBeat),
    Wasm(AnimationSpecBodyWasm), // TODO: not currently supported.
}

#[derive(Debug, Serialize, Deserialize, Clone, Encode, Decode)]
pub struct AnimationSpecBodyPhaser {
    pub kind: PhaserKind,
    // Time to complete a complete cycle: cycle step time is calculated from this.
    pub time_total: PhaserDuration,
    pub pin_to_beat: bool,
}

#[derive(Debug, Serialize, Deserialize, Copy, Clone, PartialEq, Eq, Encode, Decode)]
pub enum PhaserDuration {
    Fixed(u64),                   // Total millis.
    Beat(AnimationSpeedModifier), // How many beats.
}

impl Display for PhaserDuration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PhaserDuration::Fixed(_) => write!(f, "fixed time"),
            PhaserDuration::Beat(_) => write!(f, "millis"),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Encode, Decode)]
pub enum PhaserKind {
    Mathematical(MathematicalPhaser),
    Keyframed(KeyframedPhaser),
}

#[derive(Debug, Serialize, Deserialize, Clone, Encode, Decode)]
pub struct MathematicalPhaser {
    pub base: MathematicalBaseFunction,
    // TODO: this should actually be deprecated!
    pub stretch_factor: f32, // Between 0-1.
    pub amplitude_min: u16,
    pub amplitude_max: u16,
}

#[derive(Debug, Serialize, Deserialize, Copy, Clone, EnumIter, PartialEq, Eq, Encode, Decode)]
pub enum MathematicalBaseFunction {
    Sin,
    Cos,
    Triangle,
    Square1_2,
    Square1_8,
    Sawtooth,
    EaseIn,
    EaseOut,
    EaseInOut,
}

impl Display for MathematicalBaseFunction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Encode, Decode)]
pub struct KeyframedPhaser {}

#[derive(Debug, Serialize, Deserialize, Clone, Encode, Decode)]
pub struct AnimationSpecBodyAudioVolume {}

#[derive(Debug, Serialize, Deserialize, Clone, Encode, Decode)]
pub struct AnimationSpecBodyBeat {}

#[derive(Debug, Serialize, Deserialize, Clone, Encode, Decode)]
pub struct AnimationSpecBodyWasm {}
