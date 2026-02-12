use blaulicht_plugin_framework as bpf;
use blaulicht_plugin_framework::prelude::println;
use blaulicht_plugin_framework::{ui, Plugin};
use blaulicht_shared::{
    AppPage, ControlEvent, ControlEventMessage, MainUiEvent, PluginUiEvent, TickInput,
};
use serde::{Deserialize, Serialize};

const HFADER_PRIMARY_ID: u8 = 10;
const HFADER_SECONDARY_ID: u8 = 11;

#[derive(Serialize, Deserialize)]
#[serde(default)]
pub struct SaveState {
    restart_timer: usize,
    hfader_primary: u8,
    hfader_secondary: u8,
}

impl Default for SaveState {
    fn default() -> Self {
        Self {
            restart_timer: 0,
            hfader_primary: 64,
            hfader_secondary: 192,
        }
    }
}

pub struct SamplePlugin {
    state: SaveState,
    // midi_handle_out: Option<MidiConnection>,
    // midi_handle_in: Option<MidiConnection>,
    saved: bool,

    last_print: u32,
}

impl Default for SamplePlugin {
    fn default() -> Self {
        Self {
            state: SaveState::default(), // midi_handle_out: None,
            // midi_handle_in: None,
            saved: false,
            last_print: 0,
        }
    }
}

impl SamplePlugin {
    fn save(&self) {
        if let Ok(json) = serde_json::to_string(&self.state) {
            bpf::save_plugin_state(bpf::PluginStateLocation::Showfile, &json);
        }
    }

    fn initialize(&mut self, _tick: TickInput) {
        if let Some(json) = bpf::load_plugin_state(bpf::PluginStateLocation::Showfile) {
            if let Ok(saved) = serde_json::from_str::<SaveState>(&json) {
                self.state = saved;
            } else {
                panic!("State loading failed!");
            }
        }
    }

    fn handle_ui_event(&mut self, event: &PluginUiEvent, plugin_id: u8, my_id: u8) {
        if plugin_id != my_id {
            return;
        }

        match event {
            PluginUiEvent::HFader { id, value } | PluginUiEvent::Slider { id, value }
                if *id == HFADER_PRIMARY_ID =>
            {
                if self.state.hfader_primary != *value {
                    println!("HFADER primary updated -> {}", value);
                    self.state.hfader_primary = *value;
                }
            }
            PluginUiEvent::HFader { id, value } | PluginUiEvent::Slider { id, value }
                if *id == HFADER_SECONDARY_ID =>
            {
                if self.state.hfader_secondary != *value {
                    println!("HFADER secondary updated -> {}", value);
                    self.state.hfader_secondary = *value;
                }
            }
            _ => {}
        }
    }

    fn render_ui(&self) {
        ui::begin();
        ui::begin_frame_styled(42, "HFader Playground", 8, 8, 4, 4);
        ui::label("Horizontal fader host bridge demo");

        ui::label(&format!("Primary value: {}", self.state.hfader_primary));
        ui::hfader(
            "Primary HFader",
            HFADER_PRIMARY_ID,
            0,
            u8::MAX,
            self.state.hfader_primary,
        );

        ui::label(&format!("Secondary value: {}", self.state.hfader_secondary));
        ui::hfader(
            "Secondary HFader",
            HFADER_SECONDARY_ID,
            0,
            u8::MAX,
            self.state.hfader_secondary,
        );

        ui::separator();
        ui::label("You can also scrub the faders using number input");
        ui::end_frame();
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
        let _state = bpf::get_dmx();

        if input.clock - self.last_print > 1000 {
            println!("A");
            self.last_print = input.clock;
        }

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
                ControlEvent::PluginUi(ui_event, plugin_id) => {
                    self.handle_ui_event(&ui_event, plugin_id, input.id);
                }
                _ => {}
            }
        }

        // let ev = self.midi_handle_out.unwrap().poll();
        // for e in &ev {
        //     println!("E: {e:?}");
        // }
        //
        // self.midi_handle_in.unwrap().send(127, 42, 69);

        self.render_ui();

        if !self.saved {
            self.save();
            self.saved = true;
        }
    }
}

#[no_mangle]
extern "C" fn main() {
    bpf::hook_plugin(Box::new(SamplePlugin::default()));
}
