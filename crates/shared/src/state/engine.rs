use crate::{
    AnimationSpeedModifier, FixtureProperty, SectionState, SyncMode,
    fixture::{
        state::{FixtureGroup, FixtureState},
        value::FixtureValue,
    },
    palette::Palette,
    scene::{EngineSink, Scene},
    scene_graph::SceneGraphState,
    view::View,
};
use bincode::{Decode, Encode, config};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    fmt::Display,
};
use strum::{EnumIter, IntoEnumIterator};

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
        debug_assert!(self.fixtures_in_group.is_empty() || self.group_ids.len() <= 1);

        // A fixture filter without a group is stale state, not a selection.
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
    pub animation_templates: BTreeMap<u8, AnimationTemplate>,

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
    pub scene_graphs: SceneGraphState,

    #[serde(default)]
    pub palettes: BTreeMap<u8, Palette>,
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

        let valid_scene_ids: HashSet<u8> = self.scenes.keys().copied().collect();
        self.scene_graphs.retain_scene_ids(&valid_scene_ids);

        true
    }

    /// Verifies the palette binding invariant across every scene:
    /// for each fixture in a scene's `palette_assignments`, the property
    /// slots covered by those palettes must be `FixtureValue::PalettePointer`
    /// pointing at the "winning" palette (the last one in the list that
    /// covers the property, matching render-time apply order). Properties
    /// not covered by any assigned palette must not be PalettePointers.
    ///
    /// When `fix` is false, returns the first violation found, if any
    /// (intended for `debug_assert!`). When `fix` is true, any detected
    /// violations are logged as warnings and self-healed by stripping
    /// orphan palette assignments and re-syncing each scene's slots from
    /// its assignment list; the function then returns `Ok(())`.
    pub fn validate_palette_mapping_integrity(&mut self, fix: bool) -> Result<(), String> {
        let issues = self.collect_palette_integrity_issues();
        if issues.is_empty() {
            return Ok(());
        }

        if !fix {
            return Err(issues.into_iter().next().unwrap());
        }

        for issue in &issues {
            tracing::warn!("palette mapping integrity self-heal: {}", issue);
        }

        let palettes = self.palettes.clone();
        for scene in self.scenes.values_mut() {
            let valid: HashSet<(u8, u8)> = scene.sink.fixture_states.keys().copied().collect();
            scene
                .sink
                .palette_assignments
                .retain(|k, _| valid.contains(k));
            scene.sink.sync_palette_bindings(&palettes);
        }

        Ok(())
    }

    fn collect_palette_integrity_issues(&self) -> Vec<String> {
        let mut issues = Vec::new();

        for (scene_id, scene) in &self.scenes {
            for ((gid, fid), palette_ids) in &scene.sink.palette_assignments {
                let Some(fixture_state) = scene.sink.fixture_states.get(&(*gid, *fid)) else {
                    issues.push(format!(
                        "scene {scene_id}: palette assignment for missing fixture ({gid},{fid})"
                    ));
                    continue;
                };

                let mut winners: HashMap<FixtureProperty, u8> = HashMap::new();
                for palette_id in palette_ids {
                    let Some(palette) = self.palettes.get(palette_id) else {
                        continue;
                    };
                    for property in palette.kind.properties(&self.palettes) {
                        winners.insert(property, *palette_id);
                    }
                }

                for (property, winner_id) in &winners {
                    match fixture_state.slot(*property) {
                        FixtureValue::PalettePointer { palette_id, .. }
                            if palette_id == *winner_id => {}
                        other => {
                            issues.push(format!(
                                "scene {scene_id} fixture ({gid},{fid}) {property:?}: expected PalettePointer({winner_id}), got {other:?}"
                            ));
                        }
                    }
                }
            }

            // Also flag stale PalettePointers — slots bound to a palette that
            // is no longer in the assignment list (or the palette no longer
            // exists). These would render incorrectly and indicate a sync bug.
            for ((gid, fid), fixture_state) in &scene.sink.fixture_states {
                let assigned: Vec<u8> = scene
                    .sink
                    .palette_assignments
                    .get(&(*gid, *fid))
                    .cloned()
                    .unwrap_or_default();
                let mut covered: HashMap<FixtureProperty, u8> = HashMap::new();
                for palette_id in &assigned {
                    let Some(palette) = self.palettes.get(palette_id) else {
                        continue;
                    };
                    for property in palette.kind.properties(&self.palettes) {
                        covered.insert(property, *palette_id);
                    }
                }
                for property in FixtureProperty::iter() {
                    if let FixtureValue::PalettePointer { palette_id, .. } =
                        fixture_state.slot(property)
                    {
                        match covered.get(&property) {
                            Some(winner) if *winner == palette_id => {}
                            _ => {
                                issues.push(format!(
                                    "scene {scene_id} fixture ({gid},{fid}) {property:?}: stale PalettePointer({palette_id}); assignment list = {assigned:?}"
                                ));
                            }
                        }
                    }
                }
            }
        }

        issues
    }

    /// Rewrites every legacy audio animation — reusable templates as well as
    /// the specs cloned into active animations — into the unified
    /// [`AudioModulationSpec`] model. Returns how many specs were rewritten.
    ///
    /// Runs on load so that a subsequent save can only ever emit the new
    /// representation.
    pub fn migrate_legacy_audio_animations(&mut self) -> usize {
        let mut migrated = 0;

        for template in self.animation_templates.values_mut() {
            if template.spec.body.migrate_legacy_audio() {
                migrated += 1;
            }
        }

        for scene in self.scenes.values_mut() {
            for animations in scene.sink.active_animations.values_mut() {
                for animation in animations.values_mut() {
                    if animation.spec_cloned.body.migrate_legacy_audio() {
                        migrated += 1;
                    }
                }
            }
        }

        migrated
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
pub struct AnimationTemplate {
    pub spec: AnimationSpec,
}

// Describes how an animation runs.
#[derive(Debug, Serialize, Deserialize, Clone, Encode, Decode)]
pub struct AnimationSpec {
    pub name: String,
    pub body: AnimationSpecBody,
    pub property: FixtureProperty,
    // pub sync: SyncMode,
}

impl AnimationSpec {
    pub fn empty() -> Self {
        Self {
            name: "EMPTY".to_string(),
            body: AnimationSpecBody::AudioModulation(AudioModulationSpec::default()),
            property: FixtureProperty::Alpha,
            // sync: SyncMode::Synced,
        }
    }

    pub fn sync_mode(&self) -> SyncMode {
        match self.body {
            AnimationSpecBody::Phaser(ref animation_spec_body_phaser) => {
                animation_spec_body_phaser.sync
            }
            AnimationSpecBody::AudioVolume(_)
            | AnimationSpecBody::BPMValue(_)
            | AnimationSpecBody::AudioBeat(_)
            | AnimationSpecBody::AudioFrequencies(_)
            | AnimationSpecBody::BeatClock(_)
            | AnimationSpecBody::AudioModulation(_)
            | AnimationSpecBody::Wasm(_) => SyncMode::Synced,
        }
    }
}

impl AnimationSpec {
    pub fn is_beat_pinned(&self) -> bool {
        match &self.body {
            AnimationSpecBody::Phaser(animation_spec_body_phaser) => {
                animation_spec_body_phaser.pin_to_beat
            }
            AnimationSpecBody::AudioVolume(_)
            | AnimationSpecBody::BPMValue(_)
            | AnimationSpecBody::AudioBeat(_)
            | AnimationSpecBody::AudioFrequencies(_)
            | AnimationSpecBody::BeatClock(_)
            | AnimationSpecBody::AudioModulation(_)
            | AnimationSpecBody::Wasm(_) => false,
        }
    }

    /// Builds one of the starter presets. Presets are ordinary single-layer
    /// animations: every value stays editable and no preset identity is kept.
    pub fn preset(kind: AnimationPreset) -> Self {
        let body = match kind {
            //
            // Beat — driven by the hybrid onset/clock pulse.
            //

            // Immediate, percussive alpha stab on every kick.
            AnimationPreset::KickFlash => {
                pulse_layer(0, 120, AudioModulationBlend::Add { depth: 255.0 })
            }
            // Very short shutter burst; the tight release keeps it a stab, not a flash.
            AnimationPreset::StrobeStab => {
                pulse_layer(0, 45, AudioModulationBlend::Add { depth: 200.0 })
            }
            // Slower decay so beams visibly breathe rather than blink.
            AnimationPreset::BeamPunch => {
                pulse_layer(0, 220, AudioModulationBlend::Add { depth: 180.0 })
            }
            // Nudges hue on each beat and lets it fall back to the authored color.
            AnimationPreset::ColorFlicker => {
                pulse_layer(0, 260, AudioModulationBlend::Add { depth: 120.0 })
            }

            //
            // Frequency — continuous band energy.
            //
            AnimationPreset::BassPump => band_layer(
                40,
                180,
                AudioModulationShaping {
                    attack_ms: 5,
                    release_ms: 90,
                    ..AudioModulationShaping::default()
                },
                AudioModulationBlend::Scale {
                    peak_percent: 100.0,
                },
            ),
            // Sub-only: the threshold keeps mid-bass out of it.
            AnimationPreset::SubRumble => band_layer(
                20,
                60,
                AudioModulationShaping {
                    threshold: 0.2,
                    attack_ms: 20,
                    release_ms: 300,
                    ..AudioModulationShaping::default()
                },
                AudioModulationBlend::Scale {
                    peak_percent: 100.0,
                },
            ),
            // Hats and cymbals only, gated hard and gained back up so the
            // sparkle reads even though the band carries little energy.
            AnimationPreset::HiHatShimmer => band_layer(
                6_000,
                16_000,
                AudioModulationShaping {
                    threshold: 0.35,
                    sensitivity: 2.5,
                    attack_ms: 0,
                    release_ms: 70,
                    ..AudioModulationShaping::default()
                },
                AudioModulationBlend::Add { depth: 120.0 },
            ),
            // Vocal / lead range, lifting color rather than brightness.
            AnimationPreset::MidBloom => band_layer(
                300,
                2_500,
                AudioModulationShaping {
                    threshold: 0.15,
                    attack_ms: 40,
                    release_ms: 220,
                    ..AudioModulationShaping::default()
                },
                AudioModulationBlend::Add { depth: 140.0 },
            ),
            AnimationPreset::EnergyLift => band_layer(
                20,
                20_000,
                AudioModulationShaping {
                    attack_ms: 250,
                    release_ms: 600,
                    ..AudioModulationShaping::default()
                },
                AudioModulationBlend::Scale {
                    peak_percent: 100.0,
                },
            ),

            //
            // Section — the same signals, gated by the macro section.
            //

            // Silent through breakdowns, full on the drop, held back once the
            // track settles into its groove.
            AnimationPreset::DropBlinder => AudioModulationSpec {
                signal: AudioModulationSignal::Energy,
                shaping: AudioModulationShaping {
                    threshold: 0.25,
                    attack_ms: 0,
                    release_ms: 150,
                    breakdown_multiplier: 0.0,
                    drop_multiplier: 1.0,
                    active_beat_multiplier: 0.3,
                    ..AudioModulationShaping::default()
                },
                blend: AudioModulationBlend::Add { depth: 255.0 },
            },
            // Inverted, so the quieter the track the more it glows — and only
            // in the breakdown, where inversion is actually wanted.
            AnimationPreset::BreakdownGlow => AudioModulationSpec {
                signal: AudioModulationSignal::Energy,
                shaping: AudioModulationShaping {
                    invert: true,
                    attack_ms: 400,
                    release_ms: 900,
                    breakdown_multiplier: 1.0,
                    drop_multiplier: 0.0,
                    active_beat_multiplier: 0.2,
                    ..AudioModulationShaping::default()
                },
                blend: AudioModulationBlend::Add { depth: 160.0 },
            },
            // Beat-locked shutter that exists only for the duration of a drop.
            AnimationPreset::DropStrobe => AudioModulationSpec {
                signal: AudioModulationSignal::BeatPulse,
                shaping: AudioModulationShaping {
                    attack_ms: 0,
                    release_ms: 60,
                    breakdown_multiplier: 0.0,
                    drop_multiplier: 1.0,
                    active_beat_multiplier: 0.0,
                    ..AudioModulationShaping::default()
                },
                blend: AudioModulationBlend::Add { depth: 255.0 },
            },

            //
            // Movement — tempo-locked phasers, not sound-reactive layers.
            //
            AnimationPreset::TempoSweep => {
                return phaser_preset(
                    kind,
                    FixtureProperty::Pan,
                    MathematicalBaseFunction::Sin,
                    (0, 255),
                    // `_1_4` is the four-beat cycle (the modifier names are divisors).
                    PhaserDuration::Beat(AnimationSpeedModifier::_1_4),
                    SyncMode::StretchedEven,
                    None,
                );
            }
            // Half/half phase split plus a periodic reversal reads as a
            // ping-pong across the rig instead of a uniform nod.
            AnimationPreset::PingPongTilt => {
                return phaser_preset(
                    kind,
                    FixtureProperty::Tilt,
                    MathematicalBaseFunction::Triangle,
                    // Kept off the mechanical end stops.
                    (64, 192),
                    PhaserDuration::Beat(AnimationSpeedModifier::_1_2),
                    SyncMode::StretchedHalfHalf,
                    Some(4),
                );
            }
            // One-beat cycle lit for an eighth of it: a classic hard chase.
            AnimationPreset::EighthChase => {
                return phaser_preset(
                    kind,
                    FixtureProperty::Alpha,
                    MathematicalBaseFunction::Square1_8,
                    (0, 255),
                    PhaserDuration::Beat(AnimationSpeedModifier::_1),
                    SyncMode::StretchedEven,
                    None,
                );
            }
            // Free-running rainbow, deliberately not beat-locked.
            AnimationPreset::HueDrift => {
                return phaser_preset(
                    kind,
                    FixtureProperty::ColorHue,
                    MathematicalBaseFunction::Sin,
                    (0, 360),
                    PhaserDuration::Fixed(30_000),
                    SyncMode::StretchedEven,
                    None,
                );
            }
        };

        Self {
            name: kind.label().to_string(),
            property: kind.property(),
            body: AnimationSpecBody::AudioModulation(body),
        }
    }
}

fn pulse_layer(
    attack_ms: u32,
    release_ms: u32,
    blend: AudioModulationBlend,
) -> AudioModulationSpec {
    AudioModulationSpec {
        signal: AudioModulationSignal::BeatPulse,
        shaping: AudioModulationShaping {
            attack_ms,
            release_ms,
            ..AudioModulationShaping::default()
        },
        blend,
    }
}

fn band_layer(
    freq_min_hz: u32,
    freq_max_hz: u32,
    shaping: AudioModulationShaping,
    blend: AudioModulationBlend,
) -> AudioModulationSpec {
    AudioModulationSpec {
        signal: AudioModulationSignal::Band {
            freq_min_hz,
            freq_max_hz,
        },
        shaping,
        blend,
    }
}

fn phaser_preset(
    kind: AnimationPreset,
    property: FixtureProperty,
    base: MathematicalBaseFunction,
    (amplitude_min, amplitude_max): (u16, u16),
    time_total: PhaserDuration,
    sync: SyncMode,
    reverse_after_n_iterations: Option<u32>,
) -> AnimationSpec {
    AnimationSpec {
        name: kind.label().to_string(),
        property,
        body: AnimationSpecBody::Phaser(AnimationSpecBodyPhaser {
            kind: PhaserKind::Mathematical(MathematicalPhaser {
                base,
                stretch_factor: 1.0,
                amplitude_min: FixtureValue::Literal(amplitude_min),
                amplitude_max: FixtureValue::Literal(amplitude_max),
            }),
            time_total,
            // Fixed-duration phasers cannot pin to a beat grid they don't follow.
            pin_to_beat: matches!(time_total, PhaserDuration::Beat(_)),
            sync,
            reverse_after_n_iterations,
        }),
    }
}

/// Starter presets offered in the new-animation flow.
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash, EnumIter)]
pub enum AnimationPreset {
    // Beat.
    KickFlash,
    StrobeStab,
    BeamPunch,
    ColorFlicker,
    // Frequency.
    BassPump,
    SubRumble,
    HiHatShimmer,
    MidBloom,
    EnergyLift,
    // Section.
    DropBlinder,
    BreakdownGlow,
    DropStrobe,
    // Movement.
    TempoSweep,
    PingPongTilt,
    EighthChase,
    HueDrift,
}

