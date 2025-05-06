use std::collections::HashMap;

use map_range::MapRange;

use crate::blaulicht::MidiEvent;

use super::{
    mood, println,
    state::{MoodAnimation, MoodColorPalette, StrobeAnimation},
    strobe, State,
};

//
// Web matrix.
//

pub const STROBE_ON_BEAT: (u8, u8) = (0, 0);
pub const STROBE_TOGGLE: (u8, u8) = (1, 0);
pub const STROBE_AUTOMATION_TOGGLE: (u8, u8) = (2, 0);
pub const STROBE_REMAINING_TIME: (u8, u8) = (3, 0);
pub const STROBE_BRIGHTNESS: (u8, u8) = (4, 0);
pub const STROBE_SPEED: (u8, u8) = (5, 0);
pub const STROBE_ON_OFF_TIME: (u8, u8) = (6, 0);

pub const STROBE_SYNCED: (u8, u8) = (0, 1);
pub const STROBE_ALTERNATING: (u8, u8) = (1, 1);

pub const MOOD_ON_BEAT: (u8, u8) = (0, 2);
pub const MOOD_FORCE_TOGGLE: (u8, u8) = (1, 2);
pub const MOOD_BRIGHTNESS: (u8, u8) = (4, 2);

pub const MOOD_SYNCED: (u8, u8) = (0, 3);
pub const MOOD_ALTERNATING: (u8, u8) = (1, 3);
pub const MOOD_SNAKE: (u8, u8) = (2, 3);

pub const MOOD_PALETTE_ALL: (u8, u8) = (0, 4);
pub const MOOD_PALETTE_CYAN_MAGENTA: (u8, u8) = (1, 4);
pub const MOOD_PALETTE_ORANGE_BLUE: (u8, u8) = (2, 4);

//
// DDJ-400.
//

pub const DDJ_LEFT_BEAT_SYNC: (u8, u8) = (144, 88);
pub const DDJ_LEFT_CUE: (u8, u8) = (144, 84);
pub const DDJ_LEFT_RELOOP: (u8, u8) = (144, 77);

pub const DDJ_RIGHT_BEAT_SYNC: (u8, u8) = (145, 88);
pub const DDJ_RIGHT_CUE: (u8, u8) = (145, 84);

pub const DDJ_LEFT_HOT_CUE_0_1: (u8, u8) = (151, 4);
pub const DDJ_LEFT_HOT_CUE_1_1: (u8, u8) = (151, 5);

pub const DDJ_LEFT_BEAT_JMP_0_0: (u8, u8) = (151, 32);
pub const DDJ_LEFT_BEAT_JMP_1_0: (u8, u8) = (151, 33);

pub const DDJ_RIGHT_HOT_CUE_0_1: (u8, u8) = (153, 4);
pub const DDJ_RIGHT_HOT_CUE_1_1: (u8, u8) = (153, 5);
pub const DDJ_RIGHT_HOT_CUE_2_1: (u8, u8) = (153, 6);

pub const DDJ_RIGHT_BEAT_LOOP_0_0: (u8, u8) = (153, 96);
pub const DDJ_RIGHT_BEAT_LOOP_1_0: (u8, u8) = (153, 97);
pub const DDJ_RIGHT_BEAT_LOOP_2_0: (u8, u8) = (153, 98);

pub const DDJ_RIGHT_BEAT_JMP_0_0: (u8, u8) = (153, 32);
pub const DDJ_RIGHT_BEAT_JMP_1_0: (u8, u8) = (153, 33);
pub const DDJ_RIGHT_BEAT_JMP_2_0: (u8, u8) = (153, 34);

pub const DDJ_VOL_FADER_LEFT: (u8, u8) = (176, 19);
pub const DDJ_VOL_FADER_RIGHT: (u8, u8) = (177, 19);

// Filters on the left.
pub const DDJ_ALL_FILTER_LEFT: (u8, u8) = (182, 23);
pub const DDJ_LOW_FILTER_LEFT: (u8, u8) = (176, 15);
pub const DDJ_MID_FILTER_LEFT: (u8, u8) = (176, 11);

// Filters on the right.
pub const DDJ_ALL_FILTER_RIGHT: (u8, u8) = (182, 24);

// pub const DDJ_RIGHT_CUE: (u8, u8) = (145, 84);

