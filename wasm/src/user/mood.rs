use crate::{
    blaulicht::{self, bl_controls_set, TickInput},
    colorize, elapsed, printc, println, smidi,
    user::{
        midi::{
            DDJ_RIGHT_BEAT_JMP_0_0, DDJ_RIGHT_BEAT_JMP_1_0, DDJ_RIGHT_BEAT_JMP_2_0, DDJ_RIGHT_BEAT_LOOP_0_0, DDJ_RIGHT_BEAT_LOOP_1_0, DDJ_RIGHT_BEAT_LOOP_2_0, DDJ_RIGHT_BEAT_LOOP_3_0, DDJ_RIGHT_BEAT_SYNC, DDJ_RIGHT_CUE, DDJ_RIGHT_HOT_CUE_0_1, DDJ_RIGHT_HOT_CUE_1_1, DDJ_RIGHT_HOT_CUE_2_1, DDJ_RIGHT_RELOOP, MOOD_ALTERNATING, MOOD_BEAT_ANIM, MOOD_BRIGHTNESS, MOOD_PALETTE_ALL, MOOD_PALETTE_CYAN_MAGENTA, MOOD_PALETTE_ORANGE_BLUE, MOOD_PALETTE_WHITE, MOOD_SNAKE, MOOD_SPEED, MOOD_SPEED_BEAT, MOOD_SYNCED, STROBE_SPEED
        },
        strobe::parse_speed_multiplier,
    },
};

use super::{
    midi::{MOOD_FORCE_TOGGLE, MOOD_ON_BEAT},
    state::{MoodAnimation, MoodColorPalette, State},
};
use map_range::MapRange;

fn animation_step(state: &mut State, input: TickInput) {
    if state.animation.mood.controls.animation_on_beat {
        // TODO: relies on being called on beat.
    } else {
        let base_time = 100;
        if elapsed!(input, state.animation.mood.last_counter_update)
            < (base_time as f32 * state.animation.mood.animation_speed) as u32
        {
            return;
        }

        state.animation.mood.last_counter_update = input.time;
    }

    // Check if still in range.

    let curr_profile = state.animation.mood.controls.color_palette.to_hsv_ranges();
    let curr_range = match curr_profile.get(state.animation.mood.counter_range_index) {
        Some(i) => i,
        None => {
            state.animation.mood.counter_range_index = 0;
            &curr_profile[0]
        }
    };

    // If still legal value in range, advance in range index.
    match curr_range.contains(&(state.animation.mood.counter_in_range_value + 1)) {
        true => {
            state.animation.mood.counter_in_range_value += 1;
        }
        false => {
            state.animation.mood.counter_range_index =
                (state.animation.mood.counter_range_index + 1) % curr_profile.len();
            let new_range = &curr_profile[state.animation.mood.counter_range_index];
            state.animation.mood.counter_in_range_value = new_range.start;
        }
    };

    state.animation.mood.hsv = state.animation.mood.counter_in_range_value;

    // let hsv = {
    //     let ranges = state.animation.mood.controls.color_palette.to_hsv_ranges();

    //     let mut current_value = None;
    //     let mut last_idx = 0;
    //     for (idx, range) in ranges.iter().enumerate() {
    //         last_idx = idx;
    //         if range.contains(&(counter as usize)) {
    //             current_value = Some(counter);
    //             break;
    //         }
    //     }

    //     // TODO: make it variable when the step is called.
    //     let avg_len = ranges
    //         .iter()
    //         .map(|r| r.start.abs_diff(r.end))
    //         .sum::<usize>()
    //         / ranges.len();
    //     let base_time = 1000 / avg_len;
    //     println!("base_time: {base_time}");
    //     animation_step(state, input, base_time as u32);

    //     match current_value {
    //         Some(value) => value,
    //         None => {
    //             let next_idx = (last_idx + 1) % (ranges.len() - 1);
    //             let curr = ranges[next_idx].start as u16;
    //             // println!("RESET COUNT: {curr}");
    //             state.animation.mood.counter = curr;
    //             curr
    //         }
    //     }
    // };

    // const UPPER_LIMIT: u16 = 360;

    // state.animation.mood.counter_range_index += 1;
    // state.animation.mood.counter_range_index %= UPPER_LIMIT;
}

const MOOD_LIGHT_START_ADDRS_LEFT: [usize; 0] = [];

const MOOD_LIGHT_START_ADDRS_RIGHT: [usize; 0] = [];

const ADJ_MEGA_HEX: [usize; 26] = [
    25, 32, 39, 46, 53, 60, 67, 74, 81, 88, 95, 102, 109, 116, 123, 130, 137, 144, 151, 158, 165,
    172, 186, 193, 200, 207,
];

// TODO: isolate one strobe light into ambient.
// TODO: add ambient dimmers.

// TODO: add second strobe group
// TODO: add third strobe group (panels)
// TODO: group mood into left and right and always
// add other mood stuff and more strobes
// add fogger
// TODO: group secondary into left and right
pub const LITECRAFT_AT10: [usize; 16] = [
    240, 248, 256, 264, 272, 280, 288, 296, 304, 312, 320, 328, 336, 344, 352, 360,
];