impl AnimationPreset {
    pub fn label(&self) -> &'static str {
        match self {
            Self::KickFlash => "Kick Flash",
            Self::StrobeStab => "Strobe Stab",
            Self::BeamPunch => "Beam Punch",
            Self::ColorFlicker => "Color Flicker",
            Self::BassPump => "Bass Pump",
            Self::SubRumble => "Sub Rumble",
            Self::HiHatShimmer => "Hi-Hat Shimmer",
            Self::MidBloom => "Mid Bloom",
            Self::EnergyLift => "Energy Lift",
            Self::DropBlinder => "Drop Blinder",
            Self::BreakdownGlow => "Breakdown Glow",
            Self::DropStrobe => "Drop Strobe",
            Self::TempoSweep => "Tempo Sweep",
            Self::PingPongTilt => "Ping-Pong Tilt",
            Self::EighthChase => "Eighth Chase",
            Self::HueDrift => "Hue Drift",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            Self::KickFlash => "Beat pulse -> additive alpha, instant attack",
            Self::StrobeStab => "Beat pulse -> shutter burst, very short release",
            Self::BeamPunch => "Beat pulse -> focus, slower decay",
            Self::ColorFlicker => "Beat pulse -> hue nudge that settles back",
            Self::BassPump => "40-180 Hz energy -> scaled alpha",
            Self::SubRumble => "20-60 Hz energy -> scaled color value",
            Self::HiHatShimmer => "6-16 kHz energy -> additive alpha sparkle",
            Self::MidBloom => "300 Hz-2.5 kHz energy -> additive color value",
            Self::EnergyLift => "Broad-band energy -> scaled color value, slow",
            Self::DropBlinder => "Energy -> additive alpha, drop only",
            Self::BreakdownGlow => "Inverted energy -> additive alpha, breakdown only",
            Self::DropStrobe => "Beat pulse -> shutter, drop only",
            Self::TempoSweep => "Sine phaser -> pan, 4-beat pinned cycle",
            Self::PingPongTilt => "Triangle phaser -> tilt, 2 beats, reverses",
            Self::EighthChase => "1/8 square phaser -> alpha, 1-beat chase",
            Self::HueDrift => "Sine phaser -> hue, free-running 30 s",
        }
    }

    /// The property the preset is designed to drive.
    pub fn property(&self) -> FixtureProperty {
        match self {
            Self::KickFlash
            | Self::BassPump
            | Self::DropBlinder
            | Self::BreakdownGlow
            | Self::HiHatShimmer
            | Self::EighthChase => FixtureProperty::Alpha,
            Self::StrobeStab | Self::DropStrobe => FixtureProperty::Strobe,
            Self::BeamPunch => FixtureProperty::Focus,
            Self::ColorFlicker | Self::HueDrift => FixtureProperty::ColorHue,
            Self::SubRumble | Self::MidBloom | Self::EnergyLift => FixtureProperty::ColorValue,
            Self::TempoSweep => FixtureProperty::Pan,
            Self::PingPongTilt => FixtureProperty::Tilt,
        }
    }

    pub fn category(&self) -> AnimationPresetCategory {
        match self {
            Self::KickFlash | Self::StrobeStab | Self::BeamPunch | Self::ColorFlicker => {
                AnimationPresetCategory::Beat
            }
            Self::BassPump
            | Self::SubRumble
            | Self::HiHatShimmer
            | Self::MidBloom
            | Self::EnergyLift => AnimationPresetCategory::Frequency,
            Self::DropBlinder | Self::BreakdownGlow | Self::DropStrobe => {
                AnimationPresetCategory::Section
            }
            Self::TempoSweep | Self::PingPongTilt | Self::EighthChase | Self::HueDrift => {
                AnimationPresetCategory::Movement
            }
        }
    }
}

