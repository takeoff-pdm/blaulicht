use crate::{
    blaulicht::{self, nelapsed, TickInput},
    elapsed, printc,
    user::midi::STROBE_REMAINING_TIME,
};

use super::{
    midi::{STROBE_AUTOMATION_TOGGLE, STROBE_ON_BEAT, STROBE_TOGGLE},
    println, State,
};

const STROBE_RESET_TIME_MILLIS: u32 = 1000;
const STROBE_ACTIVE_MILLIS: u32 = 5000;
const STROBE_BURST_TIME_MILLIS: u32 = 500;

//
// Macros.
//

#[macro_export]
macro_rules! if_beat {
    ($input:expr, $state:expr, $body: expr, $not_body: expr) => {
        let mut time_between_beats_millis = $input.time_between_beats_millis as u32;
        if !$state.animation.strobe.controls.on_beat {
            time_between_beats_millis = 200;
        }

        if $input.time - $state.last_beat_time >= time_between_beats_millis {
            // Check that there is a beat.
            if $input.bass_avg > 150
                || $input.bass > 150
                || !$state.animation.strobe.controls.on_beat
            {
                $body;
                $state.last_beat_time = $input.time;
            }
        } else {
            $not_body;
        }
    };
}

//
// Toggles.
//

pub fn set_strobe_on(state: &mut State, value: bool) {
    state.animation.strobe.controls.strobe_enabled = value;
    blaulicht::bl_controls_set(STROBE_TOGGLE.0, STROBE_TOGGLE.1, value);
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

//
// Ticks
//

// TODO: fix this so that the strobe actually works.
pub fn tick(dmx: &mut [u8], input: TickInput, state: &mut State) {
    if !state.animation.strobe.controls.strobe_enabled {
        mac_aura_set_white(dmx, 100, 0);
        return;
    }

    if_beat!(
        input,
        state,
        {
            println!("{}", input.time);
            match state.animation.strobe.strobe_activate_time {
                Some(time)
                    if elapsed!(input, time) < STROBE_BURST_TIME_MILLIS
                        && state.animation.strobe.controls.on_beat =>
                {
                    // Burst strobe for the first X ms.
                    {
                        mac_aura_set_white(
                            dmx,
                            100,
                            state.animation.strobe.strobe_burst_state as u8 * 255,
                        );
                        state.animation.strobe.strobe_burst_state =
                            !state.animation.strobe.strobe_burst_state;
                    }
                }
                Some(_) => {
                    mac_aura_set_white(dmx, 100, 255); // TODO: use strobe brightness.
                }
                None => {
                    state.animation.strobe.strobe_activate_time = Some(input.time);
                    println!("[STROBE] Strobe ENABLED.");
                }
            }
        },
        {
            match state.animation.strobe.strobe_activate_time {
                Some(time)
                    if elapsed!(input, time) < STROBE_BURST_TIME_MILLIS
                        && state.animation.strobe.controls.on_beat =>
                {
                    // Burst strobe for the first X ms.
                    {
                        mac_aura_set_white(
                            dmx,
                            100,
                            state.animation.strobe.strobe_burst_state as u8 * 255,
                        );
                        state.animation.strobe.strobe_burst_state =
                            !state.animation.strobe.strobe_burst_state;
                    }
                }
                Some(_) => {
                    mac_aura_set_white(dmx, 100, 0);
                }
                None => {}
            }
        }
    );
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

        let remaining_strobe_time_millis = remaining_strobe_time / 1000;

        if state.animation.strobe.strobe_activate_time.is_some()
            && remaining_strobe_time_millis > 0
            && state.animation.strobe.last_remaining_time_shown
                != remaining_strobe_time_millis as u32
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

        match state.animation.strobe.controls.strobe_enabled {
            true => {
                // Auto disable strobe.
                if remaining_strobe_time <= 0
                    && (elapsed!(input, state.last_beat_time) as u32)
                        < DISABLE_STROBE_THIS_AMOUNT_OF_MILLIS_AFTER_LAST_BEAT
                // this should check that there is actually still a beat
                {
                    set_strobe_on(state, false);
                    println!("[STROBE] Auto DISABLED strobe.");
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
                }
            }
        }
    }
}
