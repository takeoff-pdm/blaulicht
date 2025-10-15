use std::collections::HashSet;
use std::{fmt::Display, mem::MaybeUninit};

use blaulicht_plugin_framework as bpf;
use blaulicht_plugin_framework::prelude::println;
use blaulicht_plugin_framework::{MidiConnection, MidiEvent, Plugin};
use blaulicht_shared::{hsv_to_rgb, ControlEvent, ControlEventMessage, PluginUiEvent, TickInput};
use blaulicht_shared::scene::{Scene, EngineSink, FixtureSelection, FixtureSelector};
use blaulicht_shared::fixture::state::FixtureState;
use blaulicht_shared::{HSVColor, FixtureProperty};
use std::collections::{BTreeMap, HashMap};
use map_range::MapRange;

#[derive(Debug, Clone)]
enum TransitionTrigger {
    MidiNote(Vec<u8>),
    Timer(u64),
}

#[derive(Debug, Clone)]
struct SceneStep {
    scene_index: u8,
    trigger: TransitionTrigger,
}

#[derive(Debug, Clone)]
struct Sequencer {
    name: String,
    steps: Vec<SceneStep>,
    current_step_index: usize,
}

pub struct DrumPlugin {
    midi_handle: Option<MidiConnection>,
    available_devices: Vec<String>,
    selected_device_index: usize,
    connection_error: Option<String>,
    recent_midi_messages: Vec<String>,
    sequencers: Vec<Sequencer>,
}

impl Default for DrumPlugin {
    fn default() -> Self {
        Self {
            midi_handle: None,
            available_devices: Vec::new(),
            selected_device_index: 0,
            connection_error: None,
            recent_midi_messages: Vec::new(),
            sequencers: Vec::new(),
        }
    }
}

impl DrumPlugin {
    fn create_demo_sequencers() -> Vec<Sequencer> {
        let mut sequencers = Vec::new();

        let kick_sequencer = Sequencer {
            name: "Kick Drum".to_string(),
            steps: vec![
                SceneStep {
                    scene_index: 0,
                    trigger: TransitionTrigger::MidiNote(vec![38, 40]),
                },
                SceneStep {
                    scene_index: 1,
                    trigger: TransitionTrigger::MidiNote(vec![38, 40]),
                },
            ],
            current_step_index: 0,
        };

        let snare_sequencer = Sequencer {
            name: "Snare Drum".to_string(),
            steps: vec![
                SceneStep {
                    scene_index: 2,
                    trigger: TransitionTrigger::MidiNote(vec![42, 44, 46]),
                },
                SceneStep {
                    scene_index: 3,
                    trigger: TransitionTrigger::MidiNote(vec![42, 44, 46]),
                },
            ],
            current_step_index: 0,
        };

        sequencers.push(kick_sequencer);
        sequencers.push(snare_sequencer);

        sequencers
    }

    fn handle_events(&mut self, events: &[ControlEventMessage]) {
        for e in events {
            if let ControlEvent::PluginUi(ui_ev) = e.body() {
                println!("Drum Plugin received UI event: {:?}", ui_ev);
                match ui_ev {
                    PluginUiEvent::Button { id } if id == 1 => {
                        println!("Previous button clicked");
                        if self.selected_device_index > 0 {
                            self.selected_device_index -= 1;
                        }
                    }
                    PluginUiEvent::Button { id } if id == 2 => {
                        println!("Next button clicked");
                        if self.selected_device_index + 1 < self.available_devices.len() {
                            self.selected_device_index += 1;
                        }
                    }
                    PluginUiEvent::Button { id } if id == 3 => {
                        println!("Connect button clicked");
                        self.connect_to_selected_device();
                    }
                    PluginUiEvent::Button { id } if id == 4 => {
                        println!("Refresh button clicked");
                        self.refresh_devices();
                    }
                    PluginUiEvent::Button { id } if id == 5 => {
                        println!("Disconnect button clicked");
                        self.disconnect();
                    }
                    _ => {
                        println!("Unhandled UI event: {:?}", ui_ev);
                    }
                }
            }
        }
    }

    fn process_midi_for_sequencers(&mut self, midi_events: &[MidiEvent]) {
        for midi_event in midi_events {
            let status_upper = midi_event.status & 0xF0;
            if status_upper != 0x90 {
                continue;
            }
            
            if midi_event.value == 0 {
                continue;
            }

            let note = midi_event.kind;

            for sequencer in &mut self.sequencers {
                if sequencer.steps.is_empty() {
                    continue;
                }

                let current_step = &sequencer.steps[sequencer.current_step_index];
                
                if let TransitionTrigger::MidiNote(ref trigger_notes) = current_step.trigger {
                    if trigger_notes.contains(&note) {
                        println!("Sequencer '{}': Note {} triggered transition from step {} to next", 
                                 sequencer.name, note, sequencer.current_step_index);
                        
                        sequencer.current_step_index = (sequencer.current_step_index + 1) % sequencer.steps.len();
                        
                        let dmx_state = bpf::get_dmx();
                        let scene_name = dmx_state.scenes
                            .get(&sequencer.steps[sequencer.current_step_index].scene_index)
                            .map(|s| s.name.as_str())
                            .unwrap_or("Unknown");
                        
                        println!("Sequencer '{}': Now at step {} (scene: {})", 
                                 sequencer.name, sequencer.current_step_index, scene_name);
                    }
                }
            }
        }
    }