impl Display for AnimationPreset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.label())
    }
}

/// Groups the preset list so the picker stays readable as it grows.
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash, EnumIter)]
pub enum AnimationPresetCategory {
    Beat,
    Frequency,
    Section,
    Movement,
}

impl AnimationPresetCategory {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Beat => "Beat",
            Self::Frequency => "Frequency",
            Self::Section => "Section",
            Self::Movement => "Movement",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            Self::Beat => "React to the hybrid onset / beat-clock pulse",
            Self::Frequency => "React to the energy in a frequency band",
            Self::Section => "Gated by the macro section (breakdown / drop / groove)",
            Self::Movement => "Tempo-locked phasers, not sound-reactive",
        }
    }

    pub fn presets(&self) -> impl Iterator<Item = AnimationPreset> + '_ {
        AnimationPreset::iter().filter(move |preset| preset.category() == *self)
    }
}

impl Display for AnimationPresetCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.label())
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Encode, Decode, EnumIter)]
pub enum AnimationSpecBody {
    /// Phaser operates on a degree (0-360 DEG) an the amount is increased in time steps.
    Phaser(AnimationSpecBodyPhaser),
    AudioVolume(AnimationSpecBodyAudioVolume),
    BPMValue(AnimationSpecBodyBpmValue),
    AudioBeat(AnimationSpecBodyBeat),
    AudioFrequencies(AnimationSpecBodyFrequencies),
    BeatClock(AnimationSpecBodyBeat),
    Wasm(AnimationSpecBodyWasm), // TODO: not currently supported.
    /// Unified sound-reactive layer. New serialized variants are appended so
    /// existing showfiles keep their variant indices.
    AudioModulation(AudioModulationSpec),
}

