use crate::{blaulicht::TickInput, elapsed, printc, user::midi::DIM_BRIGHTNESS_STAGE};

use super::state::State;

pub const DIMMERS_STAGE: [usize; 12] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];

pub const DIMMERS_NOT_STAGE: [usize; 2] = [
    // 13, // stage
    // 14, // stage
    // 15, // nothing
    // 16, // nothing
    // 17, // stage
    // 18, // stage
    // 19, // nothing.
    20, 21,
];

pub fn tick(dmx: &mut [u8], input: TickInput, state: &mut State) {
    if elapsed!(input, state.last_beat_time) <= 500 && state.animation.strobe.controls.strobe_enabled {
        // Dim during strobe.
        state.animation.dim.stage_real_brightness = 40;
    } else {
        state.animation.dim.stage_real_brightness = state.animation.dim.controls.brightness_stage;
    }

    for addr in DIMMERS_NOT_STAGE {
        dmx[addr] = state.animation.dim.controls.brightness_other;
    }

    for addr in DIMMERS_STAGE {
        dmx[addr] = state.animation.dim.stage_real_brightness;
    }
}

pub fn set_brightness_stage(state: &mut State, value: u8) {
    state.animation.dim.controls.brightness_stage = value;
    printc!(
        DIM_BRIGHTNESS_STAGE.0,
        DIM_BRIGHTNESS_STAGE.1,
        "D.BRI {value:03}",
    );
}

pub fn set_brightness_other(state: &mut State, value: u8) {
    state.animation.dim.controls.brightness_other = value;
    printc!(
        DIM_BRIGHTNESS_STAGE.0,
        DIM_BRIGHTNESS_STAGE.1,
        "D.BRI {value:03}",
    );
}
