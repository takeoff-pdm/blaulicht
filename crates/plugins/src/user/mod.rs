use std::{fmt::Display, mem::MaybeUninit};

use crate::{
    blaulicht::{bl_send, bl_sys, bl_udp, prelude::println},
    midi::{MidiConnection, MidiEvent},
    state::get_dmx,
};
use blaulicht_shared::{hsv_to_rgb, ControlEvent, ControlEventMessage, TickInput};
use map_range::MapRange;

//
// MIDI start.
//

#[derive(Clone, Copy)]
enum MidiDevice {
    NanoKontrol,
    MidiMix,
    APCMini,
}

impl Display for MidiDevice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                MidiDevice::NanoKontrol => "nanoKONTROL Studio",
                MidiDevice::MidiMix => "MIDI Mix",
                MidiDevice::APCMini => "APC mini mk2",
            }
        )
    }
}

impl MidiDevice {
    fn index(&self) -> usize {
        match self {
            MidiDevice::NanoKontrol => 0,
            MidiDevice::MidiMix => 1,
            MidiDevice::APCMini => 2,
        }
    }
}

//
// MIDI end.
//

struct State {
    counter: f32,
    midi_handles: Vec<(MidiDevice, MidiConnection)>,
    // ids_to_midi_types: HashMap<MidiDevice, usize>,
    enabled: bool,
    last_update: u32,

    // Selection state.
    groups: Vec<bool>,
    brightness_mod: u8,

    hsv: (u16, f32, f32),

    last_scene: u8,
    last_video: usize,
    video: usize,
    is_apc_init: bool,
    activate_wall: u8,
    disable_fog: bool,
    enable_fog_time: u32,
    strobe_enable_time: u32,
    strobe_auto_enable: bool,
    arm_drop: bool,
    arm_drop_timer: u32,
    // strobe_enabled: bool,
}

static mut STATE: MaybeUninit<State> = MaybeUninit::uninit();

fn set_video(vid: &str) {
    let body_str = vid.as_bytes();
    let mut body = Vec::with_capacity(body_str.len() + 1);
    body.push(100);
    body.extend_from_slice(body_str);
    // TODO: make IP configurable.
    bl_udp("192.168.0.102:1714", &body);
}

pub fn set_rotate(value: u8) {
    let val = (value as u16).map_range(0..127, 0..360);
    let val_bytes = val.to_le_bytes();
    let mut buf = Vec::with_capacity(1 + val_bytes.len());
    buf.push(120);
    buf.extend_from_slice(&val_bytes);
    bl_udp("192.168.0.102:1714", &buf);
}

// pub fn set_vid_speed(value: u8) {
//     bl_udp("127.0.0.1:9000", &[130, val]);
// }

pub fn set_brightness_internal(val: u8) {
    bl_udp("192.168.0.102:1714", &[130, val]);
}

pub fn initialize(input: TickInput) {
    println!("Initializing...");

    // // Get state dump.
    let state = get_dmx();
    println!("STATE: {state:?}");

    set_video("a to x ng.mp4");
    set_rotate(0);

    //
    // return;

    let mut devices = vec![
        MidiDevice::NanoKontrol,
        MidiDevice::MidiMix,
        MidiDevice::APCMini,
    ];
    devices.sort_by_key(|a| a.index());

    let mut midi_handles = Vec::with_capacity(devices.len());
    // let mut ids_to_midi_types = HashMap::new();

    for dev in devices {
        let name = dev.to_string();
        let midi_handle = MidiConnection::open(&name).unwrap();
        println!(
            "Got MIDI handle to device! HANDLE ID: {}",
            midi_handle.get_meta().device_id
        );

        midi_handles.push((dev, midi_handle));
    }

    unsafe {
        #[allow(static_mut_refs)]
        STATE.write(State {
            hsv: (127, 127.0, 127.0),
            counter: 0.0,
            midi_handles,
            // ids_to_midi_types,
            enabled: false,
            last_update: 0,
            groups: vec![false; 8],
            brightness_mod: 0,
            last_scene: 0,
            video: 0,
            last_video: 0,
            is_apc_init: true,
            activate_wall: 0,
            disable_fog: false,
            enable_fog_time: 0,
            strobe_enable_time: 0,
            strobe_auto_enable: false,
            arm_drop: false,
            arm_drop_timer: 0,
        });
    }

    // Initialize fans in the end.
    bl_sys("sudo fans on");
}