impl From<AnimationSpecBodyKind> for AnimationSpecBody {
    fn from(value: AnimationSpecBodyKind) -> Self {
        match value {
            AnimationSpecBodyKind::Phaser => Self::Phaser(AnimationSpecBodyPhaser::default()),
            AnimationSpecBodyKind::AudioVolume => {
                Self::AudioVolume(AnimationSpecBodyAudioVolume::default())
            }
            AnimationSpecBodyKind::BPMValue => Self::BPMValue(AnimationSpecBodyBpmValue::default()),
            AnimationSpecBodyKind::AudioBeat => Self::AudioBeat(AnimationSpecBodyBeat::default()),
            AnimationSpecBodyKind::AudioFrequencies => {
                Self::AudioFrequencies(AnimationSpecBodyFrequencies::default())
            }
            AnimationSpecBodyKind::BeatClock => Self::BeatClock(AnimationSpecBodyBeat::default()),
            AnimationSpecBodyKind::Wasm => Self::Wasm(AnimationSpecBodyWasm::default()),
            AnimationSpecBodyKind::AudioModulation => {
                Self::AudioModulation(AudioModulationSpec::default())
            }
        }
    }
}

impl AnimationSpecBody {
    pub fn kind(&self) -> AnimationSpecBodyKind {
        match self {
            AnimationSpecBody::Phaser(_) => AnimationSpecBodyKind::Phaser,
            AnimationSpecBody::AudioVolume(_) => AnimationSpecBodyKind::AudioVolume,
            AnimationSpecBody::BPMValue(_) => AnimationSpecBodyKind::BPMValue,
            AnimationSpecBody::AudioBeat(_) => AnimationSpecBodyKind::AudioBeat,
            AnimationSpecBody::AudioFrequencies(_) => AnimationSpecBodyKind::AudioFrequencies,
            AnimationSpecBody::BeatClock(_) => AnimationSpecBodyKind::BeatClock,
            AnimationSpecBody::Wasm(_) => AnimationSpecBodyKind::Wasm,
            AnimationSpecBody::AudioModulation(_) => AnimationSpecBodyKind::AudioModulation,
        }
    }

