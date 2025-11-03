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
}

impl Default for SamplePlugin {
    fn default() -> Self {
        Self {
            conn: unsafe { SerialConnection::dummy() },
            remote_disable_list: [false; REMOTES.len()],
            log: VecDeque::new(),
        }
    }
}

impl SamplePlugin {
    fn process(&mut self, sig: u32) {
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
                bpf::send_event(ControlEvent::SetSceneFocus(btn as u8));

                self.log.push_back(format!("R: [{remote}]: {btn}"));
                while self.log.len() > 3 {
                    self.log.pop_front();
                }
            } else {
                println!("WTF");
            }
        }
    }

    fn draw_ui(&mut self, events: &[ControlEventMessage]) {
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
            if let ControlEvent::PluginUi(ui_ev) = e.body() {
                match ui_ev {
                    PluginUiEvent::Checkbox { checked, id } => {
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
        let serial = SerialConnection::open("/dev/ttyUSB0", 115200).unwrap();
        self.conn = serial;
        println!("Sample SERIAL plugin initialized");
    }

    fn run(&mut self, input: TickInput) {
        for ev in self.conn.poll() {
            let str = String::from_utf8_lossy(&ev.body);

            if str.starts_with("r: ") {
                let num = str.split("r: ").nth(1).unwrap().trim();
                println!("P: `{num}`: {:?}", num.as_bytes());
                let n: u32 = num.parse().expect("could not parse");

                self.process(n);
            }

            println!("EV: {str} | {:?}", &ev.body);
        }

        self.draw_ui(&input.events.events);
    }
}

#[no_mangle]
extern "C" fn main() {
    bpf::hook_plugin(Box::new(SamplePlugin::default()));
}
