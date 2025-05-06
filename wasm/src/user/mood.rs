use crate::{
    blaulicht::{self, bl_controls_set, TickInput},
    colorize, printc, println, smidi,
    user::midi::{
        DDJ_RIGHT_HOT_CUE_0_1, DDJ_RIGHT_HOT_CUE_1_1, DDJ_RIGHT_HOT_CUE_2_1, MOOD_ALTERNATING, MOOD_BRIGHTNESS, MOOD_SNAKE, MOOD_SYNCED
    },
};

use super::{
    midi::{MOOD_FORCE_TOGGLE, MOOD_ON_BEAT},
    state::{MoodAnimation, State},
};
use map_range::MapRange;

fn animation_step(state: &mut State) {
    const UPPER_LIMIT: u16 = 360;

    state.animation.mood.counter += 1;
    state.animation.mood.counter %= UPPER_LIMIT;
}

const MOOD_LIGHT_START_ADDRS_LEFT: [usize; 5] = [225, 230, 235, 240, 245];

const MOOD_LIGHT_START_ADDRS_RIGHT: [usize; 5] = [200, 205, 210, 215, 220];

const MOOD_LIGHT_START_ADDRS: [usize; 1] = [250];

pub fn tick_on_beat(state: &mut State, dmx: &mut [u8], input: TickInput) {
    if state.animation.mood.controls.animation == MoodAnimation::Synced {
        tick_without_beat(state, dmx, input);
        return;
    }
}

pub fn tick_off_beat(state: &mut State, dmx: &mut [u8], input: TickInput) {
    if state.animation.mood.controls.animation == MoodAnimation::Synced {
        tick_without_beat(state, dmx, input);
        return;
    }
}

pub fn tick_without_beat(state: &mut State, dmx: &mut [u8], input: TickInput) {
    let brightness = match (
        state.animation.strobe.strobe_activate_time.is_some(),
        state.animation.mood.controls.force,
        state.animation.mood.controls.on_beat,
    ) {
        (false, _, false) | (true, true, false) => state.animation.mood.controls.brightness,
        (true, false, _) => 0,
        (false, _, true) | (true, true, true) => (input.volume as u16)
            .map_range(0..100, 0..state.animation.mood.controls.brightness as u16)
            as u8,
    };

    // TODO: make it variable when the step is called.
    animation_step(state);

    let counter = state.animation.mood.counter;
    let color = blaulicht::hsv_to_rgb(counter);

    for start in MOOD_LIGHT_START_ADDRS {
        colorize!(color, dmx, start);
        dmx[start + 3] = brightness as u8;
    }

    for start in MOOD_LIGHT_START_ADDRS_LEFT {
        colorize!(color, dmx, start);
        dmx[start + 3] = brightness as u8;
    }

    for start in MOOD_LIGHT_START_ADDRS_RIGHT {
        colorize!(color, dmx, start);
        dmx[start + 3] = brightness as u8;
    }
}

//
// Controls.
//

pub fn set_on_beat(state: &mut State, value: bool) {
    state.animation.mood.controls.on_beat = value;
    bl_controls_set(MOOD_ON_BEAT.0, MOOD_ON_BEAT.1, value);
    if value {
        println!("[MOOD] On beat ON.");
    } else {
        println!("[MOOD] On beat OFF.");
    }
}

pub fn set_force_on(state: &mut State, value: bool) {
    state.animation.mood.controls.force = value;
    bl_controls_set(MOOD_FORCE_TOGGLE.0, MOOD_FORCE_TOGGLE.1, value);
    if value {
        println!("[MOOD] Force ON.");
    } else {
        println!("[MOOD] Force OFF.");
    }
}

pub fn set_brightness(state: &mut State, value: u8) {
    state.animation.mood.controls.brightness = value;
    printc!(
        MOOD_BRIGHTNESS.0,
        MOOD_BRIGHTNESS.1,
        "BRI {value}",
    );
}

pub fn set_animation(state: &mut State, value: MoodAnimation) {
    let (x0_y1_value, x1_y1_value, x2_y1_value) = match &value {
        MoodAnimation::Synced => (127, 0, 0),
        MoodAnimation::Alternating { .. } => (0, 127, 0),
        MoodAnimation::LeftRightSnake { .. } => (0, 0, 127),
    };
    smidi!(DDJ_RIGHT_HOT_CUE_0_1, x0_y1_value);
    smidi!(DDJ_RIGHT_HOT_CUE_1_1, x1_y1_value);
    smidi!(DDJ_RIGHT_HOT_CUE_2_1, x2_y1_value);

    bl_controls_set(MOOD_SYNCED.0, MOOD_SYNCED.1, x0_y1_value == 127);
    bl_controls_set(MOOD_ALTERNATING.0, MOOD_ALTERNATING.1, x1_y1_value == 127);
    bl_controls_set(MOOD_SNAKE.0, MOOD_SNAKE.1, x2_y1_value == 127);

    state.animation.mood.controls.animation = value;
}
