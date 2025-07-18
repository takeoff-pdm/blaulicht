use std::mem::MaybeUninit;

use crate::{
    blaulicht::{bl_send, prelude::println},
    midi::{MidiConnection, MidiEvent},
};
use blaulicht_shared::{ControlEvent, ControlEventCollection, ControlEventMessage, TickInput};
use map_range::MapRange;

struct State {
    counter: f32,
    midi_handles: Vec<MidiConnection>,
    enabled: bool,
    last_update: u32,

    // Selection state.
    groups: Vec<bool>,
    brightness_mod: u8,
}

static mut STATE: MaybeUninit<State> = MaybeUninit::uninit();

pub fn initialize(input: TickInput) {
    println!("Initializing...");

    let devices = vec![
        "MIDI Mix",
        "nanoKONTROL Studio CTRL",
        "APC mini mk2 Control",
        // "Minilab3 MIDI"
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
            groups: vec![false; 8],
            brightness_mod: 0,
        });
    }
}

fn midimix(conn: MidiConnection, ev: Vec<MidiEvent>, state: &mut State) {
    for e in ev {
        match (e.status, e.kind, e.value) {
            (176, 19, v) => {
                bl_send(ControlEvent::SetBrightness(
                    (v as u16).map_range(0..127, 0..255) as u8,
                ));
            }
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

fn nano_in(conn: MidiConnection, ev: Vec<MidiEvent>, state: &mut State) {
    // conn.send(0x90, 24, 127);
    // for i in 0..255 {
    //     conn.send(0xC1, i, 127);
    // }

    // conn.send(0x91, 102, 127);

    // for i in 0..2 {
    //     for j in 0..10 {
    //         conn.send(0x91 + i, j, 127);
    //     }
    // }

    const SELECT_BUTTON_STARTER: u8 = 24;
    const COUNT_SELECT_BUTTONS: u8 = 8;

    for e in ev {
        match (e.status, e.kind, e.value) {
            // Group Select.
            (144, key, 127)
                if key >= SELECT_BUTTON_STARTER
                    && key <= SELECT_BUTTON_STARTER + COUNT_SELECT_BUTTONS =>
            {
                let g_idx = key - SELECT_BUTTON_STARTER;
                let old = state.groups[g_idx as usize];
                state.groups[g_idx as usize] = !old;

                let msg = match !old {
                    true => ControlEvent::DeSelectGroup(g_idx),
                    false => ControlEvent::SelectGroup(g_idx),
                };

                bl_send(msg);
            }
            // Set button on the left.
            (144, 82, 127) => {}
            (176, 60, value) => {
                if value >= 60 {
                    if state.brightness_mod > 0 {
                        state.brightness_mod -= 1;
                    }
                } else if state.brightness_mod < 255 {
                    state.brightness_mod += 1;
                }

                bl_send(ControlEvent::SetBrightness(state.brightness_mod));
            }
            _ => {
                println!("{}: {:?}", conn.get_meta().device_id, e);
            }
        }
    }
}

fn nano_out(
    conn: MidiConnection,
    // ev: Vec<MidiEvent>,
    control_events: &[ControlEventMessage],
    state: &mut State,
) {
    for ev in control_events {
        match ev.body() {
            ControlEvent::SelectGroup(g_index) => {
                println!("a");
                conn.send(0x90, g_index, 127);
            }
            ControlEvent::DeSelectGroup(g_index) => {
                println!("b");
                conn.send(0x90, g_index, 1);
            }
            _ => {} // ControlEvent::LimitSelectionToFixtureInCurrentGroup(_) => todo!(),
                    // ControlEvent::UnLimitSelectionToFixtureInCurrentGroup(_) => todo!(),
                    // ControlEvent::RemoveSelection => todo!(),
                    // ControlEvent::SetEnabled(_) => todo!(),
                    // ControlEvent::SetBrightness(_) => todo!(),
                    // ControlEvent::SetColor(_) => todo!(),
                    // ControlEvent::MiscEvent { descriptor, value } => todo!(),
        }
    }
}

fn apc(conn: MidiConnection, ev: Vec<MidiEvent>, state: &mut State, input: TickInput) {
    if ev.is_empty() {
        return;
    }

    println!("apc");
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

        match dev.get_meta().device_id {
            1 => midimix(dev, res, state),
            2 => {
                nano_in(dev, res, state);
                nano_out(dev, &input.events.events, state);
            }
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
