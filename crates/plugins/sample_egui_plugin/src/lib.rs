use blaulicht_plugin_framework as bpf;
use blaulicht_plugin_framework::prelude::println;
use blaulicht_plugin_framework::Plugin;
use blaulicht_shared::{ControlEvent, ControlEventMessage, EventOriginator, TickInput};

pub struct SamplePlugin {
    clicks: u32,
    enabled: bool,
    intensity: u8,
}

impl Default for SamplePlugin {
    fn default() -> Self {
        Self {
            clicks: 0,
            enabled: true,
            intensity: 128,
        }
    }
}

impl SamplePlugin {
    fn handle_events(&mut self, events: &[ControlEventMessage]) {
        for e in events {
            if let ControlEvent::MiscEvent { descriptor, value } = e.body() {
                if descriptor == 1 && value == 1 {
                    // Button with id=1 clicked in host UI
                    self.clicks = self.clicks.saturating_add(1);
                    println!("Sample: Button clicked! -> {}", self.clicks);
                } else if descriptor == 2 {
                    self.enabled = value != 0;
                    println!("Sample: Checkbox enabled = {}", self.enabled);
                } else if descriptor == 3 {
                    self.intensity = value;
                    println!("Sample: Slider intensity = {}", self.intensity);
                }
            }
        }
    }

    fn draw_ui(&self) {
        bpf::ui::begin();
        bpf::ui::label("Sample egui Plugin");
        bpf::ui::separator();
        bpf::ui::label(&format!("Clicks: {}", self.clicks));
        bpf::ui::button("Click Me", 1);
        bpf::ui::checkbox("Enabled", 2, self.enabled);
        bpf::ui::slider("Intensity", 3, 0, 255, self.intensity);
    }
}

impl Plugin for SamplePlugin {
    fn initialize(&mut self, _input: TickInput) {
        println!("Sample egui plugin initialized");
    }

    fn run(&mut self, input: TickInput) {
        // Process events (e.g., button clicks from host UI)
        self.handle_events(&input.events.events);

        // Queue UI elements for host rendering
        self.draw_ui();
    }
}

#[no_mangle]
extern "C" fn main() {
    bpf::hook_plugin(Box::new(SamplePlugin::default()));
}
