use crate::{
    ActiveAnimation, AnimationSpeedModifier, AnimationTemplate, AnimationTimerState,
    EngineSelection,
    engine::{AnimationSpec, EngineState},
    fixture::state::{Fixture, FixtureGroup, FixtureState},
    palette::Palette,
    scene::{EngineSink, FixtureSelection, FixtureSelector, Scene},
    scene_graph::SceneGraphState,
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
    #[serde(default)]
    pub scene_graphs: SceneGraphState,
    #[serde(default)]
    pub palettes: Vec<SavedMapEntry<u8, Palette>>,
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
            iteration_count: 0,
            reversed: false,
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
                palette_assignments: SavedMapEntry::from_btree_map(value.sink.palette_assignments),
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
    #[serde(default)]
    pub palette_assignments: Vec<SavedMapEntry<(u8, u8), Vec<u8>>>,
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
            palette_assignments: SavedMapEntry::to_btree_map(value.palette_assignments),
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
            scene_graphs: value.scene_graphs,
            palettes: SavedMapEntry::from_btree_map(value.palettes),
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

        let mut state = Self {
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
            scene_graphs: value.scene_graphs,
            palettes: SavedMapEntry::to_btree_map(value.palettes),
        };

        // Legacy audio modes only ever exist on the wire; the in-memory model
        // carries them as compatibility `AudioModulation` layers so that saving
        // can never write the old variants back out.
        state.migrate_legacy_audio_animations();

        Ok(state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ActiveAnimation, AnimationSpec, AnimationSpecBody, AnimationSpecBodyBeat,
        AnimationSpeedModifier, AnimationTemplate, AnimationTimerState, AudioModulationBlend,
        AudioModulationSignal, FixtureProperty, PhaserDuration, SaveEngineState, SyncMode,
        fixture::{
            FixtureType,
            dimmer::Dimmer,
            state::{Fixture, FixtureGroup, FixtureState},
        },
        scene::{EngineSink, FixtureSelection, FixtureSelector, Scene},
        view::View,
    };
    use std::collections::{BTreeMap, HashMap, HashSet};
    use strum::IntoEnumIterator;

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
                rotation: Default::default(),
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
                iteration_count: 0,
                reversed: false,
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
                    palette_assignments: Default::default(),
                },
                name: "Scene 1".to_string(),
            },
        );

        engine.current_scene_focus = 1;
        engine.current_overlay_scenes = vec![1];
        engine.overrides.insert((0, 1), 42);
        engine
    }

    /// Every audio mode that existed before the modulation rework, in the
    /// shape a legacy showfile stores it.
    fn legacy_audio_bodies() -> Vec<AnimationSpecBody> {
        use crate::AnimationSpecBodyFrequencies;

        vec![
            AnimationSpecBody::AudioVolume(Default::default()),
            AnimationSpecBody::BPMValue(Default::default()),
            AnimationSpecBody::AudioBeat(AnimationSpecBodyBeat {}),
            AnimationSpecBody::BeatClock(AnimationSpecBodyBeat {}),
            AnimationSpecBody::AudioFrequencies(AnimationSpecBodyFrequencies {
                gate: 12,
                boost: 3,
                freq_min: 100,
                freq_max: 4_000,
                normalization: Default::default(),
            }),
        ]
    }

    fn engine_with_legacy_audio_animations() -> EngineState {
        let mut engine = sample_engine_state();
        engine.animation_templates.clear();

        let selection = FixtureSelection {
            fixtures: vec![(1, 1)],
        };
        let scene = engine.scenes.get_mut(&1).unwrap();
        scene.sink.active_animations.clear();
        let mut active = BTreeMap::new();

        for (index, body) in legacy_audio_bodies().into_iter().enumerate() {
            let id = index as u8;
            let spec = AnimationSpec {
                name: format!("Legacy {id}"),
                body,
                property: FixtureProperty::Alpha,
            };
            engine
                .animation_templates
                .insert(id, AnimationTemplate { spec: spec.clone() });

            let mut animation = ActiveAnimation::new(&selection.fixtures, spec);
            animation.enabled = true;
            active.insert(id, animation);
        }

        scene.sink.active_animations.insert(selection, active);
        engine
    }

    /// Serializes `engine` as a legacy showfile would have — i.e. with the old
    /// `AnimationSpecBody` variants still on the wire.
    fn legacy_showfile_json(engine: &EngineState) -> String {
        let save_state = SaveEngineState::from(engine.clone());
        let json = serde_json::to_string(&save_state).unwrap();
        for name in [
            "AudioVolume",
            "BPMValue",
            "AudioBeat",
            "BeatClock",
            "AudioFrequencies",
        ] {
            assert!(json.contains(name), "legacy fixture is missing {name}");
        }
        json
    }

    #[test]
    fn loading_a_legacy_showfile_migrates_every_audio_mode() {
        let engine = engine_with_legacy_audio_animations();
        let json = legacy_showfile_json(&engine);

        let decoded: SaveEngineState = serde_json::from_str(&json).unwrap();
        let restored = EngineState::try_from(decoded).unwrap();

        let expected_signals = [
            AudioModulationSignal::LegacyVolume,
            AudioModulationSignal::LegacyBpm,
            AudioModulationSignal::LegacyBass,
            AudioModulationSignal::LegacyBeatClock,
        ];

        for (id, expected) in expected_signals.into_iter().enumerate() {
            let AnimationSpecBody::AudioModulation(spec) =
                &restored.animation_templates[&(id as u8)].spec.body
            else {
                panic!("template {id} was not migrated");
            };
            assert_eq!(spec.signal, expected);
            assert_eq!(spec.blend, AudioModulationBlend::LegacyAbsolute);
            assert!(spec.is_compatibility());
        }

        // The spectrum mode carries its legacy fields through unchanged.
        let AnimationSpecBody::AudioModulation(spectrum) =
            &restored.animation_templates[&4].spec.body
        else {
            panic!("spectrum template was not migrated");
        };
        let AudioModulationSignal::LegacySpectrum(freqs) = &spectrum.signal else {
            panic!("expected a legacy spectrum signal");
        };
        assert_eq!(freqs.gate, 12);
        assert_eq!(freqs.boost, 3);
        assert_eq!(freqs.freq_min, 100);
        assert_eq!(freqs.freq_max, 4_000);

        // Specs cloned into active animations are migrated too.
        let animations = restored.scenes[&1]
            .sink
            .active_animations
            .values()
            .next()
            .unwrap();
        assert_eq!(animations.len(), 5);
        for animation in animations.values() {
            assert!(matches!(
                animation.spec_cloned.body,
                AnimationSpecBody::AudioModulation(_)
            ));
        }
    }

    #[test]
    fn saving_a_migrated_showfile_never_writes_the_old_variants() {
        let engine = engine_with_legacy_audio_animations();
        let legacy_json = legacy_showfile_json(&engine);

        let decoded: SaveEngineState = serde_json::from_str(&legacy_json).unwrap();
        let restored = EngineState::try_from(decoded).unwrap();
        let resaved = serde_json::to_string(&SaveEngineState::from(restored)).unwrap();

        // Externally tagged variants appear as `"Name":`, so this catches the
        // old bodies without tripping over the `Legacy*` signal names that
        // merely contain them as substrings.
        for name in [
            "AudioVolume",
            "BPMValue",
            "AudioBeat",
            "BeatClock",
            "AudioFrequencies",
        ] {
            assert!(
                !resaved.contains(&format!("\"{name}\":")),
                "re-saved showfile still contains the legacy variant {name}"
            );
        }
        assert!(resaved.contains("AudioModulation"));
        assert!(resaved.contains("LegacySpectrum"));

        // Reloading the migrated file is a no-op.
        let round_tripped: SaveEngineState = serde_json::from_str(&resaved).unwrap();
        let mut reloaded = EngineState::try_from(round_tripped).unwrap();
        assert_eq!(reloaded.migrate_legacy_audio_animations(), 0);
    }

    #[test]
    fn migration_leaves_phasers_and_new_layers_alone() {
        use crate::AnimationPreset;

        let mut engine = EngineState::default();
        for (id, preset) in [AnimationPreset::TempoSweep, AnimationPreset::KickFlash]
            .into_iter()
            .enumerate()
        {
            engine.animation_templates.insert(
                id as u8,
                AnimationTemplate {
                    spec: AnimationSpec::preset(preset),
                },
            );
        }
        let before: Vec<AnimationSpecBody> = engine
            .animation_templates
            .values()
            .map(|template| template.spec.body.clone())
            .collect();

        assert_eq!(engine.migrate_legacy_audio_animations(), 0);

        for (index, template) in engine.animation_templates.values().enumerate() {
            assert_eq!(
                template.spec.body.kind(),
                before[index].kind(),
                "template {index} changed"
            );
        }
    }

    #[test]
    fn every_preset_is_well_formed_and_editable() {
        use crate::{AnimationPreset, AudioModulationSignal};
        use std::collections::HashSet;

        let mut labels = HashSet::new();
        let mut descriptions = HashSet::new();

        for preset in AnimationPreset::iter() {
            let spec = AnimationSpec::preset(preset);

            assert!(!preset.label().is_empty(), "{preset:?} has no label");
            assert!(
                !preset.description().is_empty(),
                "{preset:?} has no description"
            );
            assert!(labels.insert(preset.label()), "duplicate label {preset:?}");
            assert!(
                descriptions.insert(preset.description()),
                "duplicate description {preset:?}"
            );

            assert_eq!(spec.name, preset.label(), "{preset:?} name/label mismatch");
            assert_eq!(
                spec.property,
                preset.property(),
                "{preset:?} drives a different property than it advertises"
            );

            match &spec.body {
                AnimationSpecBody::AudioModulation(body) => {
                    // Presets are authored layers, never compatibility ones.
                    assert!(
                        !body.is_compatibility(),
                        "{preset:?} uses a compatibility variant"
                    );

                    // Values must already be in range, so sanitizing is a no-op.
                    let mut sanitized = body.clone();
                    sanitized.shaping.sanitize();
                    sanitized.blend.sanitize();
                    assert_eq!(
                        &sanitized, body,
                        "{preset:?} carries values outside their valid range"
                    );

                    // A zero-gain layer would silently do nothing.
                    assert!(body.shaping.sensitivity > 0.0, "{preset:?} has no gain");
                    match body.blend {
                        AudioModulationBlend::Add { depth } => {
                            assert!(depth != 0.0, "{preset:?} adds nothing")
                        }
                        AudioModulationBlend::Scale { peak_percent } => {
                            assert!(peak_percent > 0.0, "{preset:?} scales to nothing")
                        }
                        AudioModulationBlend::LegacyAbsolute => {
                            panic!("{preset:?} must not use the legacy blend")
                        }
                    }

                    if let AudioModulationSignal::Band {
                        freq_min_hz,
                        freq_max_hz,
                    } = body.signal
                    {
                        assert!(freq_min_hz < freq_max_hz, "{preset:?} has an inverted band");
                        assert!(freq_max_hz <= 20_000, "{preset:?} exceeds the spectrum");
                    }

                    // At least one section must let the layer through.
                    assert!(
                        body.shaping.breakdown_multiplier > 0.0
                            || body.shaping.drop_multiplier > 0.0
                            || body.shaping.active_beat_multiplier > 0.0,
                        "{preset:?} is muted in every section"
                    );
                }
                AnimationSpecBody::Phaser(body) => {
                    // Pinning only makes sense on a beat-timed cycle.
                    assert_eq!(
                        body.pin_to_beat,
                        matches!(body.time_total, PhaserDuration::Beat(_)),
                        "{preset:?} pins a cycle it cannot follow"
                    );
                    let crate::PhaserKind::Mathematical(math) = &body.kind else {
                        panic!("{preset:?} must use the mathematical phaser");
                    };
                    let min = math.amplitude_min.resolve(&BTreeMap::new(), spec.property);
                    let max = math.amplitude_max.resolve(&BTreeMap::new(), spec.property);
                    assert!(min < max, "{preset:?} has a collapsed amplitude range");
                    let ceiling = if spec.property == FixtureProperty::ColorHue {
                        360
                    } else {
                        255
                    };
                    assert!(max <= ceiling, "{preset:?} exceeds its property range");
                }
                other => panic!("{preset:?} produced an unexpected body: {other:?}"),
            }
        }
    }

    #[test]
    fn preset_categories_partition_the_library() {
        use crate::{AnimationPreset, AnimationPresetCategory};

        let mut seen: Vec<AnimationPreset> = Vec::new();
        for category in AnimationPresetCategory::iter() {
            let presets: Vec<AnimationPreset> = category.presets().collect();
            assert!(!presets.is_empty(), "{category:?} is empty");
            for preset in &presets {
                assert_eq!(preset.category(), category);
            }
            seen.extend(presets);
        }

        assert_eq!(seen.len(), AnimationPreset::iter().count());
        for preset in AnimationPreset::iter() {
            assert_eq!(
                seen.iter().filter(|p| **p == preset).count(),
                1,
                "{preset:?} is not in exactly one category"
            );
        }
    }

    #[test]
    fn section_presets_are_actually_gated_by_section() {
        use crate::{AnimationPreset, SectionState};

        let shaping_of = |preset: AnimationPreset| {
            let AnimationSpecBody::AudioModulation(body) = AnimationSpec::preset(preset).body
            else {
                panic!("{preset:?} must be an audio modulation layer");
            };
            body.shaping
        };

        // Blinders stay dark through the breakdown and open up on the drop.
        let blinder = shaping_of(AnimationPreset::DropBlinder);
        assert_eq!(blinder.section_multiplier(SectionState::Breakdown), 0.0);
        assert!(
            blinder.section_multiplier(SectionState::Drop)
                > blinder.section_multiplier(SectionState::ActiveBeat)
        );

        // The glow is the mirror image, and inverts so quiet means bright.
        let glow = shaping_of(AnimationPreset::BreakdownGlow);
        assert!(glow.invert);
        assert_eq!(glow.section_multiplier(SectionState::Drop), 0.0);
        assert!(
            glow.section_multiplier(SectionState::Breakdown)
                > glow.section_multiplier(SectionState::ActiveBeat)
        );

        // The drop strobe exists only inside a drop.
        let strobe = shaping_of(AnimationPreset::DropStrobe);
        assert_eq!(strobe.section_multiplier(SectionState::Breakdown), 0.0);
        assert_eq!(strobe.section_multiplier(SectionState::ActiveBeat), 0.0);
        assert!(strobe.section_multiplier(SectionState::Drop) > 0.0);
    }

    #[test]
    fn percussive_presets_respond_faster_than_sustained_ones() {
        use crate::AnimationPreset;

        let release_of = |preset: AnimationPreset| {
            let AnimationSpecBody::AudioModulation(body) = AnimationSpec::preset(preset).body
            else {
                panic!("{preset:?} must be an audio modulation layer");
            };
            (body.shaping.attack_ms, body.shaping.release_ms)
        };

        for percussive in [
            AnimationPreset::KickFlash,
            AnimationPreset::StrobeStab,
            AnimationPreset::DropStrobe,
            AnimationPreset::HiHatShimmer,
        ] {
            let (attack, release) = release_of(percussive);
            assert_eq!(attack, 0, "{percussive:?} must hit instantly");
            assert!(release <= 260, "{percussive:?} lingers too long");
        }

        for sustained in [AnimationPreset::EnergyLift, AnimationPreset::BreakdownGlow] {
            let (attack, release) = release_of(sustained);
            assert!(attack >= 250, "{sustained:?} must ease in");
            assert!(release >= 600, "{sustained:?} must ease out");
        }
    }

    #[test]
    fn movement_presets_cover_distinct_phaser_behaviors() {
        use crate::{AnimationPreset, AnimationPresetCategory, PhaserKind};

        let mut bases = Vec::new();
        for preset in AnimationPresetCategory::Movement.presets() {
            let spec = AnimationSpec::preset(preset);
            let AnimationSpecBody::Phaser(body) = &spec.body else {
                panic!("{preset:?} must be a phaser");
            };
            let PhaserKind::Mathematical(math) = &body.kind else {
                panic!("{preset:?} must be a mathematical phaser");
            };
            bases.push((preset, math.base, spec.property));
        }
        assert!(bases.len() >= 4);

        // Movement presets should not all drive the same property.
        let properties: std::collections::HashSet<_> =
            bases.iter().map(|(_, _, property)| *property).collect();
        assert!(
            properties.len() >= 3,
            "movement presets barely differ: {properties:?}"
        );

        // The ping-pong is the one that flips direction.
        let ping_pong = AnimationSpec::preset(AnimationPreset::PingPongTilt);
        let AnimationSpecBody::Phaser(ping_pong_body) = &ping_pong.body else {
            unreachable!();
        };
        assert!(ping_pong_body.reverse_after_n_iterations.is_some());
        assert_eq!(ping_pong_body.sync, SyncMode::StretchedHalfHalf);

        // Hue Drift deliberately runs free instead of following the tempo.
        let drift = AnimationSpec::preset(AnimationPreset::HueDrift);
        let AnimationSpecBody::Phaser(drift_body) = &drift.body else {
            unreachable!();
        };
        assert!(matches!(drift_body.time_total, PhaserDuration::Fixed(_)));
        assert!(!drift_body.pin_to_beat);
    }

    #[test]
    fn starter_presets_match_their_specifications() {
        use crate::{
            AnimationPreset, AnimationSpeedModifier, AudioModulationSignal,
            MathematicalBaseFunction, PhaserDuration, PhaserKind,
        };

        let kick = AnimationSpec::preset(AnimationPreset::KickFlash);
        assert_eq!(kick.property, FixtureProperty::Alpha);
        let AnimationSpecBody::AudioModulation(kick_body) = &kick.body else {
            panic!("Kick Flash must be an audio modulation layer");
        };
        assert_eq!(kick_body.signal, AudioModulationSignal::BeatPulse);
        assert!(matches!(kick_body.blend, AudioModulationBlend::Add { .. }));
        assert_eq!(kick_body.shaping.attack_ms, 0, "attack must be immediate");
        assert!(kick_body.shaping.release_ms > 0 && kick_body.shaping.release_ms <= 200);

        let pump = AnimationSpec::preset(AnimationPreset::BassPump);
        assert_eq!(pump.property, FixtureProperty::Alpha);
        let AnimationSpecBody::AudioModulation(pump_body) = &pump.body else {
            panic!("Bass Pump must be an audio modulation layer");
        };
        assert_eq!(
            pump_body.signal,
            AudioModulationSignal::Band {
                freq_min_hz: 40,
                freq_max_hz: 180
            }
        );
        assert!(matches!(
            pump_body.blend,
            AudioModulationBlend::Scale { .. }
        ));

        let lift = AnimationSpec::preset(AnimationPreset::EnergyLift);
        assert_eq!(lift.property, FixtureProperty::ColorValue);
        let AnimationSpecBody::AudioModulation(lift_body) = &lift.body else {
            panic!("Energy Lift must be an audio modulation layer");
        };
        let AudioModulationSignal::Band {
            freq_min_hz,
            freq_max_hz,
        } = lift_body.signal
        else {
            panic!("Energy Lift must use a band signal");
        };
        assert!(freq_max_hz - freq_min_hz >= 10_000, "band must be broad");
        assert!(matches!(
            lift_body.blend,
            AudioModulationBlend::Scale { .. }
        ));
        // Slower smoothing than the percussive presets.
        assert!(lift_body.shaping.attack_ms > pump_body.shaping.attack_ms);
        assert!(lift_body.shaping.release_ms > pump_body.shaping.release_ms);

        let sweep = AnimationSpec::preset(AnimationPreset::TempoSweep);
        assert_eq!(sweep.property, FixtureProperty::Pan);
        let AnimationSpecBody::Phaser(sweep_body) = &sweep.body else {
            panic!("Tempo Sweep must stay a phaser");
        };
        assert!(sweep_body.pin_to_beat);
        assert_eq!(sweep_body.sync, SyncMode::StretchedEven);
        let PhaserDuration::Beat(beats) = sweep_body.time_total else {
            panic!("Tempo Sweep must be beat-timed");
        };
        assert_eq!(beats.as_float(), 4.0, "cycle must span four beats");
        assert_eq!(beats, AnimationSpeedModifier::_1_4);
        let PhaserKind::Mathematical(math) = &sweep_body.kind else {
            panic!("Tempo Sweep must use the mathematical phaser");
        };
        assert_eq!(math.base, MathematicalBaseFunction::Sin);

        // Presets are ordinary animations: no preset identity is retained.
        assert_eq!(kick.name, AnimationPreset::KickFlash.label());
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
