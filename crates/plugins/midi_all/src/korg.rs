use blaulicht_plugin_framework::MidiEvent;
use blaulicht_plugin_framework::{self as bpf, println};

use crate::legacy::virtual_midi::VirtualMidi;
use blaulicht_shared::{
    AnimationSpeedModifier, ControlEvent, ControlEventMessage, EngineState, MainUiEvent,
    TickInput,
};
use map_range::MapRange;
use std::collections::HashSet;

pub const SELECT_BUTTON_STARTER: u8 = 46;
const COUNT_SELECT_BUTTONS: u8 = 8;
const KORG_DEVICE_NAME: &str = "nanoKONTROL Studio";

pub const FADER_BYTES: [u8; COUNT_SELECT_BUTTONS as usize] = [2, 3, 4, 5, 6, 8, 9, 12];

/// Default CC number of the nanoKONTROL Studio's big jog wheel. In the
/// factory Inc/Dec setup clockwise sends 1 and counter-clockwise sends 65.
pub const JOG_WHEEL_CC: u8 = 82;
/// CC used by older/custom Korg Kontrol Editor scenes.
const LEGACY_JOG_WHEEL_CC: u8 = 60;

fn jog_wheel_delta(cc: u8, value: u8) -> i32 {
    if cc == JOG_WHEEL_CC {
        if value < 64 {
            value as i32
        } else {
            -(value as i32 - 64)
        }
    } else if value < 64 {
        value as i32
    } else {
        value as i32 - 128
    }
}

//
// Korg State.
//

pub struct KorgSubSystem {
    selection_mode: bool,
    /// Hardware handle plus LED/control shadow; runs virtual-only when the
    /// nanoKONTROL is not attached (driven through the on-screen twin).
    pub(crate) midi: VirtualMidi,
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
            selection_mode: true,
            midi: VirtualMidi::disconnected(KORG_DEVICE_NAME),
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
        println!("[KORG] initializing...");

        self.midi = VirtualMidi::open(KORG_DEVICE_NAME);
        self.nano_init();
        println!("[KORG] done.");
    }

    /// `mapped` is the user-mapping hook: events it consumes skip the
    /// hardcoded handling below.
    pub fn run(&mut self, input: TickInput, mapped: impl FnMut(&MidiEvent) -> bool) {
        self.sync(input.clock);

        let res = self.midi.poll(input.clock);
        self.nano_in(res, mapped);
        self.nano_out(&input.events.events);

        for i in 0..COUNT_SELECT_BUTTONS as usize {
            self.handle_fader_input(i);
            self.handle_knob_input(i);
        }
    }
}

//
// Begin private impls.
//

impl KorgSubSystem {
    fn nano_in(&mut self, ev: Vec<MidiEvent>, mut mapped: impl FnMut(&MidiEvent) -> bool) {
        // self.midi.send(0x90, 46, 127);

        // for i in 0..255 {
        //     self.midi.send(0xC1, i, 127);
        // }

        // conn.send(0x91, 102, 127);

        // for i in 0..2 {
        //     for j in 0..10 {/s
        //         conn.send(0x91 + i, j, 127);
        //     }
        // }

        for e in ev {
            // println!("KORG EVENT: {:?}", e);
            if mapped(&e) {
                continue;
            }

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
                    if (SELECT_BUTTON_STARTER..SELECT_BUTTON_STARTER + COUNT_SELECT_BUTTONS)
                        .contains(&key) =>
                {
                    if !self.selection_mode {
                        println!("WARN: not in selection mode");
                    }

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
                    if self.selection_mode {
                        println!("WARN: in selection_mode");
                        continue;
                    }

                    let index = FADER_BYTES.iter().position(|b| *b == fader_byte).unwrap();
                    self.fader_vals[index] = value;
                    self.fader_vals_updated[index] = true;
                }
                (176, knob_byte, value) if knob_byte >= 13 && knob_byte <= 20 => {
                    if self.selection_mode {
                        println!("WARN: in selection_mode");
                        continue;
                    }

                    let index = knob_byte as usize - 13;
                    self.knob_vals[index] = value;
                    self.knob_vals_updated[index] = true;
                }
                // Set button on the left.
                (144, 82, 127) => {}
                // Big jog wheel: relative modification of an open numberpad.
                (176, cc, value) if cc == JOG_WHEEL_CC || cc == LEGACY_JOG_WHEEL_CC => {
                    let delta = jog_wheel_delta(cc, value);
                    if delta != 0 {
                        bpf::send_event(ControlEvent::MainUi(MainUiEvent::NumberpadAdjust {
                            delta,
                        }));
                    }
                }
                _ => {
                    println!("{}: {:?}", self.midi.device_id(), e);
                }
            }
        }
    }

    fn sync(&mut self, current_time: u32) {
        if current_time.wrapping_sub(self.last_sync) > 100 {
            self.last_sync = current_time;

            // Sync state. One decode, one copy: this used to fetch (and
            // therefore clone) the whole engine state twice.
            let dmx = bpf::get_dmx();

            self.active_groups = dmx.selection.group_ids.clone();

            // The faders/knobs drive whatever is actually rendering: the
            // overlay stack, or the single scene being previewed in live mode.
            self.scenes = if dmx.live_mode {
                vec![dmx.current_scene_focus]
            } else {
                dmx.current_overlay_scenes.clone()
            };

            self.dmx = dmx;

            self.nano_render_mode();
            self.nano_render_scenes();
            self.nano_render_groups();
        }
    }

    fn nano_render_mode(&mut self) {
        // for i in 0..127 {
        //     self.midi.send(0x90, i, 127);
        // }
        // for i in 0..127 {
        //     self.midi.send(0xB0, i, 0);
        // }

        // for i in 80..82 {
        //     self.midi
        //         .send(0xB0, i, self.selection_mode as u8 * 127);
        // }

        self.midi
            .send(0xB0, 81, self.selection_mode as u8 * 127);

        self.midi
            .send(0xB0, 80, !self.selection_mode as u8 * 127);
    }

    fn nano_blackout_groups(&mut self) {
        for g_index in 0..8 {
            self.midi
                .send(0x90, g_index + SELECT_BUTTON_STARTER, 0);
        }
    }

    fn nano_render_groups(&mut self) {
        if !self.selection_mode {
            return;
        }

        for g_index in 0..8 {
            if self.active_groups.contains(&g_index) {
                self.midi
                    .send(0x90, g_index + SELECT_BUTTON_STARTER, 127);
            } else {
                self.midi
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

            self.midi.send(
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
            if !self.selection_mode {
                continue;
            }
            match ev.body() {
                ControlEvent::SelectGroup(g_idx) => {
                    self.active_groups.insert(g_idx);
                    self.nano_render_groups();
                }
                ControlEvent::DeSelectGroup(g_idx) => {
                    self.active_groups.remove(&g_idx);
                    self.nano_render_groups();
                }
                _ => {}
            }
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
            let index = (knob_val.min(127) as u16).map_range(0..127, 0..max_index) as usize;

            let Some(scene_id) = self.scenes.get(knob) else {
                return;
            };

            let speed = AnimationSpeedModifier::from_index(index);

            bpf::send_event(ControlEvent::SetSceneMasterSpeed(*scene_id, speed));
        }
    }
}
