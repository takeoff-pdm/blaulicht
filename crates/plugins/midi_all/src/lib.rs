mod korg;
mod korg_twin;
mod legacy;

use crate::korg::KorgSubSystem;
use crate::legacy::mapping::MappingDevice;
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
        // UI first (the twin turns clicks into synthetic MIDI for this tick),
        // then the subsystems poll their devices.
        let korg = &mut self.korg;
        self.legacy_state.run(input.clone(), || {
            korg.render_twin(&input.events.events, input.id, input.clock)
        });
        let legacy = &mut self.legacy_state;
        self.korg
            .run(input, |e| legacy.handle_mapped_input(MappingDevice::Korg, e));
    }
}

#[no_mangle]
extern "C" fn main() {
    bpf::hook_plugin(Box::new(MidiAllPlugin::default()));
}
