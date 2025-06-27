use crate::{
    blaulicht::prelude::println,
    midi::{MidiConnection, MidiEvent},
};
use blaulicht_shared::TickInput;

struct State {
    counter: f32,
    midi_handle: MidiConnection,
    enabled: bool,
}

pub fn initialize(input: TickInput, data: *mut u8) {
    let mut state = unsafe { &mut *(data as *mut State) };

    println!("Initializing...");
    let midi_handle = MidiConnection::open("DDJ-400").unwrap();
    println!(
        "Got MIDI handle to device! HANDLE ID: {}",
        midi_handle.get_meta().device_id
    );

    *state = State {
        counter: 0.0,
        midi_handle,
        enabled: false,
    };
}

pub fn run(input: TickInput, data: *mut u8) {
    let state = unsafe { &mut *(data as *mut State) };

    let res = state.midi_handle.poll();

    let speed = 0.1;

    if !res.is_empty() {
        // println!("res {res:?}");
        for sig in res {
            if sig.status == 151 && sig.kind == 4 && sig.value == 127 {
                state.midi_handle.send(151, 4, state.enabled as u8 * 127);
                state.enabled = !state.enabled;
            } 

            if sig.status == 176 && sig.kind == 34 {
                if sig.value == 65 {
                    state.counter += speed;
                } else if sig.value == 63 {
                    state.counter -= speed;
                }

                println!("counter: {}", state.counter as u32);
            }
        }
    }

    // state.midi_handle.send(176, 90, state.counter as u8 % 127);
}
