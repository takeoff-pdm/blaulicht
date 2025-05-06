use std::collections::HashMap;

use crate::blaulicht::MidiEvent;

use super::{mood, println, state::{MoodAnimation, StrobeAnimation}, strobe, State};

//
// Web matrix.
//

pub const STROBE_ON_BEAT: (u8, u8) = (0, 0);
pub const STROBE_TOGGLE: (u8, u8) = (1, 0);
pub const STROBE_AUTOMATION_TOGGLE: (u8, u8) = (2, 0);
pub const STROBE_REMAINING_TIME: (u8, u8) = (3, 0);
pub const STROBE_BRIGHTNESS: (u8, u8) = (4, 0);

pub const STROBE_SYNCED: (u8, u8) = (0, 1);
pub const STROBE_ALTERNATING: (u8, u8) = (1, 1);

pub const MOOD_ON_BEAT: (u8, u8) = (0, 2);
pub const MOOD_FORCE_TOGGLE: (u8, u8) = (1, 2);
pub const MOOD_BRIGHTNESS: (u8, u8) = (4, 2);

pub const MOOD_SYNCED: (u8, u8) = (0, 3);
pub const MOOD_ALTERNATING: (u8, u8) = (1, 3);
pub const MOOD_SNAKE: (u8, u8) = (2, 3);

//
// DDJ-400.
//

pub const DDJ_LEFT_BEAT_LOOP: (u8, u8) = (144, 88);
pub const DDJ_LEFT_CUE: (u8, u8) = (144, 84);

pub const DDJ_LEFT_HOT_CUE_0_1: (u8, u8) = (151, 4);
pub const DDJ_LEFT_HOT_CUE_1_1: (u8, u8) = (151, 5);

pub const DDJ_RIGHT_HOT_CUE_0_1: (u8, u8) = (153, 4);
pub const DDJ_RIGHT_HOT_CUE_1_1: (u8, u8) = (153, 5);
pub const DDJ_RIGHT_HOT_CUE_2_1: (u8, u8) = (153, 6);

// pub const DDJ_RIGHT_CUE: (u8, u8) = (145, 84);

pub fn tick(state: &mut State, midi_events: &[MidiEvent]) {
    for event in midi_events {
        match (event.tup(), event.value) {
            // Web Matrix.
            (
                STROBE_ON_BEAT
                | MOOD_ON_BEAT
                | STROBE_TOGGLE
                | STROBE_AUTOMATION_TOGGLE
                | MOOD_FORCE_TOGGLE,
                _,
            ) => {
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
            // DDJ-400.
            // Strobe.
            (DDJ_LEFT_BEAT_LOOP, 127) => {
                strobe::set_on_beat(state, !state.animation.strobe.controls.on_beat)
            }
            (DDJ_LEFT_CUE, 127) => {
                strobe::set_strobe_on(state, !state.animation.strobe.controls.strobe_enabled)
            }
            (STROBE_SYNCED, 0) | (DDJ_LEFT_HOT_CUE_0_1, 127) => {
                strobe::set_animation(state, StrobeAnimation::Synced);
            }
            (STROBE_ALTERNATING, 0) | (DDJ_LEFT_HOT_CUE_1_1, 127) => {
                strobe::set_animation(state, StrobeAnimation::alternating());
            }
            // Mood
            (MOOD_SYNCED, 0) | (DDJ_RIGHT_HOT_CUE_0_1, 127) => {
                mood::set_animation(state, MoodAnimation::Synced);
            }
            (MOOD_ALTERNATING, 0) | (DDJ_RIGHT_HOT_CUE_1_1, 127) => {
                mood::set_animation(state, MoodAnimation::alternating());
            }
            (MOOD_SNAKE, 0) | (DDJ_RIGHT_HOT_CUE_2_1, 127) => {
                mood::set_animation(state, MoodAnimation::snake());
            }
            _ => {
                if event.value != 0 {
                    println!("Unknown MIDI event: {:?}", event);
                }
                continue;
            }
        }
    }
}
