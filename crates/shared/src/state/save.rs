use crate::{
    ActiveAnimation,
    engine::{AnimationSpec, EngineState},
    fixture::state::{Fixture, FixtureState},
    scene::{FixtureSelection, FixtureSelector, Scene},
    view::View,
};
use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::hash::Hash;

#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct SaveFixtureGroup {
    pub name: String,
    pub fixtures: Vec<SavedMapEntry<u8, Fixture>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
/// Animation spec entry saved
pub struct SavedMapEntry<K, V> {
    key: K,
    value: V,
}

impl<K, V> SavedMapEntry<K, V>
where
    K: Eq + Hash,
{
    pub fn from_btree_map(from: BTreeMap<K, V>) -> Vec<Self> {
        from.into_iter()
            .map(|(key, value)| Self { key, value })
            .collect()
    }

    pub fn from_map(from: HashMap<K, V>) -> Vec<Self> {
        from.into_iter()
            .map(|(key, value)| Self { key, value })
            .collect()
    }

    pub fn to_map(from: Vec<Self>) -> HashMap<K, V> {
        from.into_iter()
            .map(|elem| (elem.key, elem.value))
            .collect()
    }
}

/// Wrapper around the engine state to allow more flexible exporting / importing.
#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode, Default)]
pub struct SaveEngineState {
    pub groups: Vec<SavedMapEntry<u8, SaveFixtureGroup>>,
    pub animations: Vec<SavedMapEntry<u8, AnimationSpec>>,
    pub views: Vec<SavedMapEntry<u8, View>>,
    pub scenes: Vec<SavedMapEntry<u8, SavedScene>>,
    pub current_scene_focus: u8,
    pub current_overlay_scenes: Vec<u8>,
    pub overrides: Vec<SavedMapEntry<(usize, usize), u8>>,
    #[serde(default)]
    pub plugin_state: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct SavedScene {
    pub sink: SavedEngineSink,
    pub name: String,
}

impl From<Scene> for SavedScene {
    fn from(value: Scene) -> Self {
        let active_animations = value
            .sink
            .active_animations
            .into_iter()
            .map(|(key, animation_map)| SavedMapEntry {
                key,
                value: SavedMapEntry::from_btree_map(animation_map),
            })
            .collect();

        Self {
            sink: SavedEngineSink {
                fixture_states: SavedMapEntry::from_btree_map(value.sink.fixture_states),
                active_animations,
                changeset: value.sink.changeset.into_iter().collect(),
            },
            name: value.name,
        }
    }
}

impl TryFrom<SavedScene> for Scene {
    type Error = String;

    fn try_from(value: SavedScene) -> Result<Self, Self::Error> {
        todo!()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct SavedEngineSink {
    pub fixture_states: Vec<SavedMapEntry<(u8, u8), FixtureState>>,
    pub active_animations:
        Vec<SavedMapEntry<FixtureSelection, Vec<SavedMapEntry<u8, ActiveAnimation>>>>,
    pub changeset: Vec<FixtureSelector>,
}

/// From `EngineState` to `SaveEngineState`
impl From<EngineState> for SaveEngineState {
    fn from(value: EngineState) -> Self {
        let groups = value
            .groups
            .into_iter()
            .map(|(key, group)| {
                let fixtures = SavedMapEntry::from_btree_map(group.fixtures);
                SavedMapEntry {
                    key,
                    value: SaveFixtureGroup {
                        name: group.name,
                        fixtures,
                    },
                }
            })
            .collect();

        let animations = SavedMapEntry::from_btree_map(value.animations);

        let views = value
            .views
            .into_iter()
            .map(|(key, view)| SavedMapEntry { key, value: view })
            .collect();

        let scenes = value
            .scenes
            .into_iter()
            .map(|(key, scene)| SavedMapEntry {
                key,
                value: SavedScene::from(scene),
            })
            .collect();

        Self {
            groups,
            animations,
            views,
            scenes,
            current_scene_focus: value.current_scene_focus,
            current_overlay_scenes: value.current_overlay_scenes,
            overrides: SavedMapEntry::from_btree_map(value.overrides),
            plugin_state: value.plugin_state,
        }
    }
}

/// From `SaveEngineState` to `EngineState`
impl TryFrom<SaveEngineState> for EngineState {
    type Error = String;

    fn try_from(value: SaveEngineState) -> Result<Self, Self::Error> {
        todo!()
    }
}

// pub fn engine_state_to_json(from: EngineState) -> serde_json::Result<String> {
//     let converted = SaveEngineState::from(from);
//     println!("conv: {:?}", converted);
//     serde_json::to_string(&converted)
// }
