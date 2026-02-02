use blaulicht_plugin_framework::MidiEvent;
use blaulicht_plugin_framework::{self as bpf, println, MidiConnection};
use blaulicht_shared::{AnimationSpeedModifier, ControlEvent, EngineState, TickInput};
use map_range::MapRange;

//
// State.
//

const COUNT_FADERS: usize = 2;

const FADER_BYTES: [u8; 2] = [176, 177];
const KNOB_BYTES: [u8; 2] = [23, 24];

pub struct DDJSubSystem {
    midi_handle: MidiConnection,
    last_sync: u32,

    fader_vals: [u8; COUNT_FADERS],
    fader_vals_updated: [bool; COUNT_FADERS],

    knob_vals: [u8; COUNT_FADERS],
    knob_vals_updated: [bool; COUNT_FADERS],

    scenes: Vec<u8>,

    dmx: EngineState,
}

impl Default for DDJSubSystem {
    fn default() -> Self {
        Self {
            midi_handle: unsafe { MidiConnection::dummy() },
            last_sync: 0,

            fader_vals: [0; COUNT_FADERS],
            fader_vals_updated: [false; COUNT_FADERS],

            knob_vals: [0; COUNT_FADERS],
            knob_vals_updated: [false; COUNT_FADERS],

            scenes: vec![],
            dmx: EngineState::default(),
        }
    }
}

//
// Begin public impls.
//

impl DDJSubSystem {
    pub fn init(&mut self) {
        println!("[DDJ_200] initializing...");

        let name = "DDJ-200";
        let midi_handle = MidiConnection::open(name).unwrap();
        println!(
            "Got MIDI handle to device! HANDLE ID: {}",
            midi_handle.get_meta().device_id
        );

        self.midi_handle = midi_handle;

        println!("[DDJ_200] done.");
    }

    pub fn run(&mut self, input: TickInput) {
        self.sync(input.clock);

        let res = self.midi_handle.poll();
        self.midi_in(res);

        for i in 0..COUNT_FADERS {
            self.handle_fader_input(i);
            self.handle_knob_input(i);
        }
    }
}

//
// Begin private impls.
//

impl DDJSubSystem {
    fn midi_in(&mut self, ev: Vec<MidiEvent>) {
        // self.midi_handle.send(0x90, 46, 127);

        // for i in 0..255 {
        //     self.midi_handle.send(0xC1, i, 127);
        // }

        // conn.send(0x91, 102, 127);

        // for i in 0..2 {
        //     for j in 0..10 {/s
        //         conn.send(0x91 + i, j, 127);
        //     }
        // }

        for e in ev {
            // println!("KORG EVENT: {:?}", e);

            match (e.status, e.kind, e.value) {
                // Faders
                (fader_byte, 19, value) if FADER_BYTES.contains(&fader_byte) => {
                    let index = FADER_BYTES.iter().position(|b| *b == fader_byte).unwrap();
                    self.fader_vals[index] = value;
                    self.fader_vals_updated[index] = true;
                }
                (182, knob_byte, value) if KNOB_BYTES.contains(&knob_byte) => {
                    let index = KNOB_BYTES.iter().position(|b| *b == knob_byte).unwrap();
                    self.knob_vals[index] = value;
                    self.knob_vals_updated[index] = true;
                }
                _ => {
                    // println!("{}: {:?}", self.midi_handle.get_meta().device_id, e);
                }
            }
        }
    }

    fn sync(&mut self, current_time: u32) {
        if current_time - self.last_sync > 100 {
            self.last_sync = current_time;

            // Sync state.
            let dmx = bpf::get_dmx();
            self.dmx = dmx.clone();

            let mut scenes = vec![dmx.current_scene_focus];
            scenes.extend_from_slice(&dmx.current_overlay_scenes);

            self.scenes = scenes;
        }
    }

    fn handle_fader_input(&mut self, fader: usize) {
        if self.fader_vals_updated[fader] {
            self.fader_vals_updated[fader] = false;

            let alpha = self.fader_vals[fader];
            let alpha = (alpha as u16).map_range(0..127, 0..100) as u8;

            let Some(scene_id) = self.scenes.get(fader) else {
                return;
            };

            bpf::send_event(ControlEvent::SetSceneMasterAlpha(*scene_id, alpha));
        }
    }

    fn handle_knob_input(&mut self, knob: usize) {
        if self.knob_vals_updated[knob] {
            self.knob_vals_updated[knob] = false;
            let knob_val = self.knob_vals[knob];
            let max_index = AnimationSpeedModifier::ALL.len() as u16 - 1;
            let index = (knob_val as u16).map_range(0..127, 0..max_index);

            let Some(scene_id) = self.scenes.get(knob) else {
                return;
            };

            let speed = AnimationSpeedModifier::from_index(index as usize);

            bpf::send_event(ControlEvent::SetSceneMasterSpeed(*scene_id, speed));
        }
    }
}