    /// Rewrites a legacy audio body into the unified [`AudioModulationSpec`]
    /// model, using hidden compatibility signal/output variants that reproduce
    /// the former absolute values exactly. Returns `true` when a rewrite
    /// happened.
    pub fn migrate_legacy_audio(&mut self) -> bool {
        let signal = match self {
            AnimationSpecBody::AudioVolume(_) => AudioModulationSignal::LegacyVolume,
            AnimationSpecBody::BPMValue(_) => AudioModulationSignal::LegacyBpm,
            AnimationSpecBody::AudioBeat(_) => AudioModulationSignal::LegacyBass,
            AnimationSpecBody::BeatClock(_) => AudioModulationSignal::LegacyBeatClock,
            AnimationSpecBody::AudioFrequencies(frequencies) => {
                AudioModulationSignal::LegacySpectrum(frequencies.clone())
            }
            AnimationSpecBody::Phaser(_)
            | AnimationSpecBody::Wasm(_)
            | AnimationSpecBody::AudioModulation(_) => return false,
        };

        *self = AnimationSpecBody::AudioModulation(AudioModulationSpec {
            signal,
            shaping: AudioModulationShaping::default(),
            blend: AudioModulationBlend::LegacyAbsolute,
        });
        true
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, Encode, Decode, EnumIter, PartialEq, Eq)]
pub enum AnimationSpecBodyKind {
    Phaser,
    AudioVolume,
    BPMValue,
    AudioBeat,
    AudioFrequencies,
    BeatClock,
    Wasm,
    AudioModulation,
}

