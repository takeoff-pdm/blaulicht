use std::mem::MaybeUninit;

use crate::{
    blaulicht::{bl_send, prelude::println},
    midi::{MidiConnection, MidiEvent},
};
use blaulicht_shared::{ControlEvent, ControlEventCollection, TickInput};

struct State {
    counter: f32,
    midi_handles: Vec<MidiConnection>,
    enabled: bool,
    last_update: u32,
}

static mut STATE: MaybeUninit<State> = MaybeUninit::uninit();

pub fn initialize(input: TickInput) {
    println!("Initializing...");

    let devices = vec![
        // "MIDI Mix",
        // "nanoKONTROL Studio CTRL",
        // "APC mini mk2 Control",
    ];

    let mut midi_handles = vec![];

    for dev in devices {
        let midi_handle = MidiConnection::open(dev).unwrap();
        println!(
            "Got MIDI handle to device! HANDLE ID: {}",
            midi_handle.get_meta().device_id
        );

        midi_handles.push(midi_handle);
    }

    unsafe {
        #[allow(static_mut_refs)]
        STATE.write(State {
            counter: 0.0,
            midi_handles,
            enabled: false,
            last_update: 0,
        });
    }
}

fn midimix(conn: MidiConnection, ev: Vec<MidiEvent>, state: &mut State) {
    for e in ev {
        match (e.status, e.kind, e.value) {
            (144, 1, 127) => {
                state.enabled = !state.enabled;
                conn.send(144, 1, state.counter as u8 % 127);
                state.counter += 1.0;
            }
            (128, 1, 127) => {
                // conn.send(144, 1, 0);
            }
            _ => {
                println!("{}: {:?}", conn.get_meta().device_id, e);
            }
        }
    }
}

fn nano(conn: MidiConnection, ev: Vec<MidiEvent>, state: &mut State) {
    // conn.send(0x90, 24, 127);
    for i in 0..255 {
        conn.send(0x90, i, 127);
    }

   // for i in 0..2 {
    //     for j in 0..10 {
    //         conn.send(0x91 + i, j, 127);
    //     }
    // }

    for e in ev {
        match (e.status, e.kind, e.value) {
            (179, 46, 127) => {
                state.enabled = !state.enabled;
            }
            _ => {
                println!("{}: {:?}", conn.get_meta().device_id, e);
            }
        }
    }
}

fn apc(conn: MidiConnection, ev: Vec<MidiEvent>, state: &mut State, input: TickInput) {
    if input.clock - state.last_update > 0 {
        for i in 0..64 {
            conn.send(0x96, i as u8, (state.counter as usize % 127) as u8);
        }
        state.counter += 1.0;
        state.last_update = input.clock;
        println!("udate");
    }

    for e in ev {
        match (e.status, e.kind, e.value) {
            _ => {
                println!("{}: {:?}", conn.get_meta().device_id, e);
            }
        }
    }
}

pub fn run(input: TickInput) {
    let state = unsafe {
        #[allow(static_mut_refs)]
        STATE.assume_init_mut()
    };

    let handles = state.midi_handles.clone();
    for dev in handles {
        let res = dev.poll();

        // dev.send(144, 1,  state.counter as u8 % 127);

        if res.is_empty() {
            continue;
        }

        match dev.get_meta().device_id {
            1 => midimix(dev, res, state),
            2 => nano(dev, res, state),
            3 => apc(dev, res, state, input.clone()),
            _ => {}
        };
    }

    for ev in &input.events.events {
        println!("---> EVENT: {ev:?}");
    }

    //     match ev {
    //         ControlEvent::SelectGroup(42) => {
    //             state.midi_handle.send(151, 4, 127);
    //             state.enabled = true;
    //         }
    //         ControlEvent::DeSelectGroup(42) => {
    //             state.midi_handle.send(151, 4, 0);
    //             state.enabled = false;
    //         }
    //         _ => {}
    //     }
    // }

    // let res = state.midi_handle.poll();
    // let speed = 0.1;

    // if !res.is_empty() {
    //     // println!("res {res:?}");
    //     for sig in res {
    //         if sig.status == 151 && sig.kind == 4 && sig.value == 127 {
    //             // state.midi_handle.send(151, 4, state.enabled as u8 * 127);
    //             state.enabled = !state.enabled;
    //             bl_send(if state.enabled {
    //                 ControlEvent::SelectGroup(42)
    //             } else {
    //                 ControlEvent::DeSelectGroup(42)
    //             });
    //         }

    //         if sig.status == 176 && sig.kind == 34 {
    //             if sig.value == 65 {
    //                 state.counter += speed;
    //             } else if sig.value == 63 {
    //                 state.counter -= speed;
    //             }

    //             println!("counter: {}", state.counter as u32);
    //         }
    //     }
    // }

    // state.midi_handle.send(176, 90, state.counter as u8 % 127);
}
