use blaulicht_plugin_framework::MidiEvent;
use blaulicht_plugin_framework::{self as bpf, println, MidiConnection};
use blaulicht_shared::{AnimationSpeedModifier, ControlEvent, EngineState, TickInput};
use map_range::MapRange;
use std::collections::HashSet;

const COUNT_SELECT_BUTTONS: u8 = 2;
const FADER_BYTES: [u8; COUNT_SELECT_BUTTONS as usize] = [176, 177];

//
// Korg State.
//

pub struct KorgSubSystem {
    last_state_render: u32,

    selection_mode: bool,

    midi_handle: MidiConnection,
    active_groups: HashSet<u8>,
    last_sync: u32,

    fader_vals: [u8; COUNT_SELECT_BUTTONS as usize],
    fader_vals_updated: [bool; COUNT_SELECT_BUTTONS as usize],

    knob_vals: [u8; COUNT_SELECT_BUTTONS as usize],
    knob_vals_updated: [bool; COUNT_SELECT_BUTTONS as usize],

    scenes: Vec<u8>,

    dmx: EngineState,
}

impl Default for KorgSubSystem {
    fn default() -> Self {
        Self {
            last_state_render: 0,
            selection_mode: false,
            midi_handle: unsafe { MidiConnection::dummy() },
            active_groups: HashSet::new(),
            last_sync: 0,
            fader_vals: [0; COUNT_SELECT_BUTTONS as usize],
            fader_vals_updated: [false; COUNT_SELECT_BUTTONS as usize],
            knob_vals: [0; COUNT_SELECT_BUTTONS as usize],
            knob_vals_updated: [false; COUNT_SELECT_BUTTONS as usize],
            scenes: vec![],
            dmx: EngineState::default(),
        }
    }
}

//
// Begin public impls.
//

impl KorgSubSystem {
    pub fn init(&mut self) {
        println!("[DDJ-200] initializing...");

        let name = "DDJ-200";
        let midi_handle = MidiConnection::open(&name).unwrap();
        println!(
            "Got MIDI handle to device! HANDLE ID: {}",
            midi_handle.get_meta().device_id
        );

        self.midi_handle = midi_handle;

        println!("[KORG] done.");
    }

    pub fn run(&mut self, input: TickInput) {
        self.sync(input.clock);

        let res = self.midi_handle.poll();
        self.nano_in(res);

        // for ev in &input.events.events {
        //     println!("---> KORG EVENT: {ev:?}");
        // }

        for i in 0..FADER_BYTES.len() {
            self.handle_fader_input(i);
            self.handle_knob_input(i);
        }
    }
}

//
// Begin private impls.
//

impl KorgSubSystem {
    fn nano_in(&mut self, ev: Vec<MidiEvent>) {
        for e in ev {
            // println!("KORG EVENT: {:?}", e);

            match (e.status, e.kind, e.value) {
                // Faders
                (fader_byte, 33, value) if FADER_BYTES.contains(&fader_byte) => {
                    let modifier: i16 = match value {
                        63 => -1,
                        65 => 1,
                        _ => 0,
                    };

                    let index = FADER_BYTES.iter().position(|b| *b == fader_byte).unwrap();
                    self.fader_vals[index] =
                        (((self.fader_vals[index] as i16) + modifier).clamp(0, 127) as u8);
                    self.fader_vals_updated[index] = true;
                }
                (fader_byte, 19, value) if FADER_BYTES.contains(&fader_byte) => {
                    let index = FADER_BYTES.iter().position(|b| *b == fader_byte).unwrap();
                    self.fader_vals[index] = value;
                    self.fader_vals_updated[index] = true;

                    if self.selection_mode {
                        println!("WARN: not in selection_mode");
                        return;
                    }
                }
                (182, knob_byte, value) if knob_byte == 23 || knob_byte == 24 => {
                    let index = knob_byte as usize - 23;
                    self.knob_vals[index] = value;
                    self.knob_vals_updated[index] = true;

                    // Set speed
                    if self.selection_mode {
                        println!("WARN: not in selection_mode");
                        return;
                    }
                }
                (176 | 177, 51, _) => {}
                _ => {
                    println!("{}: {:?}", self.midi_handle.get_meta().device_id, e);
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

            // println!("SYNC STATE: {dmx:?}");
            self.active_groups = dmx.selection.group_ids.clone();

            let dmx = bpf::get_dmx();
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

            // println!("ALPHA: {alpha}");
            let Some(scene_id) = self.scenes.get(fader) else {
                return;
            };

            // println!("Fader values: {:?}", self.fader_vals);

            bpf::send_event(ControlEvent::SetSceneMasterAlpha(*scene_id, alpha));
        }
    }

    fn handle_knob_input(&mut self, knob: usize) {
        if self.knob_vals_updated[knob] {
            self.knob_vals_updated[knob] = false;

            let knob_val = self.knob_vals[knob];
            let max_index = AnimationSpeedModifier::ALL.len() as u16 - 1;
            let index = (knob_val as u16).map_range(0..127, 0..max_index) as usize;

            let Some(scene_id) = self.scenes.get(knob) else {
                return;
            };

            let speed = AnimationSpeedModifier::from_index(index);

            // println!("Knob values: {:?}", self.knob_vals);

            bpf::send_event(ControlEvent::SetSceneMasterSpeed(*scene_id, speed));
            // self.nano_render_scenes();
        }
    }
}
