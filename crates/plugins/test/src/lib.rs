use std::collections::VecDeque;

use blaulicht_plugin_framework::prelude::println;
use blaulicht_plugin_framework::serial::SerialConnection;
use blaulicht_plugin_framework::{self as bpf, send_event};
use blaulicht_plugin_framework::{ui, Plugin};
use blaulicht_shared::{ControlEvent, ControlEventMessage, PluginUiEvent, TickInput};

pub struct SamplePlugin {
    restart_timer: usize,
}

impl Default for SamplePlugin {
    fn default() -> Self {
        Self { restart_timer: 0 }
    }
}

impl Plugin for SamplePlugin {
    fn initialize(&mut self, _input: TickInput) {
        self.restart_timer += 1;
        println!("TEST: {}", self.restart_timer);
    }

    fn run(&mut self, input: TickInput) {
        let state = bpf::get_dmx();
    }
}

#[no_mangle]
extern "C" fn main() {
    bpf::hook_plugin(Box::new(SamplePlugin::default()));
}
