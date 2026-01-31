mod korg;
mod legacy;

use crate::korg::KorgSubSystem;
use crate::legacy::LegacyState;
use blaulicht_plugin_framework as bpf;
use blaulicht_plugin_framework::prelude::println;
use blaulicht_plugin_framework::Plugin;
use blaulicht_shared::TickInput;

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
}

#[no_mangle]
extern "C" fn main() {
    bpf::hook_plugin(Box::new(MidiAllPlugin::default()));
}
