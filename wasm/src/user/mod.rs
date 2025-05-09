mod beat;
mod init;
mod midi;
mod mood;
mod state;
mod strobe;
mod video;

use state::State;
use strobe::{mac_aura_pos, mac_aura_setup, moving_head_tick, MAC_AURA_START_ADDRS};

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

    // if elapsed!(input, state.animation.video.last_speed_update) > 1000 {
    //     (state.animation.video.speed = (state.animation.video.speed + 1) % 16);

    //     blaulicht::bl_udp("127.0.0.1:9000", &[200, 1, state.animation.video.speed]);
    //     println!("Sent speed UDP.");
    //     state.animation.video.last_speed_update = input.time;
    // }

    if elapsed!(input, state.init_time) < 6000 {
        state.was_initial = true;
        for addr in MAC_AURA_START_ADDRS {
            mac_aura_setup(input, state, dmx, addr);
        }
        return;
    }

    if state.was_initial {
        println!("SETUP COMPLETE.");
    }
    state.was_initial = false;

    moving_head_tick(state, dmx, input);

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
    video::tick(state, input, false);
}
