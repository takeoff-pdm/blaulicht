use blaulicht_plugin_framework as bpf;
use blaulicht_plugin_framework::prelude::println;
use blaulicht_plugin_framework::{MidiConnection, MidiEvent, Plugin};
use blaulicht_shared::{ControlEvent, ControlEventMessage, PluginUiEvent, TickInput};

#[derive(Debug, Clone, PartialEq)]
enum UiMode {
    Overview,
    AddingSequencer,
    EditingSequencer(usize),
    EditingStep(usize, usize),
    AddingStep(usize),
}

#[derive(Debug, Clone)]
struct SceneStep {
    scene_index: u8,
}

#[derive(Debug, Clone)]
struct Sequencer {
    name: String,
    steps: Vec<SceneStep>,
    midi_notes: Vec<u8>,
    current_step_index: usize,
}

pub struct DrumPlugin {
    midi_handle: Option<MidiConnection>,
    available_devices: Vec<String>,
    selected_device_index: usize,
    connection_error: Option<String>,
    recent_midi_messages: Vec<String>,
    sequencers: Vec<Sequencer>,
    ui_mode: UiMode,
    temp_sequencer_name: String,
    temp_scene_index: u8,
    temp_midi_note_input: String,
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
            ui_mode: UiMode::Overview,
            temp_sequencer_name: String::new(),
            temp_scene_index: 0,
            temp_midi_note_input: String::new(),
        }
    }
}

impl DrumPlugin {
    fn parse_midi_notes(input: &str) -> Vec<u8> {
        input
            .split(',')
            .filter_map(|s| s.trim().parse::<u8>().ok())
            .filter(|n| *n <= 127)
            .collect()
    }

    fn get_scene_name(scene_index: u8) -> String {
        bpf::get_dmx()
            .scenes
            .get(&scene_index)
            .map(|s| s.name.clone())
            .unwrap_or_else(|| format!("Scene {}", scene_index))
    }

    fn create_demo_sequencers() -> Vec<Sequencer> {
        let mut sequencers = Vec::new();

        let kick_sequencer = Sequencer {
            name: "Kick Drum".to_string(),
            midi_notes: vec![38, 40],
            steps: vec![
                SceneStep { scene_index: 0 },
                SceneStep { scene_index: 1 },
            ],
            current_step_index: 0,
        };

        let snare_sequencer = Sequencer {
            name: "Snare Drum".to_string(),
            midi_notes: vec![42, 44, 46],
            steps: vec![
                SceneStep { scene_index: 2 },
                SceneStep { scene_index: 3 },
            ],
            current_step_index: 0,
        };

        sequencers.push(kick_sequencer);
        sequencers.push(snare_sequencer);

        sequencers
    }

    fn add_sequencer(&mut self, name: String) {
        let sequencer = Sequencer {
            name,
            steps: Vec::new(),
            midi_notes: Vec::new(),
            current_step_index: 0,
        };
        self.sequencers.push(sequencer);
        println!("Added sequencer. Total: {}", self.sequencers.len());
    }

    fn delete_sequencer(&mut self, index: usize) {
        if index < self.sequencers.len() {
            let name = self.sequencers[index].name.clone();
            self.sequencers.remove(index);
            println!("Deleted sequencer '{}'. Total: {}", name, self.sequencers.len());
        }
    }

    fn add_step_to_sequencer(&mut self, seq_index: usize, scene_index: u8) {
        if seq_index < self.sequencers.len() {
            let step = SceneStep { scene_index };
            self.sequencers[seq_index].steps.push(step);
            println!("Added step to sequencer '{}'", self.sequencers[seq_index].name);
        }
    }

    fn delete_step(&mut self, seq_index: usize, step_index: usize) {
        if seq_index < self.sequencers.len() {
            let sequencer = &mut self.sequencers[seq_index];
            if step_index < sequencer.steps.len() {
                sequencer.steps.remove(step_index);
                if sequencer.current_step_index >= sequencer.steps.len() && !sequencer.steps.is_empty() {
                    sequencer.current_step_index = sequencer.steps.len() - 1;
                }
                println!("Deleted step from sequencer '{}'", sequencer.name);
            }
        }
    }

