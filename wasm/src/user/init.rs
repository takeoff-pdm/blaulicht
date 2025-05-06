use crate::{
    blaulicht::{self, TickInput},
    printc,
    user::{
        midi::{
            MOOD_ALTERNATING, MOOD_FORCE_TOGGLE, MOOD_ON_BEAT, MOOD_PALETTE_ALL,
            MOOD_PALETTE_CYAN_MAGENTA, MOOD_PALETTE_ORANGE_BLUE, MOOD_SNAKE, MOOD_SYNCED,
            STROBE_ALTERNATING, STROBE_AUTOMATION_TOGGLE, STROBE_ON_BEAT, STROBE_REMAINING_TIME,
            STROBE_SYNCED, STROBE_TOGGLE,
        },
        mood::{self, set_color_palette},
        state::{MoodAnimation, MoodColorPalette, StrobeAnimation},
        strobe,
    },
};

use super::state::State;

pub fn initialize(state: &mut State, input: TickInput, dmx: &mut [u8]) {
    state.reset();
    state.last_beat_time = input.time;

    // Turn off all channels.
    for i in 0..513 {
        dmx[i] = 0;
    }

    // Initialize the control surface.
    blaulicht::bl_controls_config(5, 7);
    printc!(STROBE_TOGGLE.0, STROBE_TOGGLE.1, "STROBE ENABLE");

    printc!(
        STROBE_AUTOMATION_TOGGLE.0,
        STROBE_AUTOMATION_TOGGLE.1,
        "STROBE AUTO"
    );
    strobe::set_strobe_automation_on(state, state.animation.strobe.controls.strobe_auto_enable);

    printc!(STROBE_ON_BEAT.0, STROBE_ON_BEAT.1, "STROBE ON BEAT");
    strobe::set_on_beat(state, state.animation.strobe.controls.on_beat);

    printc!(STROBE_SYNCED.0, STROBE_SYNCED.1, "STROBE SYNC");
    printc!(
        STROBE_ALTERNATING.0,
        STROBE_ALTERNATING.1,
        "STROBE ALTERNATING"
    );

    strobe::set_drop_duration(
        state,
        state.animation.strobe.controls.strobe_drop_duration_secs,
    );
    strobe::set_brightness(state, state.animation.strobe.controls.brightness);
    strobe::set_animation(state, dmx, StrobeAnimation::Synced);
    strobe::set_speed_multiplier(state, 127 / 2);

    strobe::set_on_multiplier(state, 127 / 2);
    strobe::set_off_multiplier(state, 127 / 2);

    mood::set_brightness(state, state.animation.mood.controls.brightness);

    printc!(MOOD_FORCE_TOGGLE.0, MOOD_FORCE_TOGGLE.1, "MOOD + STROBE",);
    mood::set_force_on(state, state.animation.mood.controls.force);

    printc!(MOOD_ON_BEAT.0, MOOD_ON_BEAT.1, "MOOD ON BEAT");
    mood::set_on_beat(state, state.animation.mood.controls.on_beat);

    printc!(MOOD_SYNCED.0, MOOD_SYNCED.1, "MOOD SYNC");
    printc!(MOOD_ALTERNATING.0, MOOD_ALTERNATING.1, "MOOD ALTERNATING");
    printc!(MOOD_SNAKE.0, MOOD_SNAKE.1, "MOOD SNAKE");
    mood::set_animation(state, state.animation.mood.controls.animation.clone());

    printc!(MOOD_PALETTE_ALL.0, MOOD_PALETTE_ALL.1, "CLR ALL");
    printc!(
        MOOD_PALETTE_CYAN_MAGENTA.0,
        MOOD_PALETTE_CYAN_MAGENTA.1,
        "CLR C/M"
    );
    printc!(
        MOOD_PALETTE_ORANGE_BLUE.0,
        MOOD_PALETTE_ORANGE_BLUE.1,
        "CLR O/B"
    );
    mood::set_color_palette(state, state.animation.mood.controls.color_palette.clone());

    println!("Initialized finished.");
}
