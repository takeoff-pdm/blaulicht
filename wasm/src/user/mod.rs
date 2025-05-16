mod beat;
mod config;
mod dim;
mod fogger;
mod init;
mod logo;
mod midi;
mod mood;
mod state;
mod strobe;
mod video;

use config::{Config, Fixture, FixtureType, Light};
use state::State;
use strobe::{mac_aura_setup, MAC_AURA_START_ADDRS};

#[deny(unsafe_op_in_unsafe_fn)]
#[allow(unused_imports)]
use crate::blaulicht::{
    self, elapsed, midi,
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

// Whether the entire setup shall be run on the first tick(s).
const DO_SETUP: bool = true;

fn on_setup_complete(state: &mut State) {
    video::set_video_file(state, state::VideoFile::Hydra);
}

pub fn run(mut input: TickInput, dmx: &mut [u8], data: *mut u8, midi: &[MidiEvent]) {
    // STATE
    let state_ptr = data.cast::<State>();
    let state = unsafe { &mut *state_ptr };
    // STATE

    // if midi.len() > 0 {
    //     println!("{:?}", midi)
    // }

    // TODO: move this into beat utils.
    if !state.animation.strobe.controls.on_beat {
        input.bpm = 180;
        input.time_between_beats_millis = 333;
    }

    midi::tick(state, dmx, midi, input);

    //
    // Setup
    //
    if DO_SETUP {
        if elapsed!(input, state.init_time) < 6000 {
            state.was_initial = true;
            for addr in MAC_AURA_START_ADDRS {
                mac_aura_setup(input, state, dmx, addr);
            }
            return;
        }

        if state.was_initial {
            println!("[SETUP] Complete.");
            on_setup_complete(state);
        }
    }
    state.was_initial = false;

    fogger::tick(state, dmx);
    strobe::tick(state, dmx, input);
    dim::tick(dmx, input, state);
    video::tick(state, input, false);
    logo::tick(state, input);

    // Main on-beat logic.
    crate::if_beat_strobe!(
        input,
        state,
        {
            strobe::tick_on_beat(dmx, input, state);
        },
        {
            strobe::tick_off_beat(dmx, input, state);
        }
    );

    crate::if_beat_mood!(
        input,
        state,
        {
            mood::tick_on_beat(state, dmx, input);
        },
        {
            mood::tick_off_beat(state, dmx, input);
        }
    );
}
