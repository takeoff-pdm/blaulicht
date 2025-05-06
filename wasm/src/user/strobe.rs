use serde::de::value;

use crate::{
    blaulicht::{self, bl_controls_set, nelapsed, TickInput},
    elapsed, printc, smidi,
    user::{
        midi::{
            self, DDJ_LEFT_BEAT_LOOP, DDJ_LEFT_CUE, DDJ_LEFT_HOT_CUE_0_1, DDJ_LEFT_HOT_CUE_1_1,
            STROBE_ALTERNATING, STROBE_BRIGHTNESS, STROBE_REMAINING_TIME, STROBE_SYNCED,
        },
        state::{StrobeAnimation, StrobeAnimationAlternatingState},
    },
};

use super::{
    midi::{STROBE_AUTOMATION_TOGGLE, STROBE_ON_BEAT, STROBE_TOGGLE},
    println, State,
};

const STROBE_RESET_TIME_MILLIS: u32 = 1000;
const STROBE_ACTIVE_MILLIS: u32 = 5000;
const STROBE_BURST_TIME_MILLIS: u32 = 500;

//
// Toggles.
//

pub fn set_strobe_on(state: &mut State, value: bool) {
    state.animation.strobe.controls.strobe_enabled = value;
    blaulicht::bl_controls_set(STROBE_TOGGLE.0, STROBE_TOGGLE.1, value);
    smidi!(DDJ_LEFT_CUE, value as u8 * 127);
}

pub fn set_strobe_automation_on(state: &mut State, value: bool) {
    state.animation.strobe.controls.strobe_auto_enable = value;
    blaulicht::bl_controls_set(
        STROBE_AUTOMATION_TOGGLE.0,
        STROBE_AUTOMATION_TOGGLE.1,
        value,
    );
}

pub fn set_on_beat(state: &mut State, value: bool) {
    state.animation.strobe.controls.on_beat = value;
    blaulicht::bl_controls_set(STROBE_ON_BEAT.0, STROBE_ON_BEAT.1, value);
    smidi!(DDJ_LEFT_BEAT_LOOP, value as u8 * 127);
}

pub fn set_animation(state: &mut State, value: StrobeAnimation) {
    let (x0_y1_value, x1_y1_value) = match &value {
        StrobeAnimation::Synced => (127, 0),
        StrobeAnimation::Alternating { .. } => (0, 127),
    };
    smidi!(DDJ_LEFT_HOT_CUE_0_1, x0_y1_value);
    smidi!(DDJ_LEFT_HOT_CUE_1_1, x1_y1_value);

    bl_controls_set(STROBE_SYNCED.0, STROBE_SYNCED.1, x0_y1_value == 127);
    bl_controls_set(
        STROBE_ALTERNATING.0,
        STROBE_ALTERNATING.1,
        x1_y1_value == 127,
    );

    state.animation.strobe.controls.strobe_animation = value;
}

pub fn set_brightness(state: &mut State, value: u8) {
    state.animation.strobe.controls.brightness = value;
    printc!(STROBE_BRIGHTNESS.0, STROBE_BRIGHTNESS.1, "BRI {value}",);
}

pub fn set_drop_duration(state: &mut State, value_secs: u32) {
    state.animation.strobe.controls.strobe_drop_duration_secs = value_secs;
    printc!(STROBE_REMAINING_TIME.0, STROBE_REMAINING_TIME.1, "/ ({value_secs}s)",);
}

//
// Fixtures.
//

fn mac_aura_set_white(dmx: &mut [u8], start_addr: usize, value: u8) {
    dmx[start_addr + 1] = value;
    dmx[start_addr + 9] = 255;
    dmx[start_addr + 10] = 255;
    dmx[start_addr + 11] = 255;
    dmx[start_addr + 12] = 255;
}

fn generic_3_chan_set_white(dmx: &mut [u8], start_addr: usize, value: u8) {
    dmx[start_addr + 0] = value;
    dmx[start_addr + 1] = value;
    dmx[start_addr + 2] = value;
}

//
// Ticks
//

const MAC_AURA_START_ADDRS: [usize; 6] = [100, 115, 130, 145, 160, 175];
const GENERIC_3_CHAN_START_ADDRS: [usize; 1] = [300];