pub fn tick_on_beat(state: &mut State, dmx: &mut [u8], input: TickInput) {
    if state.animation.mood.controls.animation_on_beat {
        animation_step(state, input);
    }

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
    if !state.animation.mood.controls.animation_on_beat {
        animation_step(state, input);
    }

    let brightness = match (
        (state.animation.strobe.strobe_activate_time.is_some()
            && state.animation.strobe.controls.strobe_enabled),
        state.animation.mood.controls.force,
        state.animation.mood.controls.on_beat,
    ) {
        (false, _, false) | (true, true, false) => state.animation.mood.controls.brightness,
        (true, false, _) => 0,
        (false, _, true) | (true, true, true) => (input.volume as u16)
            .map_range(0..100, 0..state.animation.mood.controls.brightness as u16)
            as u8,
    };

    // Get HSV based on counter and current color scheme.
    // TODO: this could be implemented in a more efficient way.

    // println!("HSV: {hsv}");
    let color = blaulicht::hsv_to_rgb(state.animation.mood.hsv);

    for start in ADJ_MEGA_HEX {
        colorize!(color, dmx, start);
        dmx[start + 6] = brightness as u8;
    }

    // for start in LITECRAFT_AT10 {
    //     colorize!(color, dmx, start);
    //     dmx[start + 7] = brightness as u8;
    // }

    // for start in MOOD_LIGHT_START_ADDRS_RIGHT {
    //     colorize!(color, dmx, start);
    //     dmx[start + 3] = brightness as u8;
    // }
}

//
// Controls.
//

pub fn set_speed_multiplier(state: &mut State, value: u8) {
    let (parsed_value, value_str) = parse_speed_multiplier(value);
    state.animation.mood.animation_speed = parsed_value;
    printc!(MOOD_SPEED.0, MOOD_SPEED.1, "M.SPEED {value_str}",);
}

pub fn set_speed_multiplier_beat(state: &mut State, value: u8) {
    let (parsed_value, value_str) = parse_speed_multiplier(value);
    state.animation.mood.animation_speed_beat = parsed_value;
    printc!(MOOD_SPEED_BEAT.0, MOOD_SPEED_BEAT.1, "M.B.SPEED {value_str}",);
}

pub fn set_on_beat(state: &mut State, value: bool) {
    state.animation.mood.controls.on_beat = value;
    bl_controls_set(MOOD_ON_BEAT.0, MOOD_ON_BEAT.1, value);
    smidi!(DDJ_RIGHT_BEAT_SYNC, value as u8 * 127);
}

pub fn set_on_beat_anim(state: &mut State, value: bool) {
    state.animation.mood.controls.animation_on_beat = value;
    bl_controls_set(MOOD_BEAT_ANIM.0, MOOD_BEAT_ANIM.1, value);
    smidi!(DDJ_RIGHT_RELOOP, value as u8 * 127);
}

pub fn set_force_on(state: &mut State, value: bool) {
    state.animation.mood.controls.force = value;
    bl_controls_set(MOOD_FORCE_TOGGLE.0, MOOD_FORCE_TOGGLE.1, value);
    smidi!(DDJ_RIGHT_CUE, value as u8 * 127);
}

pub fn set_brightness(state: &mut State, value: u8) {
    state.animation.mood.controls.brightness = value;
    printc!(MOOD_BRIGHTNESS.0, MOOD_BRIGHTNESS.1, "M.BRI {value:03}",);
}

pub fn set_animation(state: &mut State, value: MoodAnimation) {
    let (x0_y1_value, x1_y1_value, x2_y1_value) = match &value {
        MoodAnimation::Synced => (127, 0, 0),
        MoodAnimation::Alternating { .. } => (0, 127, 0),
        MoodAnimation::LeftRightSnake { .. } => (0, 0, 127),
    };

    smidi!(DDJ_RIGHT_BEAT_JMP_0_0, x0_y1_value);
    smidi!(DDJ_RIGHT_BEAT_JMP_1_0, x1_y1_value);
    smidi!(DDJ_RIGHT_BEAT_JMP_2_0, x2_y1_value);

    bl_controls_set(MOOD_SYNCED.0, MOOD_SYNCED.1, x0_y1_value == 127);
    bl_controls_set(MOOD_ALTERNATING.0, MOOD_ALTERNATING.1, x1_y1_value == 127);
    bl_controls_set(MOOD_SNAKE.0, MOOD_SNAKE.1, x2_y1_value == 127);

    state.animation.mood.controls.animation = value;
}

pub fn set_color_palette(state: &mut State, value: MoodColorPalette, input: TickInput) {
    let (x0_y0_value, x1_y0_value, x2_y0_value, x3_y0_value) = match &value {
        MoodColorPalette::All => (127, 0, 0, 0),
        MoodColorPalette::CyanMagenta => (0, 127, 0, 0),
        MoodColorPalette::OrangeBlue => (0, 0, 127, 0),
        MoodColorPalette::White => (0, 0, 0, 127),
    };

    smidi!(DDJ_RIGHT_BEAT_LOOP_0_0, x0_y0_value);
    smidi!(DDJ_RIGHT_BEAT_LOOP_1_0, x1_y0_value);
    smidi!(DDJ_RIGHT_BEAT_LOOP_2_0, x2_y0_value);
    smidi!(DDJ_RIGHT_BEAT_LOOP_3_0, x3_y0_value);

    bl_controls_set(MOOD_PALETTE_ALL.0, MOOD_PALETTE_ALL.1, x0_y0_value == 127);
    bl_controls_set(
        MOOD_PALETTE_CYAN_MAGENTA.0,
        MOOD_PALETTE_CYAN_MAGENTA.1,
        x1_y0_value == 127,
    );
    bl_controls_set(
        MOOD_PALETTE_ORANGE_BLUE.0,
        MOOD_PALETTE_ORANGE_BLUE.1,
        x2_y0_value == 127,
    );

    bl_controls_set(
        MOOD_PALETTE_WHITE.0,
        MOOD_PALETTE_WHITE.1,
        x3_y0_value == 127,
    );

    state.animation.mood.controls.color_palette = value;

    animation_step(state, input);
}
