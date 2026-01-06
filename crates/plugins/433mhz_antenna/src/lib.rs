use std::collections::VecDeque;

use blaulicht_plugin_framework::prelude::println;
use blaulicht_plugin_framework::serial::SerialConnection;
use blaulicht_plugin_framework::{self as bpf, send_event};
use blaulicht_plugin_framework::{ui, Plugin};
use blaulicht_shared::{ControlEvent, ControlEventMessage, PluginUiEvent, TickInput};

const REMOTES: [[u32; 5]; 2] = [
    [
        11973516, // 1
        11973514, // 2
        11973513, // 3
        11973517, // 4
        11973515, // 5
    ],
    [
        10194876, // 1
        10194874, // 2
        10194873, // 3
        10194877, // 4
        10194875, // 5
    ],
];

pub struct SamplePlugin {
    conn: SerialConnection,
    remote_disable_list: [bool; REMOTES.len()],
    log: VecDeque<String>,
    intensity_mapping: Vec<u8>,
    drums_toggle_time: u32,
}

impl Default for SamplePlugin {
    fn default() -> Self {
        Self {
            conn: unsafe { SerialConnection::dummy() },
            remote_disable_list: [true; REMOTES.len()],
            log: VecDeque::new(),
            intensity_mapping: vec![],
            drums_toggle_time: 0,
        }
    }
}

impl SamplePlugin {
    fn process(&mut self, sig: u32, now: u32) {
        if sig == 10194868 || sig == 11973508 {
            let elapsed = now - self.drums_toggle_time;
            if (elapsed) > 500 {
                send_event(ControlEvent::MiscEvent {
                    descriptor: 43,
                    value: 0,
                });
                self.drums_toggle_time = now;
            } else {
                println!("DEBOUNCE: {elapsed} elapsed");
            }
            return;
        }

        let mut button_descriptor = None;

        for (remote_index, remote) in REMOTES.iter().enumerate() {
            let index = match remote.iter().position(|e| *e == sig) {
                Some(i) => i,
                None => {
                    println!("no result from receiver");
                    continue;
                }
            };

            button_descriptor = Some((remote_index, index));
            break;
        }

        println!("button-index: {button_descriptor:?}");

        if let Some((remote, btn)) = button_descriptor {
            if self.remote_disable_list[remote] {
                let new_index = self.intensity_mapping.get(btn as usize);
                match new_index {
                    Some(scene_index) => {
                        bpf::send_event(ControlEvent::SetSceneFocus(*scene_index as u8));

                        self.log.push_back(format!("R: [{remote}]: {btn}"));
                        while self.log.len() > 3 {
                            self.log.pop_front();
                        }
                    }
                    None => {
                        println!("NO MAPPED INDEX.");
                    }
                }
            } else {
                println!("WTF");
            }
        }
    }

    fn draw_ui(&mut self, events: &[ControlEventMessage], pid: u8) {
        ui::begin();

        for idx in 0..REMOTES.len() {
            ui::checkbox(
                &format!("Remote {idx}"),
                idx as u8,
                self.remote_disable_list[idx],
            );
        }

        for item in &self.log {
            ui::label(&item);
        }

        for e in events {
            if let ControlEvent::PluginUi(ui_ev, pid) = e.body() {
                match ui_ev {
                    PluginUiEvent::Checkbox { checked, id } => {
                        if id != pid {
                            continue;
                        }
                        println!("checked ({id}): {checked}");
                        self.remote_disable_list[id as usize] = checked;
                    }
                    _ => {}
                }
            }
        }
    }
}

impl Plugin for SamplePlugin {
    fn initialize(&mut self, _input: TickInput) {
        let port_path = "/dev/antenna";
        println!("Open {port_path}...");
        let serial = match SerialConnection::open(&port_path, 115200) {
            Ok(p) => p,
            Err(e) => {
                panic!("Port error: {e}");
            }
        };
        self.conn = serial;
        println!("Antenna SERIAL plugin initialized");
    }

    fn run(&mut self, input: TickInput) {
        let state = bpf::get_dmx();
        if state.scenes.len() > 0 && self.intensity_mapping.is_empty() {
            println!("RUNNING INIT for intensity");
            self.intensity_mapping = vec![];

            let mut index = 0;

            for _ in 0..state.scenes.len() {
                for (scene_id, scene) in state.scenes.iter() {
                    let char_to_test = index.to_string().chars().nth(0).unwrap();
                    let name_char = scene.name.chars().nth(0).unwrap();
                    // println!("TESTING INDEX {index} and scene {scene_id} | CHAR: {char_to_test} vs {name_char}");
                    if scene.name.len() > 0 && name_char == char_to_test {
                        self.intensity_mapping.push(*scene_id);
                        println!("added intensity {index} --> Scene {scene_id}");
                        index += 1;
                        continue;
                    }
                }
            }

            if self.intensity_mapping.is_empty() {
                self.intensity_mapping.push(0)
            }

            println!("INTENSITY: {:?}", self.intensity_mapping);
        }

        for ev in self.conn.poll() {
            let str = String::from_utf8_lossy(&ev.body);

            if str.starts_with("r: ") {
                let num = str.split("r: ").nth(1).unwrap().trim();
                println!("P: `{num}`: {:?}", num.as_bytes());
                let n: u32 = num.parse().expect("could not parse");

                self.process(n, input.clock);
            }

            println!("EV: {str} | {:?}", &ev.body);
        }

        self.draw_ui(&input.events.events, input.id);
    }
}

#[no_mangle]
extern "C" fn main() {
    bpf::hook_plugin(Box::new(SamplePlugin::default()));
}