impl AnimationSpecBodyKind {
    /// Kinds offered when authoring a new animation. The legacy audio kinds
    /// are migration targets only and never appear here.
    pub fn selectable() -> impl Iterator<Item = Self> {
        [Self::Phaser, Self::AudioModulation, Self::Wasm].into_iter()
    }
}

/// A sound-reactive layer: a signal, the shaping applied to it, and how the
/// shaped envelope blends into the authored scene value.
#[derive(Debug, Serialize, Deserialize, Clone, Encode, Decode, Default, PartialEq)]
pub struct AudioModulationSpec {
    pub signal: AudioModulationSignal,
    pub shaping: AudioModulationShaping,
    pub blend: AudioModulationBlend,
}

impl AudioModulationSpec {
    /// True when this layer only exists to replay a legacy animation mode.
    /// Such layers are editable through their applicable legacy fields only.
    pub fn is_compatibility(&self) -> bool {
        self.signal.is_compatibility() || self.blend.is_compatibility()
    }
}

/// What the layer listens to.
#[derive(Debug, Serialize, Deserialize, Clone, Encode, Decode, PartialEq)]
pub enum AudioModulationSignal {
    /// Overall energy of the mix, normalized to `0..1`.
    Energy,
    /// RMS energy of the FFT buckets overlapping `[freq_min_hz, freq_max_hz]`.
    /// One uniform value for the whole selection.
    Band { freq_min_hz: u32, freq_max_hz: u32 },
    /// Hybrid pulse: fires immediately on a new onset and falls back to the
    /// beat clock only when an expected beat had no onset.
    BeatPulse,
    /// Compatibility: former `AudioVolume` mode.
    LegacyVolume,
    /// Compatibility: former `BPMValue` mode.
    LegacyBpm,
    /// Compatibility: former `AudioBeat` mode.
    LegacyBass,
    /// Compatibility: former `BeatClock` mode.
    LegacyBeatClock,
    /// Compatibility: former `AudioFrequencies` mode, including its spatial
    /// spread of spectrum buckets across the selection.
    LegacySpectrum(AnimationSpecBodyFrequencies),
}

impl Default for AudioModulationSignal {
    fn default() -> Self {
        Self::Energy
    }
}

impl AudioModulationSignal {
    pub fn is_compatibility(&self) -> bool {
        matches!(
            self,
            Self::LegacyVolume
                | Self::LegacyBpm
                | Self::LegacyBass
                | Self::LegacyBeatClock
                | Self::LegacySpectrum(_)
        )
    }

