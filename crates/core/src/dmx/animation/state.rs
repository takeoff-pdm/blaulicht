use blaulicht_shared::{AnimationSpeedModifier, FixtureProperty};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::fmt::Display;
use strum::EnumIter;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AnimationSpec {
    pub name: Cow<'static, str>,
    pub body: AnimationSpecBody,
    pub property: FixtureProperty,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum AnimationSpecBody {
    /// Phaser operates on a degree (0-360 DEG) an the amount is increased in time steps.
    Phaser(AnimationSpecBodyPhaser),
    AudioVolume(AnimationSpecBodyAudioVolume),
    Beat(AnimationSpecBodyBeat),
    Wasm(AnimationSpecBodyWasm), // TODO: not currently supported.
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AnimationSpecBodyPhaser {
    pub kind: PhaserKind,
    // Time to complete a complete cycle: cycle step time is calculated from this.
    pub time_total: PhaserDuration,
}

#[derive(Debug, Serialize, Deserialize, Copy, Clone, PartialEq, Eq)]
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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum PhaserKind {
    Mathematical(MathematicalPhaser),
    Keyframed(KeyframedPhaser),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MathematicalPhaser {
    pub base: MathematicalBaseFunction,
    // TODO: this should actually be deprecated!
    pub stretch_factor: f32, // Between 0-1.
    pub amplitude_min: u8,
    pub amplitude_max: u8,
}

#[derive(Debug, Serialize, Deserialize, Copy, Clone, EnumIter, PartialEq, Eq)]
pub enum MathematicalBaseFunction {
    Sin,
    Cos,
    Triangle,
    Square,
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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct KeyframedPhaser {}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AnimationSpecBodyAudioVolume {}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AnimationSpecBodyBeat {}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AnimationSpecBodyWasm {}
