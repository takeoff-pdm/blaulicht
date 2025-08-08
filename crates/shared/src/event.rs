use std::fmt::Display;

use bincode::{Decode, Encode, config};
use serde::{Deserialize, Serialize};
use strum::EnumIter;

/// This event is emitted by the UI or the plugin system to control fixtures in the DMX engine.
/// All emitted events are processed by the DMX engine and applied to the fixtures.
/// Furthermore, they are also piped back into the plugin system to allow plugins to react to these events.
#[derive(Debug, Serialize, Deserialize, Encode, Decode, Clone)]
pub struct ControlEventMessage {
    originator: EventOriginator,
    body: ControlEvent,
}

impl Display for ControlEventMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl ControlEventMessage {
    pub fn new(originator: EventOriginator, body: ControlEvent) -> Self {
        Self { originator, body }
    }

    pub fn requires_selection(&self) -> bool {
        match self.originator {
            EventOriginator::Web | EventOriginator::Plugin => self.body.requires_selection(),
            EventOriginator::DmxEngine => false,
        }
    }

    pub fn body(&self) -> ControlEvent {
        self.body.clone()
    }

    pub fn originator(&self) -> EventOriginator {
        self.originator
    }

    pub fn serialize(&self) -> Vec<u8> {
        bincode::encode_to_vec(self, config::standard()).unwrap()
    }

    pub fn deserialize(buf: &[u8]) -> Self {
        let (data, _) = bincode::decode_from_slice(buf, config::standard()).unwrap();
        data
    }
}

#[derive(Debug, Serialize, Deserialize, Encode, Decode, Copy, Clone, PartialEq, PartialOrd)]
pub enum EventOriginator {
    Web,
    DmxEngine,
    Plugin,
}

#[derive(Serialize, Deserialize, Debug, Copy, Clone, EnumIter, PartialEq, Eq, Encode, Decode)]
pub enum AnimationSpeedModifier {
    _1_16,
    _1_8,
    _1_4,
    _1_2,
    _1,
    _2,
    _4,
    _8,
}

impl TryFrom<f32> for AnimationSpeedModifier {
    type Error = ();

    fn try_from(value: f32) -> Result<Self, Self::Error> {
        const _1_16: f32 = 16.0;
        const _1_8: f32 = 8.0;
        const _1_4: f32 = 4.0;
        const _1_2: f32 = 2.0;
        const _1: f32 = 1.0;
        const _2: f32 = 1.0 / 2.0;
        const _4: f32 = 1.0 / 4.0;
        const _8: f32 = 1.0 / 8.0;

        Ok(match value {
            _1_16 => Self::_1_16,
            _1_8 => Self::_1_8,
            _1_4 => Self::_1_4,
            _1_2 => Self::_1_2,
            _1 => Self::_1,
            _2 => Self::_2,
            _4 => Self::_4,
            _8 => Self::_8,
            _ => return Err(()),
        })
    }
}

impl AnimationSpeedModifier {
    pub const ALL: [Self; 8] = [
        Self::_1_16,
        Self::_1_8,
        Self::_1_4,
        Self::_1_2,
        Self::_1,
        Self::_2,
        Self::_4,
        Self::_8,
    ];

    pub fn as_index(&self) -> usize {
        Self::ALL.iter().position(|&v| v == *self).unwrap()
    }

    pub fn from_index(index: usize) -> Self {
        Self::ALL[index]
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::_1_16 => "1/16",
            Self::_1_8 => "1/8",
            Self::_1_4 => "1/4",
            Self::_1_2 => "1/2",
            Self::_1 => "1",
            Self::_2 => "2",
            Self::_4 => "4",
            Self::_8 => "8",
        }
    }

    pub fn as_float(&self) -> f32 {
        match self {
            AnimationSpeedModifier::_1_16 => 16.0,
            AnimationSpeedModifier::_1_8 => 8.0,
            AnimationSpeedModifier::_1_4 => 4.0,
            AnimationSpeedModifier::_1_2 => 2.0,
            AnimationSpeedModifier::_1 => 1.0,
            AnimationSpeedModifier::_2 => 1.0 / 2.0,
            AnimationSpeedModifier::_4 => 1.0 / 4.0,
            AnimationSpeedModifier::_8 => 1.0 / 8.0,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Encode, Decode, Clone, EnumIter, PartialEq, Eq)]
pub enum FixtureProperty {
    Brightness,
    ColorHue,
    ColorSaturation,
    ColorValue,
    Tilt,
    Pan,
    Rotation,
}

