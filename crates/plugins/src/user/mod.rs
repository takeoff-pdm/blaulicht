use blaulicht_shared::TickInput;
use crate::{blaulicht::prelude::println, midi::{MidiConnection, MidiEvent}};

struct State {
    counter: u32,
}

pub fn initialize(input: TickInput, data: *mut u8) {
    println!("Initialize user code here 1 a");
    let midi_handle = MidiConnection::open("DDJ-200");
    println!("MIDI handle outcome: {midi_handle:?}");
}

pub fn run(input: TickInput, data: *mut u8) {
    let state = unsafe { &mut *(data as *mut State) };
    state.counter += 1;

    // println!("run {}", state.counter);
}