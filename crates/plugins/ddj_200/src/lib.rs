mod ddj;

use blaulicht_plugin_framework as bpf;
use blaulicht_plugin_framework::prelude::println;
use blaulicht_plugin_framework::Plugin;
use blaulicht_shared::TickInput;

use crate::ddj::DDJSubSystem;

#[derive(Default)]
pub struct MidiAllPlugin {
    ddj: DDJSubSystem,
}

impl Plugin for MidiAllPlugin {
    fn initialize(&mut self, _input: TickInput) {
        self.ddj.init();
        println!("[Midi DDJ] Initialized");
    }

    fn run(&mut self, input: TickInput) {
        self.ddj.run(input.clone());
    }
}

#[no_mangle]
extern "C" fn main() {
    bpf::hook_plugin(Box::new(MidiAllPlugin::default()));
}
