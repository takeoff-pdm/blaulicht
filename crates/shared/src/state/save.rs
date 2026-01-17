use crate::{
    ActiveAnimation, AnimationSpeedModifier, AnimationTemplate, AnimationTimerState,
    EngineSelection,
    engine::{AnimationSpec, EngineState},
    fixture::state::{Fixture, FixtureGroup, FixtureState},
    scene::{EngineSink, FixtureSelection, FixtureSelector, Scene},
    view::View,
};
use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::hash::Hash;
use std::net::SocketAddr;

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

impl<K, V> SavedMapEntry<K, V> {
    pub fn from_btree_map(from: BTreeMap<K, V>) -> Vec<Self> {
        from.into_iter()
            .map(|(key, value)| Self { key, value })
            .collect()
    }

    pub fn to_btree_map(from: Vec<Self>) -> BTreeMap<K, V>
    where
        K: Ord,
    {
        from.into_iter().map(Self::into_pair).collect()
    }

    fn into_pair(self) -> (K, V) {
        (self.key, self.value)
    }
}

impl<K, V> SavedMapEntry<K, V>
where
    K: Eq + Hash,
{
    pub fn from_map(from: HashMap<K, V>) -> Vec<Self> {
        from.into_iter()
            .map(|(key, value)| Self { key, value })
            .collect()
    }

    pub fn to_map(from: Vec<Self>) -> HashMap<K, V> {
        from.into_iter().map(Self::into_pair).collect()
    }
}

/// Wrapper around the engine state to allow more flexible exporting / importing.
#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode, Default)]
pub struct SaveEngineState {
    pub groups: Vec<SavedMapEntry<u8, SaveFixtureGroup>>,
    pub animations: Vec<SavedMapEntry<u8, AnimationTemplate>>,
    pub views: Vec<SavedMapEntry<u8, View>>,
    pub scenes: Vec<SavedMapEntry<u8, SavedScene>>,
    pub current_scene_focus: u8,
    pub current_overlay_scenes: Vec<u8>,
    pub overrides: Vec<SavedMapEntry<(usize, usize), u8>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct ShowfileArtNetReceiver {
    pub address: SocketAddr,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode, Default)]
pub struct ShowfileArtNetState {
    pub receivers: Vec<ShowfileArtNetReceiver>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode, Default)]
pub struct Showfile {
    pub engine: SaveEngineState,
    #[serde(default)]
    pub artnet: ShowfileArtNetState,
    #[serde(default)]
    pub plugin_state: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct SavedActiveAnimation {
    pub speed_factor: AnimationSpeedModifier,
    pub enabled: bool,
    pub fixture_timers: Vec<SavedMapEntry<(u8, u8), AnimationTimerState>>,
    pub spec: AnimationSpec,
}

impl From<ActiveAnimation> for SavedActiveAnimation {
    fn from(value: ActiveAnimation) -> Self {
        Self {
            speed_factor: value.speed_factor,
            enabled: value.enabled,
            fixture_timers: SavedMapEntry::from_btree_map(value.fixture_timers),
            spec: value.spec_cloned,
        }
    }
}

impl From<SavedActiveAnimation> for ActiveAnimation {
    fn from(value: SavedActiveAnimation) -> Self {
        Self {
            speed_factor: value.speed_factor,
            enabled: value.enabled,
            fixture_timers: SavedMapEntry::to_btree_map(value.fixture_timers),
            spec_cloned: value.spec,
        }
    }
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
            .map(|(key, animation_map)| {
                let saved_map = animation_map
                    .into_iter()
                    .map(|(animation_id, animation)| SavedMapEntry {
                        key: animation_id,
                        value: SavedActiveAnimation::from(animation),
                    })
                    .collect();

                SavedMapEntry {
                    key,
                    value: saved_map,
                }
            })
            .collect();

        Self {
            sink: SavedEngineSink {
                fixture_states: SavedMapEntry::from_btree_map(value.sink.fixture_states),
                active_animations,
                changeset: value.sink.changeset.into_iter().collect(),
                master_alpha_fader: value.sink.master_alpha_fader,
                master_alpha_speed: value.sink.master_speed,
            },
            name: value.name,
        }
    }
}

impl TryFrom<SavedScene> for Scene {
    type Error = String;

