mod beat;
mod clock;
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

use clock::Time;
use state::State;

#[deny(unsafe_op_in_unsafe_fn)]
#[allow(unused_imports)]
use crate::blaulicht::{
    self, elapsed, midi,
    prelude::{printc, println},
    MidiEvent, TickInput,
};

static mut GLOBAL_TIME: Time = Time::new(0);

pub fn initialize(input: TickInput, dmx: &mut [u8], data: *mut u8) {
    // STATE
    let state_ptr = data.cast::<State>();
    let state = unsafe { &mut *state_ptr };
    // STATE

    unsafe {
        GLOBAL_TIME = Time::new(input.time);
    }

    init::initialize(state, input, dmx);
}

// Whether the entire setup shall be run on the first tick(s).
const DO_SETUP: bool = true;

fn on_setup_complete(state: &mut State) {
    // video::set_video_file(state, state::video::VideoFile::Hydra);
    println!("Setup complete hook.");
    println!("{:?}", state.config.strobe_groups);
    println!("filter: {:?}", state.beat_filter.sensitivity)
}

pub fn run(input: TickInput, dmx: &mut [u8], data: *mut u8, midi: &[MidiEvent]) {
    // STATE
    let state_ptr = data.cast::<State>();
    let state = unsafe { &mut *state_ptr };
    // STATE

    let now = Time::new(input.time);
    unsafe {
        GLOBAL_TIME = now;
    }

    // TODO: move this into beat utils.
    // if !state.animation.strobe.controls.on_beat {
    //     input.bpm = 180;
    //     input.time_between_beats_millis = 333;
    // }

    // midi::tick(state, dmx, midi, input);

    //
    // Setup
    //
    if DO_SETUP {
        if input.time < 6000 {
            state.was_initial = true;

            for group in state.config.strobe_groups.iter_mut() {
                group.setup(now, dmx);
            }

            for group in state.config.mood_groups.iter_mut() {
                group.setup(now, dmx);
            }

            for group in state.config.dimmer_groups.iter_mut() {
                group.setup(now, dmx);
            }

            return;
        }

        if state.was_initial {
            println!("[SETUP] Complete.");
            on_setup_complete(state);
        }
    }
    state.was_initial = false;

    //
    // Mood.
    //

    mood::tick_without_beat(state, dmx, input);

    //
    // Clocks.
    //

    state.clock.tick(input);
    let activated = state.clock.activate(|activation| {
        // println!("Clock: activation: {:?}", activation)
    });

    //
    // Beat filters.
    //

    state.beat_filter.tick(input);

    let is_on_beat = activated && state.beat_filter.is_open();

    let beat_filter_out = state.beat_filter.is_open() || state.beat_filter.is_open_first_time();

    state.drop_filter.beat_filter_in(beat_filter_out);

    // if state.beat_filter.is_open_first_time() {
    //     strobe::tick_on_beat(dmx, input, state);
    // }

    // Call on-beat routine(s).
    if is_on_beat || state.beat_filter.is_open_first_time() {
        // println!("there is a beat.");
        strobe::tick_on_beat(dmx, input, state);
    } else {
        strobe::tick_off_beat(dmx, input, state);
    }

    // fogger::tick(state, dmx);
    // strobe::tick(state, dmx, input);
    // dim::tick(dmx, input, state);
    // video::tick(state, input, false);
    // logo::tick(state, input);

    // Main on-beat logic.
    // crate::if_beat_strobe!(
    //     input,
    //     state,
    //     {
    //         // strobe::tick_on_beat(dmx, input, state);
    //     },
    //     {
    //         // strobe::tick_off_beat(dmx, input, state);
    //     }
    // );

    // crate::if_beat_mood!(
    //     input,
    //     state,
    //     {
    //         // mood::tick_on_beat(state, dmx, input);
    //     },
    //     {
    //         // mood::tick_off_beat(state, dmx, input);
    //     }
    // );

    // let beat_speed_multiplier = 1.0;

    //
    //
    //

    // let mut time_between_beats_millis = input.time_between_beats_millis as f32;
    // Apply speed modifier on the beat clock.
    // time_between_beats_millis *= beat_speed_multiplier;

    // if (time_between_beats_millis > 0.0) {
    // } else {
    // }
}
