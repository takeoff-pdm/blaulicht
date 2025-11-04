use blaulicht_plugin_framework as bpf;
use blaulicht_plugin_framework::prelude::println;
use blaulicht_plugin_framework::Plugin;
use blaulicht_shared::{ControlEvent, ControlEventMessage, PluginUiEvent, TickInput};

pub struct SamplePlugin {
    clicks: u32,
    enabled: bool,
    intensity: u8,
    name: String,
    bio: String,
    color: (u8, u8, u8, u8),
}

impl Default for SamplePlugin {
    fn default() -> Self {
        Self {
            clicks: 0,
            enabled: true,
            intensity: 128,
            name: "Alice".to_string(),
            bio: "Hello Blaulicht!\nThis is a multi-line text.\nTry editing me!".to_string(),
            color: (120, 180, 200, 255),
        }
    }
}

impl SamplePlugin {
    fn handle_events(&mut self, events: &[ControlEventMessage], id: u8) {
        for e in events {
            if let ControlEvent::PluginUi(ui_ev, pid) = e.body() {
                if pid != id {
                    continue;
                }
                match ui_ev {
                    PluginUiEvent::Button { id } if id == 1 => {
                        self.clicks = self.clicks.saturating_add(1);
                        println!("Sample: Button clicked! -> {}", self.clicks);
                    }
                    PluginUiEvent::Checkbox { id, checked } if id == 2 => {
                        self.enabled = checked;
                        println!("Sample: Checkbox enabled = {}", self.enabled);
                    }
                    PluginUiEvent::Slider { id, value } if id == 3 => {
                        self.intensity = value;
                        println!("Sample: Slider intensity = {}", self.intensity);
                    }
                    PluginUiEvent::Text { id, text } if id == 4 => {
                        println!("Sample: Text changed = {}", text);
                        self.name = text.chars().take(32).collect();
                    }
                    PluginUiEvent::Text { id, text } if id == 6 => {
                        println!("Sample: Bio changed");
                        self.bio = text;
                    }
                    PluginUiEvent::Color { id, r, g, b, a } if id == 7 => {
                        self.color = (r, g, b, a);
                        println!("Sample: Color changed = rgba({}, {}, {}, {})", r, g, b, a);
                    }
                    _ => {}
                }
            }
        }
    }

    fn draw_ui(&self) {
        bpf::ui::begin();
        bpf::ui::begin_frame_styled(10, "Sample egui Plugin", 8, 8, 4, 4);
        bpf::ui::label(&format!("Clicks: {}", self.clicks));
        bpf::ui::button("Click Me", 1);
        bpf::ui::checkbox("Enabled", 2, self.enabled);
        bpf::ui::slider("Intensity", 3, 0, 255, self.intensity);
        bpf::ui::text_edit("Name", 4, &self.name);
        bpf::ui::text_edit_multiline("Bio", 6, &self.bio);
        bpf::ui::color_picker(7, self.color.0, self.color.1, self.color.2, self.color.3);
        bpf::ui::begin_collapsing(8, "Advanced", false);
        bpf::ui::begin_horizontal();
        bpf::ui::label("Preview:");
        bpf::ui::painter_begin(5, 120, 40);
        bpf::ui::painter_rect(0, 0, 120, 40, 30, 30, 30, 255);
        bpf::ui::painter_circle(20, 20, 10, 80, 180, 120, 255);
        bpf::ui::painter_circle(60, 20, 10, 200, 120, 80, 255);
        bpf::ui::painter_circle(100, 20, 10, 120, 80, 200, 255);
        bpf::ui::painter_line(0, 39, 119, 39, 255, 255, 255, 200, 1);
        bpf::ui::painter_text(4, 4, 12, 255, 255, 255, 255, &self.name);
        bpf::ui::painter_rect_stroke(0, 0, 120, 40, 200, 200, 200, 255, 1);
        bpf::ui::painter_circle_stroke(60, 20, 16, 200, 200, 50, 255, 2);
        bpf::ui::painter_cubic_bezier(0, 0, 20, 40, 100, 0, 120, 40, 255, 200, 0, 255, 1);
        bpf::ui::painter_end();
        bpf::ui::end_horizontal();
        bpf::ui::end_collapsing();

        bpf::ui::begin_tabs(9);
        bpf::ui::begin_tab(9, 1, "Overview");
        bpf::ui::label("This is the overview tab");
        bpf::ui::end_tab();
        bpf::ui::begin_tab(9, 2, "Details");
        bpf::ui::label("This is the details tab");
        bpf::ui::end_tab();
        bpf::ui::end_tabs();
        bpf::ui::end_frame();
    }
}

impl Plugin for SamplePlugin {
    fn initialize(&mut self, _input: TickInput) {
        println!("Sample egui plugin initialized");
    }

    fn run(&mut self, input: TickInput) {
        // Process events (e.g., button clicks from host UI)
        self.handle_events(&input.events.events, input.id);

        // Queue UI elements for host rendering
        self.draw_ui();
    }
}

#[no_mangle]
extern "C" fn main() {
    bpf::hook_plugin(Box::new(SamplePlugin::default()));
}
