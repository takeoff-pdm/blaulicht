use blaulicht_plugin_framework::MidiEvent;
use blaulicht_plugin_framework::{self as bpf, println, MidiConnection};
use blaulicht_shared::{
    AnimationSpeedModifier, ControlEvent, ControlEventMessage, EngineState, TickInput,
};
use map_range::MapRange;
use std::collections::HashSet;

const SELECT_BUTTON_STARTER: u8 = 46;
const COUNT_SELECT_BUTTONS: u8 = 8;

const FADER_BYTES: [u8; COUNT_SELECT_BUTTONS as usize] = [2, 3, 4, 5, 6, 8, 9, 12];

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
    knob_vals: [u8; COUNT_SELECT_BUTTONS as usize],

    scenes: Vec<u8>,

    dmx: EngineState,
}

impl Default for KorgSubSystem {
    fn default() -> Self {
        Self {
            last_state_render: 0,
            selection_mode: true,
            midi_handle: unsafe { MidiConnection::dummy() },
            active_groups: HashSet::new(),
            last_sync: 0,
            fader_vals: [0; COUNT_SELECT_BUTTONS as usize],
            knob_vals: [0; COUNT_SELECT_BUTTONS as usize],
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
        println!("[KORG] initializing...");

        let name = "nanoKONTROL Studio";
        let midi_handle = MidiConnection::open(&name).unwrap();
        println!(
            "Got MIDI handle to device! HANDLE ID: {}",
            midi_handle.get_meta().device_id
        );

        self.midi_handle = midi_handle;

        self.nano_init();
        println!("[KORG] done.");
    }

    pub fn run(&mut self, input: TickInput) {
        self.sync(input.clock);

        let res = self.midi_handle.poll();
        self.nano_in(res);
        self.nano_out(&input.events.events);

        for ev in &input.events.events {
            println!("---> KORG EVENT: {ev:?}");
        }
    }
}

//
// Begin private impls.
//

impl KorgSubSystem {
    fn nano_in(&mut self, ev: Vec<MidiEvent>) {
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
            println!("KORG EVENT: {:?}", e);

            match (e.status, e.kind, e.value) {
                // Toggle operating mode:
                (176, 80, 127) => {
                    self.selection_mode = false;
                    self.nano_blackout_groups();
                    self.nano_render_mode();
                }
                (176, 81, 127) => {
                    self.selection_mode = true;
                    self.nano_blackout_groups();
                    self.nano_render_mode();
                }
                // Group Select.
                (144, key, 127)
                    if key >= SELECT_BUTTON_STARTER
                        && key <= SELECT_BUTTON_STARTER + COUNT_SELECT_BUTTONS
                        && self.selection_mode =>
                {
                    let g_idx = key - SELECT_BUTTON_STARTER;

                    let dmx = bpf::get_dmx();
                    let old = dmx.selection.group_ids.contains(&g_idx);

                    let msg = match old {
                        true => ControlEvent::DeSelectGroup(g_idx),
                        false => ControlEvent::SelectGroup(g_idx),
                    };

                    bpf::send_event(msg);
                }
                // Faders
                (176, fader_byte, value) if FADER_BYTES.contains(&fader_byte) => {
                    let index = FADER_BYTES.iter().position(|b| *b == fader_byte).unwrap();
                    self.fader_vals[index] = value;
                    println!("Fader values: {:?}", self.fader_vals);

                    if self.selection_mode {
                        println!("WARN: not in selection_mode");
                        return;
                    }

                    self.handle_fader_input(index);
                }
                (176, knob_byte, value) if knob_byte >= 13 && knob_byte <= 20 => {
                    let index = knob_byte as usize - 13;
                    self.knob_vals[index] = value;
                    println!("Knob values: {:?}", self.knob_vals);

                    // Set speed
                    if self.selection_mode {
                        println!("WARN: not in selection_mode");
                        return;
                    }

                    self.handle_knob_input(index);
                }
                // Set button on the left.
                (144, 82, 127) => {}
                (176, 60, value) => {
                    // if value >= 60 {
                    //     if state.brightness_mod > 0 {
                    //         state.brightness_mod -= 1;
                    //     }
                    // } else if state.brightness_mod < 255 {
                    //     state.brightness_mod += 1;
                    // }

                    // bpf::send_event(ControlEvent::SetAlpha(state.brightness_mod));
                }
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

            self.nano_render_mode();
            self.nano_render_scenes();
            self.nano_render_groups();
        }
    }