fn midimix(conn: MidiConnection, ev: Vec<MidiEvent>, state: &mut State) {
    for e in ev {
        match (e.status, e.kind, e.value) {
            (176, 19, v) => {
                bl_send(ControlEvent::SetAlpha(
                    (v as u16).map_range(0..127, 0..255) as u8
                ));
            }
            (144, 1, 127) => {
                state.enabled = !state.enabled;
                conn.send(144, 1, state.counter as u8 % 127);
                state.counter += 1.0;
            }
            (176, 16, value) => {
                let val = (value as u16).map_range(0..127, 0..360);
                bl_send(ControlEvent::SetColorHue(val));
            }
            (176, 17, value) => {
                let val = (value as u16).map_range(0..127, 0..255);
                bl_send(ControlEvent::SetColorSaturation(val as u8));
            }
            (176, 18, value) => {
                let val = (value as u16).map_range(0..127, 0..255);
                bl_send(ControlEvent::SetColorValue(val as u8));
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

                let msg = match old {
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

                bl_send(ControlEvent::SetAlpha(state.brightness_mod));
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
    const SCENES: [u8; 8] = [56, 48, 40, 32, 24, 16, 8, 0];
    const VIDEOS: [u8; 8] = [63, 55, 47, 39, 31, 23, 15, 7];

    const SCENES_FIXED: [u8; 5] = [60, 52, 44, 36, 28];
    const SCENE_MAPPING_FIXED: [u8; 5] = [4, 3, 0, 5, 2];

    if state.is_apc_init {
        for i in 0..64 {
            conn.send(0x96, i as u8, 0);
        }

        state.is_apc_init = false;
    }

    // if ev.is_empty() {
    //     return;
    // }

    // if input.clock - state.last_update > 0 {
    //     // for i in 0..64 {
    //     conn.send(0x96, 56 as u8, (state.counter as usize % 127) as u8);
    //     // }
    //     state.counter += 1.0;
    //     state.last_update = input.clock;
    // }

    // if input.clock - state.last_update > 0 {
    //     // for i in 0..64 {
    //     for s in SCENES {
    //         conn.send(0x96, s as u8, 0);
    //     }
    //     conn.send(0x96, 56 as u8, 100);
    //     // }
    //     state.counter += 1.0;
    //     state.last_update = input.clock;
    // }

    let scene = get_dmx().current_scene_focus;
    if scene != state.last_scene {
        // for i in 0..64 {
        for s in SCENES {
            conn.send(0x96, s, 0);
        }

        conn.send(0x96, SCENES[scene as usize], 10);

        for s in SCENES_FIXED {
            conn.send(0x96, s, 0);
        }

        if let Some(rev_mapped) = SCENE_MAPPING_FIXED.iter().position(|e| *e == scene) {
            conn.send(0x96, SCENES_FIXED[rev_mapped], 20);
        }

        // }
        // state.counter += 1.0;
        // state.last_update = input.clock;
        println!("sync scene");

        state.last_scene = scene;
    }

    if state.video != state.last_video {
        // for i in 0..64 {
        for s in VIDEOS {
            conn.send(0x96, s, 0);
        }

        conn.send(0x96, VIDEOS[state.video], 10);

        // }
        // state.counter += 1.0;
        // state.last_update = input.clock;
        println!("sync vieo");

        state.last_video = state.video;
    }

    for e in ev {
        match (e.status, e.kind, e.value) {
            (144, 106, 127) => {
                bl_send(ControlEvent::Transaction(vec![
                    ControlEvent::PushSelection,
                    ControlEvent::SelectGroup(2),
                    ControlEvent::SetAlpha(255),
                    ControlEvent::PopSelection,
                ]));
                state.disable_fog = true;
                state.enable_fog_time = input.clock;
            }
            (144, 107, 127) => {
                state.activate_wall = 2;
            }
            (176, 55, val) => {
                println!("val");
                set_brightness_internal(val);
            }
            (144, scene, 127) if SCENES_FIXED.contains(&scene) => {
                let normal_index = SCENES_FIXED.iter().position(|v| *v == scene).unwrap();
                let mapped_index = SCENE_MAPPING_FIXED[normal_index];

                let dmx = get_dmx();

                if !dmx.scenes.contains_key(&mapped_index) {
                    println!("E: no such scene");
                    return;
                }

                bl_send(ControlEvent::SetSceneFocus(mapped_index));
            }
            (144, scene, 127) if SCENES.contains(&scene) => {
                let index = SCENES.iter().position(|v| *v == scene).unwrap() as u8;

                let dmx = get_dmx();

                if !dmx.scenes.contains_key(&index) {
                    println!("E: no such scene");
                    return;
                }

                bl_send(ControlEvent::SetSceneFocus(index));
            }
            (144, video, 127) if VIDEOS.contains(&video) => {
                let index = VIDEOS.iter().position(|v| *v == video).unwrap();

                const VIDEO_SRCS: [&str; 4] =
                    ["a to x ng.mp4", "cheese.webm", "grr.webm", "swim.webm"];

                // let dmx = get_dmx();
                //
                // if !dmx.scenes.contains_key(&index) {
                //     println!("E: no such scene");
                //     return;
                // }

                if let Some(s) = VIDEO_SRCS.get(index) {
                    set_video(s);

                    state.video = index;
                }
                // bl_send(ControlEvent::SetSceneFocus(index));
            }
            _ => {
                println!("{}: {:?}", conn.get_meta().device_id, e);
            }
        }
    }
}

const STROBE_SCENE: u8 = 2;
//
// fn enable_strobe() {
//     bl_send(ControlEvent::SetSceneFocus(STROBE_SCENE));
// }

pub fn run(input: TickInput) {
    let state = unsafe {
        #[allow(static_mut_refs)]
        STATE.assume_init_mut()
    };

    let dmx = get_dmx();

    if input.audio_data.bass_avg < 70 && !state.arm_drop {
        println!("ARMED DROP");
        state.arm_drop = true;
        state.arm_drop_timer = input.clock;
    }

    if state.arm_drop {
        // if input.clock - state.arm_drop_timer > 1000 {
        //     match dmx.current_scene_focus == 3 {
        //         true => {
        //             bl_send(ControlEvent::SetSceneFocus(5)); // HYPE UP
        //         }
        //         false => {
        //             bl_send(ControlEvent::SetSceneFocus(3)); // HYPE UP
        //         }
        //     };
        //
        //     state.arm_drop_timer = input.clock;
        // }
    }

    // Drop detection and so on.
    // match input.audio_data.
    if state.arm_drop
        // && input.audio_data.bpm != 0
        && input.audio_data.bass_avg_short > 200
    && dmx.current_scene_focus != STROBE_SCENE
    && !state.strobe_auto_enable
    {
        state.strobe_enable_time = input.clock;
        state.strobe_auto_enable = true;
        bl_send(ControlEvent::SetSceneFocus(STROBE_SCENE));
        println!("enable strobe");
    } else {
        // println!(
        //     "{} {}",
        //     input.audio_data.bpm, input.audio_data.bass_avg_short
        // );
    }

    if input.clock - state.strobe_enable_time > 3500 && state.strobe_auto_enable {
        bl_send(ControlEvent::SetSceneFocus(0));
        println!("disable strobe");
        state.strobe_auto_enable = false;
        state.arm_drop = false;
    }

    // if dmx.current_scene_focus != STROBE_SCENE {
    //     state.strobe_auto_enable = false;
    // }

    // println!("{:?}", input.audio_data);

    if state.disable_fog && input.clock - state.enable_fog_time > 2000 {
        bl_send(ControlEvent::Transaction(vec![
            ControlEvent::PushSelection,
            ControlEvent::SelectGroup(2),
            ControlEvent::SetAlpha(0),
            ControlEvent::PopSelection,
        ]));
        state.disable_fog = false;
    }

    if state.activate_wall == 1 {
        set_brightness_internal(0);
        state.activate_wall = 0;
    }

    if state.activate_wall == 2 {
        set_brightness_internal(100);
        state.activate_wall = 1;
    }

    // if input.clock > 60000 {
    //     let dmx = get_dmx();
    //
    //     println!("{dmx:?}");
    //     panic!("");
    // }

    // return;

    let handles = state.midi_handles.clone();
    for (dev, handle) in handles {
        let res = handle.poll();

        match dev {
            MidiDevice::MidiMix => midimix(handle, res, state),
            MidiDevice::NanoKontrol => {
                nano_in(handle, res, state);
                nano_out(handle, &input.events.events, state);
            }
            MidiDevice::APCMini => apc(handle, res, state, input.clone()),
        }
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
