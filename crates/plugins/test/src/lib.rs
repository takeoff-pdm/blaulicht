use blaulicht_plugin_framework as bpf;
use blaulicht_plugin_framework::Plugin;
use blaulicht_shared::TickInput;
use serde::{Deserialize, Serialize};

const HFADER_PRIMARY_ID: u8 = 10;
const HFADER_SECONDARY_ID: u8 = 11;

#[derive(Serialize, Deserialize)]
#[serde(default)]
pub struct SaveState {
    restart_timer: usize,
}

impl Default for SaveState {
    fn default() -> Self {
        Self { restart_timer: 0 }
    }
}

pub struct SamplePlugin {
    state: SaveState,
}

impl Default for SamplePlugin {
    fn default() -> Self {
        Self {
            state: SaveState::default(), // midi_handle_out: None,
        }
    }
}

impl Plugin for SamplePlugin {
    fn initialize(&mut self, _input: TickInput) {
        self.state.restart_timer += 1;
        bpf::Command
    }

    fn run(&mut self, input: TickInput) {}
}

#[no_mangle]
extern "C" fn main() {
    bpf::hook_plugin(Box::new(SamplePlugin::default()));
}