pub fn tick_on_beat(dmx: &mut [u8], input: TickInput, state: &mut State) {
    if !state.animation.strobe.controls.strobe_enabled {
        return;
    }

    match state.animation.strobe.strobe_activate_time {
        Some(time)
            if elapsed!(input, time) < STROBE_BURST_TIME_MILLIS
                && state.animation.strobe.controls.on_beat
                && (StrobeAnimation::Synced
                    == state.animation.strobe.controls.strobe_animation) =>
        {
            // Burst strobe for the first X ms.
            // {
            //     mac_aura_set_white(
            //         dmx,
            //         100,
            //         state.animation.strobe.strobe_burst_state as u8
            //             * state.animation.strobe.controls.brightness,
            //     );

            //     state.animation.strobe.strobe_burst_state =
            //         !state.animation.strobe.strobe_burst_state;
            // }
        }
        Some(_) => match state.animation.strobe.controls.strobe_animation {
            StrobeAnimation::Synced => {
                for s in MAC_AURA_START_ADDRS {
                    mac_aura_set_white(dmx, s, state.animation.strobe.controls.brightness);
                }
                for s in GENERIC_3_CHAN_START_ADDRS {
                    generic_3_chan_set_white(dmx, s, state.animation.strobe.controls.brightness);
                }
            }
            StrobeAnimation::Alternating(_) => {
                alternating_on_beat_tick(state, dmx, input);
            }
        },
        None => {
            state.animation.strobe.strobe_activate_time = Some(input.time);
            println!("[STROBE] Strobe ENABLED.");
        }
    }
}

pub fn tick_off_beat(dmx: &mut [u8], input: TickInput, state: &mut State) {
    if !state.animation.strobe.controls.strobe_enabled {
        for s in MAC_AURA_START_ADDRS {
            mac_aura_set_white(dmx, s, 0);
        }
        for s in GENERIC_3_CHAN_START_ADDRS {
            generic_3_chan_set_white(dmx, s, 0);
        }
        return;
    }

    match state.animation.strobe.strobe_activate_time {
        Some(time)
            if elapsed!(input, time) < STROBE_BURST_TIME_MILLIS
                && state.animation.strobe.controls.on_beat =>
        {
            // Burst strobe for the first X ms.
            {
                for s in MAC_AURA_START_ADDRS {
                    mac_aura_set_white(
                        dmx,
                        s,
                        state.animation.strobe.strobe_burst_state as u8
                            * state.animation.strobe.controls.brightness,
                    );
                }
                for s in GENERIC_3_CHAN_START_ADDRS {
                    generic_3_chan_set_white(
                        dmx,
                        s,
                        state.animation.strobe.strobe_burst_state as u8
                            * state.animation.strobe.controls.brightness,
                    );
                }
                state.animation.strobe.strobe_burst_state =
                    !state.animation.strobe.strobe_burst_state;
            }
        }
        Some(_) => {
            if state.animation.strobe.strobe_burst_state {
                state.animation.strobe.strobe_burst_state = false;
                // for s in MAC_AURA_START_ADDRS {
                //     mac_aura_set_white(
                //         dmx,
                //         s,
                //         state.animation.strobe.controls.brightness,
                //     );
                // }
            } else {
                match &mut state.animation.strobe.controls.strobe_animation {
                    StrobeAnimation::Synced => {
                        for s in MAC_AURA_START_ADDRS {
                            mac_aura_set_white(dmx, s, 0);
                        }
                        for s in GENERIC_3_CHAN_START_ADDRS {
                            generic_3_chan_set_white(dmx, s, 0);
                        }
                    }
                    StrobeAnimation::Alternating(ref mut a) => {
                        for (k, v) in a.times.clone() {
                            if v.enabled
                                && elapsed!(input, v.change_time)
                                    > state.animation.strobe.controls.time_on_millis
                            {
                                let start_addr = MAC_AURA_START_ADDRS[k];
                                mac_aura_set_white(dmx, start_addr, 0);

                                a.times.insert(
                                    k,
                                    StrobeAnimationAlternatingState::disabled(input.time),
                                );

                                a.last_change_time = input.time;
                            }
                        }
                    }
                }
            }
        }
        None => {}
    }
}

// TODO: fix this so that the strobe actually works.
// pub fn tick(dmx: &mut [u8], input: TickInput, state: &mut State) {
//     if_beat!(input, state, {}, {});
// }