    /// Signal sources offered when authoring a new layer.
    pub fn selectable() -> impl Iterator<Item = Self> {
        [
            Self::Energy,
            Self::Band {
                freq_min_hz: 40,
                freq_max_hz: 180,
            },
            Self::BeatPulse,
        ]
        .into_iter()
    }

    /// Stable identity used to compare a spec against the picker entries.
    pub fn discriminant_label(&self) -> &'static str {
        match self {
            Self::Energy => "Energy",
            Self::Band { .. } => "Band",
            Self::BeatPulse => "Beat Pulse",
            Self::LegacyVolume => "Legacy Volume",
            Self::LegacyBpm => "Legacy BPM",
            Self::LegacyBass => "Legacy Bass",
            Self::LegacyBeatClock => "Legacy Beat Clock",
            Self::LegacySpectrum(_) => "Legacy Spectrum",
        }
    }
}

impl Display for AudioModulationSignal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Band {
                freq_min_hz,
                freq_max_hz,
            } => write!(f, "Band {freq_min_hz}-{freq_max_hz} Hz"),
            other => write!(f, "{}", other.discriminant_label()),
        }
    }
}

/// How the raw `0..1` signal is turned into the envelope that drives output.
#[derive(Debug, Serialize, Deserialize, Clone, Copy, Encode, Decode, PartialEq)]
pub struct AudioModulationShaping {
    /// Input below this (`0..1`) produces no output.
    pub threshold: f32,
    /// Gain applied after the threshold, before clamping back to `0..1`.
    pub sensitivity: f32,
    /// Smoothing time constant while the envelope rises. `0` snaps instantly.
    pub attack_ms: u32,
    /// Smoothing time constant while the envelope falls. `0` snaps instantly.
    pub release_ms: u32,
    /// Mirrors the shaped value. Only applied while the audio input is valid.
    pub invert: bool,
    pub breakdown_multiplier: f32,
    pub drop_multiplier: f32,
    pub active_beat_multiplier: f32,
}

impl Default for AudioModulationShaping {
    fn default() -> Self {
        Self {
            threshold: 0.0,
            sensitivity: 1.0,
            attack_ms: 10,
            release_ms: 200,
            invert: false,
            breakdown_multiplier: 1.0,
            drop_multiplier: 1.0,
            active_beat_multiplier: 1.0,
        }
    }
}

impl AudioModulationShaping {
    pub fn section_multiplier(&self, section: SectionState) -> f32 {
        let raw = match section {
            SectionState::Breakdown => self.breakdown_multiplier,
            SectionState::Drop => self.drop_multiplier,
            SectionState::ActiveBeat => self.active_beat_multiplier,
        };
        if raw.is_finite() {
            raw.clamp(0.0, MAX_SECTION_MULTIPLIER)
        } else {
            1.0
        }
    }

    pub fn sanitize(&mut self) {
        self.threshold = finite_or(self.threshold, 0.0).clamp(0.0, MAX_THRESHOLD);
        self.sensitivity = finite_or(self.sensitivity, 1.0).clamp(0.0, MAX_SENSITIVITY);
        self.attack_ms = self.attack_ms.min(MAX_SMOOTHING_MS);
        self.release_ms = self.release_ms.min(MAX_SMOOTHING_MS);
        self.breakdown_multiplier =
            finite_or(self.breakdown_multiplier, 1.0).clamp(0.0, MAX_SECTION_MULTIPLIER);
        self.drop_multiplier =
            finite_or(self.drop_multiplier, 1.0).clamp(0.0, MAX_SECTION_MULTIPLIER);
        self.active_beat_multiplier =
            finite_or(self.active_beat_multiplier, 1.0).clamp(0.0, MAX_SECTION_MULTIPLIER);
    }
}

/// The threshold never reaches 1.0 — that would leave no usable input range.
pub const MAX_THRESHOLD: f32 = 0.99;
pub const MAX_SENSITIVITY: f32 = 8.0;
pub const MAX_SMOOTHING_MS: u32 = 10_000;
pub const MAX_SECTION_MULTIPLIER: f32 = 4.0;
pub const MAX_ADD_DEPTH: f32 = 360.0;
pub const MAX_SCALE_PERCENT: f32 = 400.0;

fn finite_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() { value } else { fallback }
}

/// How the envelope reaches the output value.
#[derive(Debug, Serialize, Deserialize, Clone, Copy, Encode, Decode, PartialEq)]
pub enum AudioModulationBlend {
    /// Signed offset added to the base value: `envelope * depth`.
    Add { depth: f32 },
    /// Fraction of the base value reached at full envelope:
    /// `base * envelope * peak_percent / 100`.
    Scale { peak_percent: f32 },
    /// Compatibility: replaces the base value outright with the legacy
    /// absolute output. Never offered for newly authored layers.
    LegacyAbsolute,
}

