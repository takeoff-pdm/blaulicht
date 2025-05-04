mod midi;
mod mood;
mod state;
mod strobe;

use midi::{MOOD_FORCE_TOGGLE, MOOD_ON_BEAT, STROBE_AUTOMATION_TOGGLE, STROBE_ON_BEAT, STROBE_TOGGLE};
use state::State;

#[deny(unsafe_op_in_unsafe_fn)]
#[allow(unused_imports)]
use crate::blaulicht::{
    self, colorize, elapsed, hsv_to_rgb, midi,
    prelude::{printc, println},
    MidiEvent, TickInput,
};
use crate::blaulicht::{bl_controls_config, bl_controls_set, bl_log};
// use map_range::MapRange;

pub fn initialize(input: TickInput, dmx: &mut [u8], data: *mut u8) {
    // STATE
    let state_ptr = data.cast::<State>();
    let state = unsafe { &mut *state_ptr };
    // STATE

    state.reset();
    state.last_beat_time = input.time;

    // Turn off all channels.
    for i in 0..513 {
        dmx[i] = 0;
    }

    // Initialize the control surface.
    bl_controls_config(4, 4);
    printc!(STROBE_TOGGLE.0, STROBE_TOGGLE.1, "STROBE");

    printc!(
        STROBE_AUTOMATION_TOGGLE.0,
        STROBE_AUTOMATION_TOGGLE.1,
        "STROBE AUTO"
    );
    strobe::set_strobe_automation_on(state, state.animation.strobe.controls.strobe_auto_enable);

    printc!(STROBE_ON_BEAT.0, STROBE_ON_BEAT.1, "STROBE ON BEAT");
    strobe::set_on_beat(state, state.animation.strobe.controls.on_beat);

    mood::set_brightness(state, state.animation.mood.controls.brightness);


    printc!(MOOD_ON_BEAT.0, MOOD_ON_BEAT.1, "MOOD ON BEAT");
    mood::set_on_beat(state, state.animation.mood.controls.on_beat);
    

    println!("Initialized finished.");
}

pub fn run(input: TickInput, dmx: &mut [u8], data: *mut u8, midi: &[MidiEvent]) {
    // STATE
    let state_ptr = data.cast::<State>();
    let state = unsafe { &mut *state_ptr };
    // STATE

    midi::tick(state, midi);

    mood::tick(state, dmx, input);

    // Call strobe auto enable logic regularly.
    strobe::tick(dmx, input, state);
    strobe::auto_enable_tick(state, input);
}