#[derive(Debug, Serialize, Deserialize, Encode, Decode, Clone)]
pub enum ControlEvent {
    //
    // Selections Actions.
    //
    // -------------------------------------------------------------------------------------------------------------------------------
    //
    /// Selects a group by its index.
    /// Operations will be limited to this group but applied to all fixtures in the group.
    SelectGroup(u8),

    // Removes the current selection of this group.
    DeSelectGroup(u8),

    // Limits application of operations of the current group to the fixture matching the given index relative to the group.
    // Only works if exactly one group is selected.
    LimitSelectionToFixtureInCurrentGroup(u8),

    // Undoes the operation from above.
    UnLimitSelectionToFixtureInCurrentGroup(u8),

    /// Removes the current selection.
    /// Works top-down: if a fixture is selected, it will be removed first then the group.
    RemoveSelection,

    /// Removes everything that is selected.
    RemoveAllSelection,

    /// Pushes the current selection to the selection stack, making the current selection empty.
    PushSelection,

    /// Pops the top selection off the selection stack, replacing the current selection.
    PopSelection,

    //
    // Basic Fixture Actions.
    //
    // -------------------------------------------------------------------------------------------------------------------------------
    //
    /// If a fixture is disabled, it will not produce DMX output and all channels are set to 0.
    /// Of course, there are exceptions, e.g. if the fixture would have to be re-striked.
    SetEnabled(bool),
    /// Sets the brightness of the fixture, 0 is usually black, 255 is full brightness.
    /// TODO: replace this with set-property messages
    SetBrightness(u8),
    /// Sets the color of the fixture using the RGB format.
    SetColor((u8, u8, u8)),
    //
    // Animations
    //
    AddAnimation(u8),
    RemoveAnimation(u8),
    ResetAnimation(u8),
    PauseAnimation(u8),
    PlayAnimation(u8),
    SetAnimationSpeed(u8, AnimationSpeedModifier),
    // TODO: animation speed modifier or something like this.
    //
    //
    // Other Fixture Actions.
    //
    // -------------------------------------------------------------------------------------------------------------------------------
    /// Could set an arbitrary value to a fixture channel.
    /// Alternatively, can be used to communicate with plugins.
    MiscEvent {
        descriptor: u8,
        value: u8,
    },
    // Transaction Actions.
    Transaction(Vec<ControlEvent>),
}

#[macro_export]
macro_rules! CONTROLS_REQUIRING_SELECTION {
    () => {
        ControlEvent::SetEnabled(_)
            | ControlEvent::SetBrightness(_)
            | ControlEvent::SetColor(_)
            | ControlEvent::AddAnimation(_)
            | ControlEvent::RemoveAnimation(_)
            | ControlEvent::ResetAnimation(_)
            | ControlEvent::PauseAnimation(_)
            | ControlEvent::PlayAnimation(_)
            | ControlEvent::SetAnimationSpeed(_, _)
    };
}

impl ControlEvent {
    pub fn serialize(&self) -> Vec<u8> {
        bincode::encode_to_vec(self, config::standard()).unwrap()
    }

    pub fn deserialize(buf: &[u8]) -> Self {
        let (data, _) = bincode::decode_from_slice(buf, config::standard()).unwrap();
        data
    }

    pub fn requires_selection(&self) -> bool {
        match self {
            CONTROLS_REQUIRING_SELECTION!() => true,
            ControlEvent::SelectGroup(_)
            | ControlEvent::DeSelectGroup(_)
            | ControlEvent::LimitSelectionToFixtureInCurrentGroup(_)
            | ControlEvent::UnLimitSelectionToFixtureInCurrentGroup(_)
            | ControlEvent::RemoveSelection
            | ControlEvent::RemoveAllSelection
            | ControlEvent::MiscEvent { .. }
            | ControlEvent::PopSelection
            | ControlEvent::PushSelection => false,
            ControlEvent::Transaction(_) => false,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Encode, Decode, Default, Clone)]
pub struct ControlEventCollection {
    pub events: Vec<ControlEventMessage>,
}

impl ControlEventCollection {
    pub fn serialize(&self) -> Vec<u8> {
        bincode::encode_to_vec(self, config::standard()).unwrap()
    }

    pub fn deserialize(buf: &[u8]) -> Self {
        let (data, _) = bincode::decode_from_slice(buf, config::standard()).unwrap();
        data
    }
}
