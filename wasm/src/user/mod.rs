mod beat;
mod init;
mod midi;
mod mood;
mod state;
mod strobe;

use state::State;

#[deny(unsafe_op_in_unsafe_fn)]
#[allow(unused_imports)]
use crate::blaulicht::{
    self, colorize, elapsed, hsv_to_rgb, midi,
    prelude::{printc, println},
    MidiEvent, TickInput,
};

pub fn initialize(input: TickInput, dmx: &mut [u8], data: *mut u8) {
    // STATE
    let state_ptr = data.cast::<State>();
    let state = unsafe { &mut *state_ptr };
    // STATE

    init::initialize(state, input, dmx);
}

pub fn run(input: TickInput, dmx: &mut [u8], data: *mut u8, midi: &[MidiEvent]) {
    // STATE
    let state_ptr = data.cast::<State>();
    let state = unsafe { &mut *state_ptr };
    // STATE

    midi::tick(state, dmx, midi);

    // Main on-beat logic.
    crate::if_beat!(
        input,
        state,
        {
            strobe::tick_on_beat(dmx, input, state);
            mood::tick_on_beat(state, dmx, input);
        },
        {
            strobe::tick_off_beat(dmx, input, state);
            mood::tick_off_beat(state, dmx, input);
        }
    );

    // Call strobe auto enable logic regularly.
    strobe::auto_enable_tick(state, input);
}
