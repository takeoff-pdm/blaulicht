use std::mem::MaybeUninit;

use crate::{
    blaulicht::{bl_send, prelude::println},
    midi::MidiConnection,
};
use blaulicht_shared::{ControlEvent, ControlEventCollection, TickInput};

struct State {
    counter: f32,
    midi_handle: MidiConnection,
    enabled: bool,
}

static mut STATE: MaybeUninit<State> = MaybeUninit::uninit();

pub fn initialize(input: TickInput) {
    println!("Initializing...");
    let midi_handle = MidiConnection::open("DDJ-400").unwrap();
    println!(
        "Got MIDI handle to device! HANDLE ID: {}",
        midi_handle.get_meta().device_id
    );

    unsafe {
        #[allow(static_mut_refs)]
        STATE.write(State {
            counter: 0.0,
            midi_handle,
            enabled: false,
        });
    }
}

pub fn run(input: TickInput) {
    let state = unsafe {
        #[allow(static_mut_refs)]
        STATE.assume_init_mut()
    };

    for ev in &input.events.events {
        println!("---> EVENT: {ev:?}");

        match ev {
            ControlEvent::SelectGroup(42) => {
                state.midi_handle.send(151, 4, 127);
                state.enabled = true;
            }
            ControlEvent::DeSelectGroup(42) => {
                state.midi_handle.send(151, 4, 0);
                state.enabled = false;
            }
            _ => {}
        }
    }

    let res = state.midi_handle.poll();
    let speed = 0.1;

    if !res.is_empty() {
        // println!("res {res:?}");
        for sig in res {
            if sig.status == 151 && sig.kind == 4 && sig.value == 127 {
                // state.midi_handle.send(151, 4, state.enabled as u8 * 127);
                state.enabled = !state.enabled;
                bl_send(if state.enabled {
                    ControlEvent::SelectGroup(42)
                } else {
                    ControlEvent::DeSelectGroup(42)
                });
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