    fn try_from(value: SavedScene) -> Result<Self, Self::Error> {
        let sink = EngineSink::try_from(value.sink)?;

        Ok(Self {
            sink,
            name: value.name,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct SavedEngineSink {
    pub fixture_states: Vec<SavedMapEntry<(u8, u8), FixtureState>>,
    pub active_animations:
        Vec<SavedMapEntry<FixtureSelection, Vec<SavedMapEntry<u8, SavedActiveAnimation>>>>,
    pub changeset: Vec<FixtureSelector>,
    pub master_alpha_fader: u8,
    pub master_alpha_speed: AnimationSpeedModifier,
}

impl TryFrom<SavedEngineSink> for EngineSink {
    type Error = String;

    fn try_from(value: SavedEngineSink) -> Result<Self, Self::Error> {
        let fixture_states = SavedMapEntry::to_btree_map(value.fixture_states);
        let active_animations = value
            .active_animations
            .into_iter()
            .map(|SavedMapEntry { key, value }| {
                let animations = value
                    .into_iter()
                    .map(|SavedMapEntry { key, value }| (key, ActiveAnimation::from(value)))
                    .collect::<BTreeMap<_, _>>();
                (key, animations)
            })
            .collect::<HashMap<_, _>>();
        let changeset = value.changeset.into_iter().collect();

        Ok(Self {
            fixture_states,
            active_animations,
            changeset,
            master_alpha_fader: value.master_alpha_fader,
            master_speed: value.master_alpha_speed,
        })
    }
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

        let animations = SavedMapEntry::from_btree_map(value.animation_templates);

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
        }
    }
}

/// From `SaveEngineState` to `EngineState`
impl TryFrom<SaveEngineState> for EngineState {
    type Error = String;

    fn try_from(value: SaveEngineState) -> Result<Self, Self::Error> {
        let groups = value
            .groups
            .into_iter()
            .map(|SavedMapEntry { key, value }| {
                let fixtures = SavedMapEntry::to_btree_map(value.fixtures);

                (
                    key,
                    FixtureGroup {
                        name: value.name,
                        fixtures,
                    },
                )
            })
            .collect::<BTreeMap<_, _>>();

        let animations = SavedMapEntry::to_btree_map(value.animations);
        let views = SavedMapEntry::to_btree_map(value.views);

        let scenes = value
            .scenes
            .into_iter()
            .map(|SavedMapEntry { key, value }| {
                let scene = Scene::try_from(value)?;
                Ok((key, scene))
            })
            .collect::<Result<BTreeMap<_, _>, Self::Error>>()?;

        let overrides = SavedMapEntry::to_btree_map(value.overrides);

        Ok(Self {
            groups,
            animation_templates: animations,
            selection: EngineSelection::default(),
            selection_stack: VecDeque::new(),
            control_buffer: FixtureState::default(),
            views,
            scenes,
            current_scene_focus: value.current_scene_focus,
            current_overlay_scenes: value.current_overlay_scenes,
            overrides,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ActiveAnimation, AnimationSpec, AnimationSpecBody, AnimationSpecBodyBeat,
        AnimationSpeedModifier, AnimationTemplate, AnimationTimerState, FixtureProperty,
        SaveEngineState, SyncMode,
        fixture::{
            FixtureType,
            dimmer::Dimmer,
            state::{Fixture, FixtureGroup, FixtureState},
        },
        scene::{EngineSink, FixtureSelection, FixtureSelector, Scene},
        view::View,
    };
    use std::collections::{BTreeMap, HashMap, HashSet};

    fn sample_engine_state() -> EngineState {
        let mut engine = EngineState::default();

        // Groups with fixtures.
        let mut fixtures = BTreeMap::new();
        fixtures.insert(
            1,
            Fixture {
                name: "Fixture 1".to_string(),
                type_: FixtureType::Dimmer(Dimmer::DimmerSingle),
                pos: Default::default(),
                start_addr: 1,
                universe_no: 0,
            },
        );
        engine.groups.insert(
            1,
            FixtureGroup {
                name: "Group 1".to_string(),
                fixtures,
            },
        );

        // Animation specs.
        engine.animation_templates.insert(
            1,
            AnimationTemplate {
                spec: AnimationSpec {
                    name: "Anim".to_string(),
                    body: AnimationSpecBody::AudioBeat(AnimationSpecBodyBeat {}),
                    property: FixtureProperty::Alpha,
                },
            },
        );

        // Views.
        engine.views.insert(
            1,
            View {
                name: "View 1".to_string(),
                base_scene: 1,
                overlays: vec![1],
            },
        );

        // Scenes.
        let mut fixture_states = BTreeMap::new();
        fixture_states.insert((1, 1), FixtureState::default());

        let mut timers = BTreeMap::new();
        timers.insert((1, 1), AnimationTimerState::default());

        let mut active_animation_map = BTreeMap::new();
        active_animation_map.insert(
            1,
            ActiveAnimation {
                speed_factor: AnimationSpeedModifier::_32,
                enabled: true,
                fixture_timers: timers,
                spec_cloned: AnimationSpec {
                    name: "Sample Animation".to_string(),
                    body: AnimationSpecBody::BeatClock(AnimationSpecBodyBeat {}),
                    property: FixtureProperty::Alpha,
                    // sync: SyncMode::StretchedHalfHalf,
                },
            },
        );

        let mut active_animations = HashMap::new();
        active_animations.insert(
            FixtureSelection {
                fixtures: vec![(1, 1)],
            },
            active_animation_map,
        );

        let mut changeset = HashSet::new();
        changeset.insert(FixtureSelector {
            gid: 1,
            fid: 1,
            property: FixtureProperty::Alpha,
        });

        engine.scenes.insert(
            1,
            Scene {
                sink: EngineSink {
                    fixture_states,
                    active_animations,
                    changeset,
                    master_alpha_fader: 100,
                    master_speed: AnimationSpeedModifier::_1,
                },
                name: "Scene 1".to_string(),
            },
        );

        engine.current_scene_focus = 1;
        engine.current_overlay_scenes = vec![1];
        engine.overrides.insert((0, 1), 42);
        engine
    }

    #[test]
    fn save_engine_state_serializes_to_json() {
        let engine = sample_engine_state();
        let save_state = SaveEngineState::from(engine.clone());

        let json = serde_json::to_string(&save_state).expect("JSON serialization should succeed");
        let decoded: SaveEngineState =
            serde_json::from_str(&json).expect("JSON deserialization should succeed");
        let restored =
            EngineState::try_from(decoded).expect("Conversion back to EngineState should succeed");

        assert_eq!(restored.groups.len(), engine.groups.len());
        assert_eq!(
            restored.animation_templates.len(),
            engine.animation_templates.len()
        );
        assert_eq!(restored.views.len(), engine.views.len());
        assert_eq!(restored.scenes.len(), engine.scenes.len());
        assert_eq!(restored.current_scene_focus, engine.current_scene_focus);
        assert_eq!(
            restored.current_overlay_scenes,
            engine.current_overlay_scenes
        );
        assert_eq!(restored.overrides, engine.overrides);
    }

    #[test]
    fn showfile_serialization_keeps_optional_sections() {
        let engine = sample_engine_state();
        let save_state = SaveEngineState::from(engine.clone());

        let mut plugin_state = HashMap::new();
        plugin_state.insert("plugin".to_string(), "state".to_string());

        let showfile = Showfile {
            engine: save_state,
            artnet: ShowfileArtNetState {
                receivers: vec![ShowfileArtNetReceiver {
                    address: "127.0.0.1:6454".parse().unwrap(),
                    enabled: true,
                }],
            },
            plugin_state,
        };

        let json =
            serde_json::to_string(&showfile).expect("Showfile JSON serialization should succeed");
        let decoded: Showfile =
            serde_json::from_str(&json).expect("Showfile JSON deserialization should succeed");

        assert_eq!(decoded.artnet.receivers.len(), 1);
        assert!(decoded.artnet.receivers[0].enabled);
        assert_eq!(
            decoded.artnet.receivers[0].address,
            "127.0.0.1:6454".parse().unwrap()
        );
        assert_eq!(
            decoded.plugin_state.get("plugin"),
            Some(&"state".to_string())
        );

        let restored_engine =
            EngineState::try_from(decoded.engine).expect("Engine conversion should succeed");
        assert_eq!(restored_engine.groups.len(), engine.groups.len());
    }
}