pub fn tick(state: &mut State, dmx: &mut [u8], midi_events: &[MidiEvent]) {
    for event in midi_events {
        match (event.tup(), event.value) {
            // Web Matrix.
            // (
            //     STROBE_ON_BEAT
            //     | MOOD_ON_BEAT
            //     | STROBE_TOGGLE
            //     | STROBE_AUTOMATION_TOGGLE
            //     | MOOD_FORCE_TOGGLE,
            //     _,
            // ) => {
            //     let value = (1 - event.value) != 0;
            //     match event.tup() {
            //         STROBE_TOGGLE => strobe::set_strobe_on(state, value),
            //         STROBE_AUTOMATION_TOGGLE => strobe::set_strobe_automation_on(state, value),
            //         MOOD_FORCE_TOGGLE => mood::set_force_on(state, value),
            //         STROBE_ON_BEAT => strobe::set_on_beat(state, value),
            //         other => unreachable!("Got other: {:?}", other),
            //     }
            // }
            // DDJ-400.
            // Strobe basics.
            (DDJ_LEFT_BEAT_SYNC, 127) => {
                strobe::set_on_beat(state, !state.animation.strobe.controls.on_beat)
            }
            (DDJ_LEFT_CUE, 127) => {
                strobe::set_strobe_on(state, !state.animation.strobe.controls.strobe_enabled)
            }
            (STROBE_AUTOMATION_TOGGLE, _) | (DDJ_LEFT_RELOOP, 127) => {
                strobe::set_strobe_automation_on(
                    state,
                    !state.animation.strobe.controls.strobe_auto_enable,
                )
            }
            (DDJ_VOL_FADER_LEFT, vol) => {
                strobe::set_brightness(state, (vol as u16).map_range(0..127, 0..255) as u8);
            }
            (DDJ_ALL_FILTER_LEFT, vol) => {
                strobe::set_speed_multiplier(state, vol);
            }
            (DDJ_LOW_FILTER_LEFT, vol) => {
                strobe::set_on_multiplier(state, vol);
            }
            (DDJ_MID_FILTER_LEFT, vol) => {
                strobe::set_off_multiplier(state, vol);
            }
            // Strobe animation.
            (STROBE_SYNCED, 0) | (DDJ_LEFT_BEAT_JMP_0_0, 127) => {
                strobe::set_animation(state, dmx, StrobeAnimation::Synced);
            }
            (STROBE_ALTERNATING, 0) | (DDJ_LEFT_BEAT_JMP_1_0, 127) => {
                strobe::set_animation(state, dmx, StrobeAnimation::alternating());
            }
            // Mood basics.
            (DDJ_VOL_FADER_RIGHT, vol) => {
                mood::set_brightness(state, (vol as u16).map_range(0..127, 0..255) as u8);
            }
            (MOOD_ON_BEAT, _) | (DDJ_RIGHT_BEAT_SYNC, 127) => {
                mood::set_on_beat(state, !state.animation.mood.controls.on_beat);
            }
            (MOOD_FORCE_TOGGLE, _) | (DDJ_RIGHT_CUE, 127) => {
                mood::set_force_on(state, !state.animation.mood.controls.force);
            }
            // Mood animation.
            (MOOD_SYNCED, 0) | (DDJ_RIGHT_BEAT_JMP_0_0, 127) => {
                mood::set_animation(state, MoodAnimation::Synced);
            }
            (MOOD_ALTERNATING, 0) | (DDJ_RIGHT_BEAT_JMP_1_0, 127) => {
                mood::set_animation(state, MoodAnimation::alternating());
            }
            (MOOD_SNAKE, 0) | (DDJ_RIGHT_BEAT_JMP_2_0, 127) => {
                mood::set_animation(state, MoodAnimation::snake());
            }
            // Mood color.
            (MOOD_PALETTE_ALL, 0) | (DDJ_RIGHT_BEAT_LOOP_0_0, 127) => {
                mood::set_color_palette(state, MoodColorPalette::All);
            }
            (MOOD_PALETTE_CYAN_MAGENTA, 0) | (DDJ_RIGHT_BEAT_LOOP_1_0, 127) => {
                mood::set_color_palette(state, MoodColorPalette::CyanMagenta);
            }
            (MOOD_PALETTE_ORANGE_BLUE, 0) | (DDJ_RIGHT_BEAT_LOOP_2_0, 127) => {
                mood::set_color_palette(state, MoodColorPalette::OrangeBlue);
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