    fn refresh_devices(&mut self) {
        self.available_devices = bpf::midi::enumerate_devices();
        println!("Found {} MIDI devices", self.available_devices.len());
        if self.selected_device_index >= self.available_devices.len() {
            self.selected_device_index = 0;
        }
    }

    fn connect_to_selected_device(&mut self) {
        if self.available_devices.is_empty() {
            self.connection_error = Some("No MIDI devices available".to_string());
            println!("No MIDI devices available");
            return;
        }

        let device_name = &self.available_devices[self.selected_device_index];
        println!("Connecting to MIDI device: {}", device_name);

        match MidiConnection::open(device_name) {
            Ok(handle) => {
                println!("Connected! Device ID: {}", handle.get_meta().device_id);
                self.midi_handle = Some(handle);
                self.connection_error = None;
            }
            Err(e) => {
                let error_msg = format!("Failed to connect: {:?}", e);
                println!("{}", error_msg);
                self.connection_error = Some(error_msg);
            }
        }
    }

    fn disconnect(&mut self) {
        println!("Disconnecting MIDI device");
        self.midi_handle = None;
        self.connection_error = None;
    }

    fn draw_ui(&self) {
        bpf::ui::begin();
        bpf::ui::begin_frame_styled(10, "Drum Plugin", 8, 8, 4, 4);

        let is_connected = self.midi_handle.is_some();

        if let Some(ref handle) = self.midi_handle {
            bpf::ui::label(&format!("Connected to device ID: {}", handle.get_meta().device_id));
        } else {
            bpf::ui::label("Not connected");
        }

        bpf::ui::separator();
        bpf::ui::label("MIDI Device Selection");

        if self.available_devices.is_empty() {
            bpf::ui::label("No devices found");
        } else {
            let device_name = &self.available_devices[self.selected_device_index];
            bpf::ui::label(&format!("Device {}/{}", self.selected_device_index + 1, self.available_devices.len()));
            bpf::ui::label(device_name);

            if !is_connected {
                bpf::ui::begin_horizontal();
                bpf::ui::button("Previous", 1);
                bpf::ui::button("Next", 2);
                bpf::ui::end_horizontal();

                bpf::ui::button("Connect", 3);
            } else {
                bpf::ui::label("(Device switching disabled while connected)");
                bpf::ui::button("Disconnect", 5);
            }
        }

        if !is_connected {
            bpf::ui::button("Refresh Devices", 4);
        }

        if let Some(ref error) = self.connection_error {
            bpf::ui::separator();
            bpf::ui::label(&format!("Error: {}", error));
        }

        if !self.sequencers.is_empty() {
            bpf::ui::separator();
            bpf::ui::label(&format!("Active Sequencers: {}", self.sequencers.len()));
            
            let engine_state = bpf::get_dmx();
            for sequencer in &self.sequencers {
                let current_step = &sequencer.steps[sequencer.current_step_index];
                let scene_name = engine_state.scenes
                    .get(&current_step.scene_index)
                    .map(|s| s.name.as_str())
                    .unwrap_or("(unknown)");
                
                bpf::ui::label(&format!("{}: {} (step {}/{})", 
                    sequencer.name, 
                    scene_name,
                    sequencer.current_step_index + 1,
                    sequencer.steps.len()));
            }
        }

        if !self.recent_midi_messages.is_empty() {
            bpf::ui::separator();
            bpf::ui::label("Recent MIDI Messages:");
            for msg in &self.recent_midi_messages {
                bpf::ui::label(msg);
            }
        }

        bpf::ui::end_frame();
    }
}

impl Plugin for DrumPlugin {
    fn initialize(&mut self, input: TickInput) {
        println!("Initializing Drum Plugin...");

        let state = bpf::get_dmx();
        println!("STATE: {state:?}");

        self.refresh_devices();
        
        self.sequencers = Self::create_demo_sequencers();
        println!("Initialized {} sequencers", self.sequencers.len());
    }

    fn run(&mut self, input: TickInput) {
        self.handle_events(&input.events.events);

        if let Some(ref handle) = self.midi_handle {
            let res = handle.poll();

            self.process_midi_for_sequencers(&res);

            for midi_event in res {
                let msg = format!(
                    "status: 0x{:02X}, kind: 0x{:02X}, value: {}",
                    midi_event.status, midi_event.kind, midi_event.value
                );
                println!("MIDI: {}", msg);
                
                self.recent_midi_messages.insert(0, msg);
                if self.recent_midi_messages.len() > 5 {
                    self.recent_midi_messages.truncate(5);
                }
            }
        }

        self.draw_ui();
    }
}

#[no_mangle]
#[inline(never)]
pub extern "C" fn __wasm_bp() {
    core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::SeqCst);
}
#[no_mangle]
extern "C" fn main() {
    __wasm_bp();
    bpf::hook_plugin(Box::new(DrumPlugin::default()));
}

