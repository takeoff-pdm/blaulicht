
//
// MIDI start.
//

use std::fmt::Display;

#[derive(Clone, Copy)]
pub enum MidiDevice {
    MidiMix,
    APCMini,
}

impl Display for MidiDevice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                MidiDevice::MidiMix => "MIDI Mix",
                MidiDevice::APCMini => "APC mini mk2",
            }
        )
    }
}

impl MidiDevice {
    pub fn index(&self) -> usize {
        match self {
            MidiDevice::MidiMix => 0,
            MidiDevice::APCMini => 1,
        }
    }
}
