mod korg;
mod legacy;

use crate::korg::KorgSubSystem;
use crate::legacy::LegacyState;
use blaulicht_plugin_framework as bpf;
use blaulicht_plugin_framework::prelude::println;
use blaulicht_plugin_framework::Plugin;
use blaulicht_shared::TickInput;

// Set to false to require all MIDI devices at startup.
pub const ALLOW_MISSING_MIDI: bool = true;

#[derive(Default)]
pub struct MidiAllPlugin {
    korg: KorgSubSystem,
    legacy_state: LegacyState,
}

impl Plugin for MidiAllPlugin {
    fn initialize(&mut self, _input: TickInput) {
        self.korg.init();
        self.legacy_state.init();
        println!("[Midi All] Initialized");
    }

    fn run(&mut self, input: TickInput) {
        self.korg.run(input.clone());
        self.legacy_state.run(input);
    }

    fn ui(&mut self) {
        let now = self.legacy_state.ui_clock();
        self.legacy_state.render_ui(now);
    }
}

#[no_mangle]
extern "C" fn main() {
    bpf::hook_plugin(Box::new(MidiAllPlugin::default()));
}
