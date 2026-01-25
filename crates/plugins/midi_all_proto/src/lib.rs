use blaulicht_plugin_framework as bpf;
use blaulicht_plugin_framework::prelude::println;
use blaulicht_plugin_framework::Plugin;
use blaulicht_shared::TickInput;

use crate::korg::KorgSubSystem;

mod korg;

#[derive(Default)]
pub struct MidiAllPlugin {
    korg: KorgSubSystem,
}

impl Plugin for MidiAllPlugin {
    fn initialize(&mut self, _input: TickInput) {
        println!("Initializing...");

        // Get state dump.
        let state = bpf::get_dmx();
        println!("STATE TEST -> {} SCENES.", state.scenes.len());

        self.korg.init();
    }

    fn run(&mut self, input: TickInput) {
        self.korg.run(input.clone());
    }
}

#[no_mangle]
extern "C" fn main() {
    bpf::hook_plugin(Box::new(MidiAllPlugin::default()));
}