impl Default for AudioModulationBlend {
    fn default() -> Self {
        Self::Add { depth: 255.0 }
    }
}

impl AudioModulationBlend {
    pub fn is_compatibility(&self) -> bool {
        matches!(self, Self::LegacyAbsolute)
    }

    /// Blend modes offered when authoring a new layer.
    pub fn selectable() -> impl Iterator<Item = Self> {
        [
            Self::Add { depth: 255.0 },
            Self::Scale {
                peak_percent: 100.0,
            },
        ]
        .into_iter()
    }

    pub fn sanitize(&mut self) {
        match self {
            Self::Add { depth } => {
                *depth = finite_or(*depth, 0.0).clamp(-MAX_ADD_DEPTH, MAX_ADD_DEPTH)
            }
            Self::Scale { peak_percent } => {
                *peak_percent = finite_or(*peak_percent, 100.0).clamp(0.0, MAX_SCALE_PERCENT)
            }
            Self::LegacyAbsolute => {}
        }
    }
}

impl Display for AudioModulationBlend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Add { .. } => write!(f, "Add"),
            Self::Scale { .. } => write!(f, "Scale"),
            Self::LegacyAbsolute => write!(f, "Absolute (legacy)"),
        }
    }
}

impl Display for AnimationSpecBodyKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Encode, Decode, Default)]
pub struct AnimationSpecBodyPhaser {
    pub kind: PhaserKind,
    // Time to complete a complete cycle: cycle step time is calculated from this.
    pub time_total: PhaserDuration,
    pub pin_to_beat: bool,
    pub sync: SyncMode,
    /// If set, reverses the fixture timer assignment order every N full iterations.
    pub reverse_after_n_iterations: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize, Copy, Clone, PartialEq, Eq, Encode, Decode)]
pub enum PhaserDuration {
    Fixed(u64),                   // Total millis.
    Beat(AnimationSpeedModifier), // How many beats.
}

impl Default for PhaserDuration {
    fn default() -> Self {
        Self::Fixed(1000)
    }
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

impl Default for PhaserKind {
    fn default() -> Self {
        Self::Mathematical(MathematicalPhaser::default())
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Encode, Decode, Default)]
pub struct MathematicalPhaser {
    pub base: MathematicalBaseFunction,
    // TODO: this should actually be deprecated!
    pub stretch_factor: f32, // Between 0-1.
    pub amplitude_min: FixtureValue,
    pub amplitude_max: FixtureValue,
}

#[derive(Debug, Serialize, Deserialize, Copy, Clone, EnumIter, PartialEq, Eq, Encode, Decode)]
pub enum MathematicalBaseFunction {
    Sin,
    Cos,
    Triangle,
    Square1_2,
    Square1_8,
    Spike1_8,
    ExpSpike1_8,
    Sawtooth,
    EaseIn,
    EaseOut,
    EaseInOut,
}

impl Default for MathematicalBaseFunction {
    fn default() -> Self {
        Self::Sin
    }
}

impl Display for MathematicalBaseFunction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Encode, Decode, Default)]
pub struct KeyframedPhaser {}

#[derive(Debug, Serialize, Deserialize, Clone, Encode, Decode, Default)]
pub struct AnimationSpecBodyAudioVolume {}

#[derive(Debug, Serialize, Deserialize, Clone, Encode, Decode, Default)]
pub struct AnimationSpecBodyBpmValue {}

#[derive(Debug, Serialize, Deserialize, Clone, Encode, Decode, Default)]
pub struct AnimationSpecBodyBeat {}

#[derive(Debug, Serialize, Deserialize, Clone, Encode, Decode, PartialEq, Eq)]
pub struct AnimationSpecBodyFrequencies {
    pub gate: u8,
    pub boost: u8,
    pub freq_min: u16,
    pub freq_max: u16,
    pub normalization: FrequencyNormalization,
}

impl Default for AnimationSpecBodyFrequencies {
    fn default() -> Self {
        Self {
            gate: 0,
            boost: 0,
            freq_min: 0,
            freq_max: 20_000,
            normalization: FrequencyNormalization::default(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Copy, Clone, PartialEq, Eq, Encode, Decode, EnumIter)]
pub enum FrequencyNormalization {
    Logarithmic,
    Off,
    Flat,
}

impl Default for FrequencyNormalization {
    fn default() -> Self {
        Self::Off
    }
}

impl Display for FrequencyNormalization {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Encode, Decode, Default)]
pub struct AnimationSpecBodyWasm {}

#[cfg(test)]
mod tests {
    use super::EngineSelection;

    #[test]
    fn orphaned_fixture_filter_is_empty_selection() {
        let selection = EngineSelection {
            fixtures_in_group: [7].into_iter().collect(),
            ..Default::default()
        };

        assert!(selection.is_empty());
    }
}