fn alternating_on_beat_tick(state: &mut State, dmx: &mut [u8], input: TickInput) {
    let StrobeAnimation::Alternating(ref mut a) = state.animation.strobe.controls.strobe_animation
    else {
        unreachable!("Called alternating strobe tick on non-alternating strobe animation");
    };

    // if elapsed!(input, a.last_change_time) < state.animation.strobe.controls.time_off_millis {
    //     return;
    // }

    let shall_enable = match a.times.get(&a.current_index) {
        None => {
            a.times.insert(
                a.current_index,
                StrobeAnimationAlternatingState::enabled(input.time),
            );
            true
        }
        Some(curr) => {
            if !curr.enabled
                && elapsed!(input, curr.change_time)
                    > state.animation.strobe.controls.time_off_millis
            {
                a.times.insert(
                    a.current_index,
                    StrobeAnimationAlternatingState::enabled(input.time),
                );
                true
            } else {
                false
            }
        }
    };

    let start_dmx_addr = MAC_AURA_START_ADDRS[a.current_index];
    mac_aura_set_white(
        dmx,
        start_dmx_addr,
        shall_enable as u8 * state.animation.strobe.controls.brightness,
    );

    if shall_enable {
        a.last_change_time = input.time;
        a.current_index = (a.current_index + 1) % MAC_AURA_START_ADDRS.len();
    }
}

pub fn auto_enable_tick(state: &mut State, input: TickInput) {
    if !state.animation.strobe.controls.on_beat {
        return;
    }

    const DISABLE_STROBE_THIS_AMOUNT_OF_MILLIS_AFTER_LAST_BEAT: u32 = 1000;
    const ENABLE_STROBE_BELOW_AVG_BASS_THRESHOLD: u8 = 50;

    // Update strobe activation status.
    let strobe_was_inactive_long_enough = elapsed!(input, state.last_beat_time)
        > STROBE_RESET_TIME_MILLIS
        && elapsed!(
            input,
            state.animation.strobe.strobe_deactivate_time.unwrap_or(0)
        ) > STROBE_RESET_TIME_MILLIS;

    if strobe_was_inactive_long_enough {
        state.animation.strobe.strobe_deactivate_time = Some(input.time);
        state.animation.strobe.strobe_activate_time = None;
    }

    if state.animation.strobe.controls.strobe_auto_enable {
        let remaining_strobe_time = STROBE_ACTIVE_MILLIS as i32
            - nelapsed!(
                input,
                state.animation.strobe.strobe_activate_time.unwrap_or(0)
            );

        show_remaining_strobe_time(state, Some(remaining_strobe_time));

        match state.animation.strobe.controls.strobe_enabled {
            true => {
                let since_last_beat_time = (elapsed!(input, state.last_beat_time) as u32);

                // Auto disable strobe.
                if remaining_strobe_time <= 0
                    && since_last_beat_time < DISABLE_STROBE_THIS_AMOUNT_OF_MILLIS_AFTER_LAST_BEAT
                // this should check that there is actually still a beat
                {
                    set_strobe_on(state, false);
                    println!("[STROBE] Auto DISABLED strobe.");
                }

                // Hide remaining indicator if strobe is not currently active.
                if since_last_beat_time > DISABLE_STROBE_THIS_AMOUNT_OF_MILLIS_AFTER_LAST_BEAT {
                    show_remaining_strobe_time(state, None);
                }
            }
            false => {
                // Auto enable strobe.
                if strobe_was_inactive_long_enough
                    && input.bass_avg < ENABLE_STROBE_BELOW_AVG_BASS_THRESHOLD
                {
                    set_strobe_on(state, true);
                    state.animation.strobe.strobe_deactivate_time = Some(input.time);
                    println!("[STROBE] Auto ENABLED strobe.");
                    show_remaining_strobe_time(state, None);
                }
            }
        }
    }
}

fn show_remaining_strobe_time(state: &mut State, remaining_strobe_time: Option<i32>) {
    match remaining_strobe_time {
        Some(remaining_strobe_time) => {
            let remaining_strobe_time_millis = remaining_strobe_time / 1000;

            if state.animation.strobe.strobe_activate_time.is_some()
                && remaining_strobe_time_millis > 0
                && state.animation.strobe.last_remaining_time_shown != remaining_strobe_time as u32
            {
                printc!(
                    STROBE_REMAINING_TIME.0,
                    STROBE_REMAINING_TIME.1,
                    "REM: {}s",
                    remaining_strobe_time_millis
                );
                state.animation.strobe.last_remaining_time_shown = remaining_strobe_time as u32;
            } else if state.animation.strobe.strobe_activate_time.is_some() {
                printc!(STROBE_REMAINING_TIME.0, STROBE_REMAINING_TIME.1, "OFF",);
            }
        }
        None => {
            printc!(STROBE_REMAINING_TIME.0, STROBE_REMAINING_TIME.1, "/",);
        }
    }
}
