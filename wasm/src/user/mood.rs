use crate::{
    blaulicht::{self, bl_controls_set, TickInput},
    colorize, printc, println,
};

use super::{
    midi::{MOOD_FORCE_TOGGLE, MOOD_ON_BEAT},
    state::State,
};
use map_range::MapRange;

fn animation_step(state: &mut State) {
    const UPPER_LIMIT: u16 = 360;

    state.animation.mood.counter += 1;
    state.animation.mood.counter %= UPPER_LIMIT;
}

const MOOD_LIGHT_START_ADDRS: [usize; 1] = [20];

pub fn tick(state: &mut State, dmx: &mut [u8], input: TickInput) {
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
}

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
        MOOD_FORCE_TOGGLE.0,
        MOOD_FORCE_TOGGLE.1,
        "MOOD FORCE ({value})",
    );
}
