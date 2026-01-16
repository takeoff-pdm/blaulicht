use blaulicht_plugin_framework as bpf;
use blaulicht_plugin_framework::prelude::println;
use blaulicht_plugin_framework::serial::SerialConnection;
use blaulicht_plugin_framework::{send_event, MidiConnection};
use blaulicht_plugin_framework::{ui, Plugin};
use blaulicht_shared::{
    AppPage, ControlEvent, ControlEventMessage, MainUiEvent, PluginUiEvent, TickInput,
};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

#[derive(Serialize, Deserialize)]
pub struct SaveState {
    restart_timer: usize,
}

pub struct SamplePlugin {
    state: SaveState,
    // midi_handle_out: Option<MidiConnection>,
    // midi_handle_in: Option<MidiConnection>,
}

impl Default for SamplePlugin {
    fn default() -> Self {
        Self {
            state: SaveState { restart_timer: 0 }, // midi_handle_out: None,
                                                   // midi_handle_in: None,
        }
    }
}

impl SamplePlugin {
    fn save(&self) {
        if let Ok(json) = serde_json::to_string(&self.state) {
            bpf::save_plugin_state(&json);
        }
    }

    fn initialize(&mut self, _tick: TickInput) {
        if let Some(json) = bpf::load_plugin_state() {
            if let Ok(saved) = serde_json::from_str::<SaveState>(&json) {
                self.state = saved;
            } else {
                panic!("State loading failed!");
            }
        }
    }
}

impl Plugin for SamplePlugin {
    fn initialize(&mut self, _input: TickInput) {
        self.initialize(_input);

        println!("TEST: {}", self.state.restart_timer);

        self.state.restart_timer += 1;

        self.save();

        // self.midi_handle_out = Some(MidiConnection::open("Blaulicht OUT").unwrap());
        // self.midi_handle_in = Some(MidiConnection::open("Blaulicht IN").unwrap());
    }

    fn run(&mut self, input: TickInput) {
        let state = bpf::get_dmx();

        for ev in &input.events.events {
            println!("TEST-EV: {ev:?}");

            match ev.body() {
                ControlEvent::MainUi(main_ui_event) => match main_ui_event {
                    blaulicht_shared::MainUiEvent::NavigatePage(app_page) => {
                        // bpf::send_event(ControlEvent::MainUi(MainUiEvent::NavigatePage(
                        //     AppPage::Audio,
                        // )));
                    }
                    blaulicht_shared::MainUiEvent::SetPluginUIOpen { plugin_id, open } => {
                        println!("FOO: {plugin_id} | {open}");
                        // bpf::send_event(ControlEvent::MainUi(MainUiEvent::SetPluginUIOpen {
                        //     plugin_id,
                        //     open: false,
                        // }));
                    }
                    _ => {}
                },
                _ => {}
            }
        }

        // let ev = self.midi_handle_out.unwrap().poll();
        // for e in &ev {
        //     println!("E: {e:?}");
        // }
        //
        // self.midi_handle_in.unwrap().send(127, 42, 69);
    }
}

#[no_mangle]
extern "C" fn main() {
    bpf::hook_plugin(Box::new(SamplePlugin::default()));
}