    fn nano_render_mode(&mut self) {
        // for i in 0..127 {
        //     self.midi_handle.send(0x90, i, 127);
        // }
        // for i in 0..127 {
        //     self.midi_handle.send(0xB0, i, 0);
        // }

        // for i in 80..82 {
        //     self.midi_handle
        //         .send(0xB0, i, self.selection_mode as u8 * 127);
        // }

        self.midi_handle
            .send(0xB0, 81, self.selection_mode as u8 * 127);

        self.midi_handle
            .send(0xB0, 80, !self.selection_mode as u8 * 127);
    }

    fn nano_blackout_groups(&mut self) {
        for g_index in 0..8 {
            self.midi_handle
                .send(0x90, g_index + SELECT_BUTTON_STARTER, 0);
        }
    }

    fn nano_render_groups(&mut self) {
        if !self.selection_mode {
            return;
        }

        for g_index in 0..8 {
            if self.active_groups.contains(&g_index) {
                self.midi_handle
                    .send(0x90, g_index + SELECT_BUTTON_STARTER, 127);
            } else {
                self.midi_handle
                    .send(0x90, g_index + SELECT_BUTTON_STARTER, 0);
            }
        }
    }

    fn nano_render_scenes(&mut self) {
        if self.selection_mode {
            return;
        }

        for g_index in 0..8 {
            let scene_id = self.scenes.get(g_index);
            let is_active = match scene_id {
                Some(scene_id) => {
                    if let Some(scene) = self.dmx.scenes.get(scene_id) {
                        scene.sink.master_alpha_fader > 0
                    } else {
                        false
                    }
                }
                None => false,
            };

            self.midi_handle.send(
                0x90,
                g_index as u8 + SELECT_BUTTON_STARTER,
                is_active as u8 * 127,
            );
        }
    }

    fn nano_init(&mut self) {
        self.nano_render_groups();
    }

    fn nano_out(&mut self, control_events: &[ControlEventMessage]) {
        for ev in control_events {
            if self.selection_mode {
                match ev.body() {
                    ControlEvent::SelectGroup(g_idx) => {
                        self.active_groups.insert(g_idx);
                        self.nano_render_groups();
                    }
                    ControlEvent::DeSelectGroup(g_idx) => {
                        self.active_groups.remove(&g_idx);
                        self.nano_render_groups();
                    }
                    _ => {} // ControlEvent::LimitSelectionToFixtureInCurrentGroup(_) => todo!(),
                            // ControlEvent::UnLimitSelectionToFixtureInCurrentGroup(_) => todo!(),
                            // ControlEvent::RemoveSelection => todo!(),
                            // ControlEvent::SetEnabled(_) => todo!(),
                            // ControlEvent::SetBrightness(_) => todo!(),
                            // ControlEvent::SetColor(_) => todo!(),
                            // ControlEvent::MiscEvent { descriptor, value } => todo!(),
                }
            } else {
            }
        }
    }

    fn handle_fader_input(&mut self, fader: usize) {
        let alpha = self.fader_vals[fader];
        let alpha = (alpha as u16).map_range(0..127, 0..100) as u8;
        println!("ALPHA: {alpha}");
        let Some(scene_id) = self.scenes.get(fader) else {
            return;
        };

        bpf::send_event(ControlEvent::SetSceneMasterAlpha(*scene_id, alpha));
        // self.nano_render_scenes();
    }

    fn handle_knob_input(&mut self, knob: usize) {
        let knob_val = self.knob_vals[knob];
        let max_index = AnimationSpeedModifier::ALL.len() as u16 - 1;
        let index = (knob_val as u16).map_range(0..127, 0..max_index) as usize;

        let Some(scene_id) = self.scenes.get(knob) else {
            return;
        };

        let speed = AnimationSpeedModifier::from_index(index);

        bpf::send_event(ControlEvent::SetSceneMasterSpeed(*scene_id, speed));
        // self.nano_render_scenes();
    }
}
