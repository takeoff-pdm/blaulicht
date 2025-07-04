use std::fmt::Display;

use bincode::{Decode, Encode, config};
use serde::{Deserialize, Serialize};

/// This event is emitted by the UI or the plugin system to control fixtures in the DMX engine.
/// All emitted events are processed by the DMX engine and applied to the fixtures.
/// Furthermore, they are also piped back into the plugin system to allow plugins to react to these events.
#[derive(Debug, Serialize, Deserialize, Encode, Decode, Copy, Clone)]
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
        Self {
            originator,
            body,
        }
    }

    pub fn requires_selection(&self) -> bool {
        match self.originator {
            EventOriginator::Web | EventOriginator::Plugin => self.body.requires_selection(),
            EventOriginator::DmxEngine => false,
        }
    }

    pub fn body(&self) -> ControlEvent {
        self.body
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

#[derive(Debug, Serialize, Deserialize, Encode, Decode, Copy, Clone)]
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
    //
    // Basic Fixture Actions.
    //
    // -------------------------------------------------------------------------------------------------------------------------------
    //
    /// If a fixture is disabled, it will not produce DMX output and all channels are set to 0.
    /// Of course, there are exceptions, e.g. if the fixture would have to be re-striked.
    SetEnabled(bool),
    /// Sets the brightness of the fixture, 0 is usually black, 255 is full brightness.
    SetBrightness(u8),
    /// Sets the color of the fixture using the RGB format.
    SetColor((u8, u8, u8)),
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
}

#[macro_export]
macro_rules! CONTROLS_REQUIRING_SELECTION {
    () => {
        ControlEvent::SetEnabled(_) | ControlEvent::SetBrightness(_) | ControlEvent::SetColor(_)
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
            | ControlEvent::MiscEvent { .. } => false,
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