    fn update_step(&mut self, seq_index: usize, step_index: usize, scene_index: u8) {
        if seq_index < self.sequencers.len() {
            let sequencer = &mut self.sequencers[seq_index];
            if step_index < sequencer.steps.len() {
                sequencer.steps[step_index].scene_index = scene_index;
                println!("Updated step in sequencer '{}'", sequencer.name);
            }
        }
    }

    fn update_sequencer_midi_notes(&mut self, seq_index: usize, midi_notes: Vec<u8>) {
        if seq_index < self.sequencers.len() {
            self.sequencers[seq_index].midi_notes = midi_notes;
            println!("Updated MIDI notes for sequencer '{}'", self.sequencers[seq_index].name);
        }
    }

    fn handle_events(&mut self, events: &[ControlEventMessage]) {
        for e in events {
            if let ControlEvent::PluginUi(ui_ev) = e.body() {
                println!("Drum Plugin received UI event: {:?}", ui_ev);
                match ui_ev {
                    PluginUiEvent::Button { id } if id == 1 => {
                        if self.selected_device_index > 0 {
                            self.selected_device_index -= 1;
                        }
                    }
                    PluginUiEvent::Button { id } if id == 2 => {
                        if self.selected_device_index + 1 < self.available_devices.len() {
                            self.selected_device_index += 1;
                        }
                    }
                    PluginUiEvent::Button { id } if id == 3 => {
                        self.connect_to_selected_device();
                    }
                    PluginUiEvent::Button { id } if id == 4 => {
                        self.refresh_devices();
                    }
                    PluginUiEvent::Button { id } if id == 5 => {
                        self.disconnect();
                    }
                    PluginUiEvent::Button { id } if id == 10 => {
                        self.ui_mode = UiMode::AddingSequencer;
                        self.temp_sequencer_name.clear();
                    }
                    PluginUiEvent::Button { id } if id == 11 => {
                        if !self.temp_sequencer_name.trim().is_empty() {
                            self.add_sequencer(self.temp_sequencer_name.clone());
                        }
                        self.ui_mode = UiMode::Overview;
                    }
                    PluginUiEvent::Button { id } if id == 12 => {
                        self.ui_mode = UiMode::Overview;
                    }
                    PluginUiEvent::Button { id } if id == 13 => {
                        self.ui_mode = UiMode::Overview;
                    }
                    PluginUiEvent::Button { id } if id >= 20 && id < 40 => {
                        let seq_idx = ((id - 20) / 4) as usize;
                        let action = (id - 20) % 4;
                        
                        match action {
                            0 => {
                                if seq_idx < self.sequencers.len() {
                                    let sequencer = &self.sequencers[seq_idx];
                                    let notes_str: Vec<String> = sequencer.midi_notes.iter().map(|n| n.to_string()).collect();
                                    self.temp_midi_note_input = notes_str.join(", ");
                                    self.ui_mode = UiMode::EditingSequencer(seq_idx);
                                }
                            }
                            1 => {
                                self.delete_sequencer(seq_idx);
                            }
                            2 => {
                                if seq_idx < self.sequencers.len() {
                                    self.ui_mode = UiMode::AddingStep(seq_idx);
                                    self.temp_scene_index = 0;
                                }
                            }
                            _ => {}
                        }
                    }
                    PluginUiEvent::Button { id } if id >= 40 && id < 60 => {
                        let offset = (id - 40) as usize;
                        let seq_idx = offset / 20;
                        let step_idx = offset % 20;
                        
                        if seq_idx < self.sequencers.len() {
                            let sequencer = &self.sequencers[seq_idx];
                            if step_idx < sequencer.steps.len() {
                                let step = &sequencer.steps[step_idx];
                                self.temp_scene_index = step.scene_index;
                                self.ui_mode = UiMode::EditingStep(seq_idx, step_idx);
                            }
                        }
                    }
                    PluginUiEvent::Button { id } if id >= 60 && id < 80 => {
                        let offset = (id - 60) as usize;
                        let seq_idx = offset / 20;
                        let step_idx = offset % 20;
                        
                        self.delete_step(seq_idx, step_idx);
                    }
                    PluginUiEvent::Button { id } if id == 100 => {
                        if let UiMode::EditingStep(seq_idx, step_idx) = self.ui_mode {
                            self.update_step(seq_idx, step_idx, self.temp_scene_index);
                            self.ui_mode = UiMode::EditingSequencer(seq_idx);
                        }
                    }
                    PluginUiEvent::Button { id } if id == 101 => {
                        if let UiMode::EditingStep(seq_idx, _) = self.ui_mode {
                            self.ui_mode = UiMode::EditingSequencer(seq_idx);
                        }
                    }
                    PluginUiEvent::Button { id } if id == 102 => {
                        if let UiMode::AddingStep(seq_idx) = self.ui_mode {
                            self.add_step_to_sequencer(seq_idx, self.temp_scene_index);
                            self.ui_mode = UiMode::EditingSequencer(seq_idx);
                        }
                    }
                    PluginUiEvent::Button { id } if id == 103 => {
                        if let UiMode::EditingStep(seq_idx, _) | UiMode::AddingStep(seq_idx) = self.ui_mode {
                            self.ui_mode = UiMode::EditingSequencer(seq_idx);
                        }
                    }
                    PluginUiEvent::Button { id } if id == 104 => {
                        if let UiMode::EditingSequencer(seq_idx) = self.ui_mode {
                            let midi_notes = Self::parse_midi_notes(&self.temp_midi_note_input);
                            self.update_sequencer_midi_notes(seq_idx, midi_notes);
                        }
                    }
                    PluginUiEvent::Slider { id, value } if id == 110 => {
                        self.temp_scene_index = value;
                    }
                    PluginUiEvent::Text { id, text } if id == 120 => {
                        if self.temp_sequencer_name != text {
                            self.temp_sequencer_name = text.clone();
                        }
                    }
                    PluginUiEvent::Text { id, text } if id == 121 => {
                        if self.temp_midi_note_input != text {
                            self.temp_midi_note_input = text.clone();
                        }
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

                if sequencer.midi_notes.contains(&note) {
                    println!("Sequencer '{}': Note {} triggered transition from step {} to next", 
                             sequencer.name, note, sequencer.current_step_index);
                    
                    sequencer.current_step_index = (sequencer.current_step_index + 1) % sequencer.steps.len();
                    
                    let scene_index = sequencer.steps[sequencer.current_step_index].scene_index;
                    
                    let dmx_state = bpf::get_dmx();
                    let scene_name = dmx_state.scenes
                        .get(&scene_index)
                        .map(|s| s.name.as_str())
                        .unwrap_or("Unknown");
                    
                    println!("Sequencer '{}': Now at step {} (scene: {})", 
                             sequencer.name, sequencer.current_step_index, scene_name);
                    
                    bpf::send_event(ControlEvent::SetSceneFocus(scene_index));
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

    fn draw_device_management(&self) {
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

        if !self.recent_midi_messages.is_empty() {
            bpf::ui::separator();
            bpf::ui::label("Recent MIDI Messages:");
            for msg in &self.recent_midi_messages {
                bpf::ui::label(msg);
            }
        }
    }

    fn draw_overview_ui(&self) {
        bpf::ui::separator();
        bpf::ui::label(&format!("Sequencers ({})", self.sequencers.len()));
        
        bpf::ui::button("+ Add Sequencer", 10);

        for (seq_idx, sequencer) in self.sequencers.iter().enumerate() {
            bpf::ui::separator();
            bpf::ui::label(&format!("{}. {}", seq_idx + 1, sequencer.name));
            bpf::ui::label(&format!("  Steps: {}", sequencer.steps.len()));
            
            if !sequencer.steps.is_empty() {
                let current_step = &sequencer.steps[sequencer.current_step_index];
                let scene_name = Self::get_scene_name(current_step.scene_index);
                bpf::ui::label(&format!("  Current: {} (step {}/{})", 
                    scene_name,
                    sequencer.current_step_index + 1,
                    sequencer.steps.len()));
            }

            bpf::ui::begin_horizontal();
            let edit_id = (20 + seq_idx * 4) as u8;
            let delete_id = (21 + seq_idx * 4) as u8;
            bpf::ui::button("Edit", edit_id);
            bpf::ui::button("Delete", delete_id);
            bpf::ui::end_horizontal();
        }
    }

    fn draw_adding_sequencer_ui(&self) {
        bpf::ui::separator();
        bpf::ui::label("Add New Sequencer");
        
        bpf::ui::label("Name:");
        bpf::ui::text_edit("", 120, &self.temp_sequencer_name);
        
        bpf::ui::begin_horizontal();
        bpf::ui::button("Save", 11);
        bpf::ui::button("Cancel", 12);
        bpf::ui::end_horizontal();
    }

    fn draw_editing_sequencer_ui(&self, seq_idx: usize) {
        if seq_idx >= self.sequencers.len() {
            return;
        }

        let sequencer = &self.sequencers[seq_idx];
        
        bpf::ui::separator();
        bpf::ui::label(&format!("Editing: {}", sequencer.name));
        
        bpf::ui::separator();
        bpf::ui::label("MIDI Notes (comma-separated):");
        bpf::ui::text_edit("", 121, &self.temp_midi_note_input);
        bpf::ui::button("Update MIDI Notes", 104);
        
        bpf::ui::separator();
        bpf::ui::label("Steps:");
        bpf::ui::button("+ Add Step", (22 + seq_idx * 4) as u8);

        for (step_idx, step) in sequencer.steps.iter().enumerate() {
            bpf::ui::separator();
            
            let scene_name = Self::get_scene_name(step.scene_index);
            let is_current = step_idx == sequencer.current_step_index;
            let marker = if is_current { "→ " } else { "  " };
            
            bpf::ui::label(&format!("{}Step {}: {}", marker, step_idx + 1, scene_name));

            bpf::ui::begin_horizontal();
            let edit_step_id = (40 + seq_idx * 20 + step_idx) as u8;
            let delete_step_id = (60 + seq_idx * 20 + step_idx) as u8;
            bpf::ui::button("Edit", edit_step_id);
            bpf::ui::button("Delete", delete_step_id);
            bpf::ui::end_horizontal();
        }

        bpf::ui::separator();
        bpf::ui::button("< Back", 13);
    }

    fn draw_editing_step_ui(&self, seq_idx: usize, step_idx: usize) {
        if seq_idx >= self.sequencers.len() {
            return;
        }

        let sequencer = &self.sequencers[seq_idx];
        
        if step_idx >= sequencer.steps.len() {
            return;
        }
        
        bpf::ui::separator();
        bpf::ui::label(&format!("Edit Step {} in {}", step_idx + 1, sequencer.name));
        
        bpf::ui::label(&format!("Scene Index: {}", self.temp_scene_index));
        bpf::ui::label(&Self::get_scene_name(self.temp_scene_index));
        bpf::ui::slider("Scene", 110, 0, 255, self.temp_scene_index);
        
        bpf::ui::begin_horizontal();
        bpf::ui::button("Save", 100);
        bpf::ui::button("Cancel", 101);
        bpf::ui::end_horizontal();
    }

    fn draw_adding_step_ui(&self, seq_idx: usize) {
        if seq_idx >= self.sequencers.len() {
            return;
        }

        let sequencer = &self.sequencers[seq_idx];
        
        bpf::ui::separator();
        bpf::ui::label(&format!("Add Step to {}", sequencer.name));
        
        bpf::ui::label(&format!("Scene Index: {}", self.temp_scene_index));
        bpf::ui::label(&Self::get_scene_name(self.temp_scene_index));
        bpf::ui::slider("Scene", 110, 0, 255, self.temp_scene_index);
        
        bpf::ui::begin_horizontal();
        bpf::ui::button("Add", 102);
        bpf::ui::button("Cancel", 103);
        bpf::ui::end_horizontal();
    }

    fn draw_ui(&self) {
        bpf::ui::begin();
        bpf::ui::begin_frame_styled(10, "Drum Plugin", 8, 8, 4, 4);

        self.draw_device_management();

        match &self.ui_mode {
            UiMode::Overview => self.draw_overview_ui(),
            UiMode::AddingSequencer => self.draw_adding_sequencer_ui(),
            UiMode::EditingSequencer(seq_idx) => self.draw_editing_sequencer_ui(*seq_idx),
            UiMode::EditingStep(seq_idx, step_idx) => self.draw_editing_step_ui(*seq_idx, *step_idx),
            UiMode::AddingStep(seq_idx) => self.draw_adding_step_ui(*seq_idx),
        }

        bpf::ui::end_frame();
    }
}

impl Plugin for DrumPlugin {
    fn initialize(&mut self, _input: TickInput) {
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
                    "status: 0x{:02X}, kind: {}, value: {}",
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

