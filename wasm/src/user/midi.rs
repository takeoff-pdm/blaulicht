use crate::blaulicht::MidiEvent;

use super::{mood, println, strobe, State};

pub const STROBE_ON_BEAT: (u8, u8) = (0, 0);
pub const STROBE_TOGGLE: (u8, u8) = (1, 0);
pub const STROBE_AUTOMATION_TOGGLE: (u8, u8) = (2, 0);
pub const STROBE_REMAINING_TIME: (u8, u8) = (3, 0);

pub const MOOD_ON_BEAT: (u8, u8) = (0, 1);
pub const MOOD_FORCE_TOGGLE: (u8, u8) = (1, 1);

pub fn tick(state: &mut State, midi: &[MidiEvent]) {
    for event in midi {
        match event.tup() {
            STROBE_ON_BEAT
            | MOOD_ON_BEAT
            | STROBE_TOGGLE
            | STROBE_AUTOMATION_TOGGLE
            | MOOD_FORCE_TOGGLE => {
                let value = (1 - event.value) != 0;
                match event.tup() {
                    STROBE_TOGGLE => strobe::set_strobe_on(state, value),
                    STROBE_AUTOMATION_TOGGLE => strobe::set_strobe_automation_on(state, value),
                    MOOD_FORCE_TOGGLE => mood::set_force_on(state, value),
                    STROBE_ON_BEAT => strobe::set_on_beat(state, value),
                    MOOD_ON_BEAT => mood::set_on_beat(state, value),
                    other => unreachable!("Got other: {:?}", other),
                }
            }
            _ => {
                println!("Unknown MIDI event: {:?}", event);
                continue;
            }
        }
    }
}
