use std::collections::VecDeque;

use blaulicht_plugin_framework::prelude::println;
use blaulicht_plugin_framework::serial::SerialConnection;
use blaulicht_plugin_framework::{self as bpf, send_event};
use blaulicht_plugin_framework::{ui, Plugin};
use blaulicht_shared::{ControlEvent, ControlEventMessage, PluginUiEvent, TickInput};
use serde::{Deserialize, Serialize};

const MAX_REMOTE_BUTTONS: usize = 5;

const DEFAULT_REMOTE_SIGNALS: [[u32; MAX_REMOTE_BUTTONS]; 2] = [
    [
        11973516, // 1
        11973514, // 2
        11973513, // 3
        11973517, // 4
        11973515, // 5
    ],
    [
        10194876, // 1
        10194874, // 2
        10194873, // 3
        10194877, // 4
        10194875, // 5
    ],
];

const REMOTE_CHECKBOX_BASE_ID: u8 = 10;
const ADD_REMOTE_BUTTON_ID: u8 = 100;
const WIZARD_NAME_TEXT_ID: u8 = 110;
const WIZARD_RECORD_BASE_ID: u8 = 120;
const WIZARD_SAVE_BUTTON_ID: u8 = 130;
const WIZARD_CANCEL_BUTTON_ID: u8 = 131;
const SCENE_SELECT_BUTTON_BASE_ID: u8 = 150;
const SCENE_SELECT_OPTION_BASE_ID: u8 = 200;

#[derive(Clone, Serialize, Deserialize)]
struct RemoteDefinition {
    name: String,
    button_signals: [u32; MAX_REMOTE_BUTTONS],
    button_scene_ids: [Option<u8>; MAX_REMOTE_BUTTONS],
}

impl RemoteDefinition {
    fn from_signals(name: String, signals: [u32; MAX_REMOTE_BUTTONS]) -> Self {
        Self {
            name,
            button_signals: signals,
            button_scene_ids: [None; MAX_REMOTE_BUTTONS],
        }
    }
}

#[derive(Serialize, Deserialize)]
struct PersistedState {
    remote_definitions: Vec<RemoteDefinition>,
    remote_enabled: Vec<bool>,
}

#[derive(Default, Clone)]
struct RemoteWizard {
    remote_name: String,
    button_signals: [Option<u32>; MAX_REMOTE_BUTTONS],
    pending_button: Option<usize>,
    status_message: Option<String>,
}

impl RemoteWizard {
    fn new() -> Self {
        Self {
            remote_name: String::new(),
            button_signals: [None; MAX_REMOTE_BUTTONS],
            pending_button: None,
            status_message: None,
        }
    }

    fn is_complete(&self) -> bool {
        self.button_signals.iter().all(|slot| slot.is_some())
    }

    fn take_pending_button(&mut self) -> Option<usize> {
        self.pending_button.take()
    }

    fn set_pending_button(&mut self, index: usize) {
        self.pending_button = Some(index);
        self.status_message = Some(format!("Waiting for signal for button {}", index + 1));
    }

    fn record_signal(&mut self, button_index: usize, signal: u32) {
        if button_index < MAX_REMOTE_BUTTONS {
            self.button_signals[button_index] = Some(signal);
            self.status_message =
                Some(format!("Captured signal {signal} for button {}", button_index + 1));
        }
    }

    fn build_remote(&self, fallback_index: usize) -> Option<RemoteDefinition> {
        if !self.is_complete() {
            return None;
        }

        let mut signals = [0u32; MAX_REMOTE_BUTTONS];
        for (idx, value) in self.button_signals.iter().enumerate() {
            signals[idx] = value.unwrap_or_default();
        }

        let name = if self.remote_name.trim().is_empty() {
            format!("Remote {}", fallback_index + 1)
        } else {
            self.remote_name.trim().to_string()
        };

        Some(RemoteDefinition::from_signals(name, signals))
    }
}

pub struct SamplePlugin {
    conn: SerialConnection,
    remote_definitions: Vec<RemoteDefinition>,
    remote_enabled: Vec<bool>,
    log: VecDeque<String>,
    drums_toggle_time: u32,
    wizard: Option<RemoteWizard>,
    available_scenes: Vec<(u8, String)>,
    scene_selector_open: Option<(usize, usize)>,
}

impl Default for SamplePlugin {
    fn default() -> Self {
        let remote_definitions = DEFAULT_REMOTE_SIGNALS
            .iter()
            .enumerate()
            .map(|(idx, signals)| {
                RemoteDefinition::from_signals(
                    format!("Remote {}", idx + 1),
                    *signals,
                )
            })
            .collect::<Vec<_>>();
        let remote_enabled = vec![true; remote_definitions.len()];

        Self {
            conn: unsafe { SerialConnection::dummy() },
            remote_definitions,
            remote_enabled,
            log: VecDeque::new(),
            drums_toggle_time: 0,
            wizard: None,
            available_scenes: vec![],
            scene_selector_open: None,
        }
    }
}

impl SamplePlugin {
    fn push_log(&mut self, message: impl Into<String>) {
        self.log.push_back(message.into());
        while self.log.len() > 5 {
            self.log.pop_front();
        }
    }

    fn save_state(&self) {
        if let Ok(json) = serde_json::to_string(&PersistedState {
            remote_definitions: self.remote_definitions.clone(),
            remote_enabled: self.remote_enabled.clone(),
        }) {
            bpf::save_plugin_state(&json);
        }
    }

    fn load_state(&mut self) {
        let mut needs_save = false;

        if let Some(json) = bpf::load_plugin_state() {
            if let Ok(saved) = serde_json::from_str::<PersistedState>(&json) {
                self.remote_definitions = saved.remote_definitions;
                self.remote_enabled = saved.remote_enabled;
                if self.remote_enabled.len() < self.remote_definitions.len() {
                    self.remote_enabled
                        .resize(self.remote_definitions.len(), true);
                    needs_save = true;
                } else if self.remote_enabled.len() > self.remote_definitions.len() {
                    self.remote_enabled
                        .truncate(self.remote_definitions.len());
                    needs_save = true;
                }
            }
        }

        if self.remote_definitions.is_empty() {
            self.remote_definitions = DEFAULT_REMOTE_SIGNALS
                .iter()
                .enumerate()
                .map(|(idx, signals)| {
                    RemoteDefinition::from_signals(
                        format!("Remote {}", idx + 1),
                        *signals,
                    )
                })
                .collect();
            self.remote_enabled = vec![true; self.remote_definitions.len()];
            self.save_state();
            return;
        }

        if needs_save {
            self.save_state();
        }
    }

    fn scene_name(&self, scene_id: u8) -> Option<&str> {
        self.available_scenes
            .iter()
            .find(|(id, _)| *id == scene_id)
            .map(|(_, name)| name.as_str())
    }

    fn ensure_scene_assignments(&mut self) {
        for remote in &mut self.remote_definitions {
            for (button_index, slot) in remote.button_scene_ids.iter_mut().enumerate() {
                if slot.is_none() {
                    if let Some((scene_id, _)) = self.available_scenes.get(button_index) {
                        *slot = Some(*scene_id);
                    }
                }
            }
        }
    }

    fn clear_invalid_scene_selector(&mut self) {
        if let Some((remote_idx, button_idx)) = self.scene_selector_open {
            if remote_idx >= self.remote_definitions.len() || button_idx >= MAX_REMOTE_BUTTONS {
                self.scene_selector_open = None;
            }
        }
    }

    fn scene_selector_button_id(remote_idx: usize, button_idx: usize) -> Option<u8> {
        let offset = remote_idx
            .checked_mul(MAX_REMOTE_BUTTONS)?
            .checked_add(button_idx)?;
        let max_offset = (u8::MAX - SCENE_SELECT_BUTTON_BASE_ID) as usize;
        if offset > max_offset {
            return None;
        }
        Some(SCENE_SELECT_BUTTON_BASE_ID + offset as u8)
    }

    fn decode_scene_selector_button(id: u8) -> Option<(usize, usize)> {
        if id < SCENE_SELECT_BUTTON_BASE_ID || id >= SCENE_SELECT_OPTION_BASE_ID {
            return None;
        }
        let relative = id - SCENE_SELECT_BUTTON_BASE_ID;
        let remote_idx = (relative as usize) / MAX_REMOTE_BUTTONS;
        let button_idx = (relative as usize) % MAX_REMOTE_BUTTONS;
        Some((remote_idx, button_idx))
    }

    fn decode_scene_option(id: u8) -> Option<usize> {
        if id < SCENE_SELECT_OPTION_BASE_ID {
            return None;
        }
        Some((id - SCENE_SELECT_OPTION_BASE_ID) as usize)
    }

    fn handle_signal(&mut self, sig: u32, now: u32) {
        if let Some(wizard) = self.wizard.as_mut() {
            if let Some(button_index) = wizard.take_pending_button() {
                wizard.record_signal(button_index, sig);
                self.push_log(format!(
                    "Wizard captured {} for button {}",
                    sig,
                    button_index + 1
                ));
                return;
            }
        }

        self.process(sig, now);
    }

    fn process(&mut self, sig: u32, now: u32) {
        if sig == 10194868 || sig == 11973508 {
            let elapsed = now - self.drums_toggle_time;
            if (elapsed) > 500 {
                send_event(ControlEvent::MiscEvent {
                    descriptor: 43,
                    value: 0,
                });
                self.drums_toggle_time = now;
            } else {
                println!("DEBOUNCE: {elapsed} elapsed");
            }
            return;
        }

        let mut button_descriptor = None;

        for (remote_index, remote) in self.remote_definitions.iter().enumerate() {
            let index = match remote.button_signals.iter().position(|value| *value == sig) {
                Some(i) => i,
                None => {
                    println!("no result from receiver");
                    continue;
                }
            };

            button_descriptor = Some((remote_index, index));
            break;
        }

        println!("button-index: {button_descriptor:?}");
        if button_descriptor.is_none() {
            self.push_log(format!("Unmapped signal {}", sig));
            return;
        }

        if let Some((remote_idx, btn_idx)) = button_descriptor {
            if self.remote_enabled.get(remote_idx).copied().unwrap_or(false) {
                if let Some(remote) = self.remote_definitions.get(remote_idx) {
                    let scene_id = remote.button_scene_ids.get(btn_idx).copied().flatten();
                    match scene_id {
                        Some(scene_id) => {
                            bpf::send_event(ControlEvent::SetSceneFocus(scene_id));
                            let scene_label = self
                                .scene_name(scene_id)
                                .map(|name| name.to_string())
                                .unwrap_or_else(|| format!("Scene {scene_id}"));
                            self.push_log(format!(
                                "{} button {} -> {}",
                                remote.name,
                                btn_idx + 1,
                                scene_label
                            ));
                        }
                        None => {
                            self.push_log(format!(
                                "{} button {} has no scene assigned",
                                remote.name,
                                btn_idx + 1
                            ));
                        }
                    }
                } else {
                    println!("Remote definition missing for index {remote_idx}");
                    self.push_log(format!(
                        "Remote #{remote_idx} button {} missing definition",
                        btn_idx + 1
                    ));
                }
            } else {
                println!("Remote {remote_idx} disabled");
            }
        }
    }

    fn draw_ui(&mut self, events: &[ControlEventMessage], plugin_id: u8) {
        ui::begin();

        ui::label("Configured 433MHz remotes");
        if self.remote_definitions.is_empty() {
            ui::label("No remotes configured yet.");
        } else {
            for (idx, remote) in self.remote_definitions.iter().enumerate() {
                if idx >= (u8::MAX as usize).saturating_sub(REMOTE_CHECKBOX_BASE_ID as usize) {
                    continue;
                }
                let checkbox_id = REMOTE_CHECKBOX_BASE_ID.saturating_add(idx as u8);
                ui::checkbox(
                    &remote.name,
                    checkbox_id,
                    *self.remote_enabled.get(idx).unwrap_or(&false),
                );
            }
        }

        ui::separator();

        match &self.wizard {
            None => {
                ui::button("Add Remote", ADD_REMOTE_BUTTON_ID);
            }
            Some(wizard_state) => {
                ui::label("Remote setup wizard");
                ui::text_edit("Name", WIZARD_NAME_TEXT_ID, &wizard_state.remote_name);

                for button_index in 0..MAX_REMOTE_BUTTONS {
                    ui::begin_horizontal();
                    let signal_text = wizard_state.button_signals[button_index]
                        .map(|signal| signal.to_string())
                        .unwrap_or_else(|| "Not recorded".to_string());
                    ui::label(&format!("Button {}: {}", button_index + 1, signal_text));
                    ui::button(
                        &format!("Record {}", button_index + 1),
                        WIZARD_RECORD_BASE_ID.saturating_add(button_index as u8),
                    );
                    ui::end_horizontal();
                }

                ui::begin_horizontal();
                ui::button("Save remote", WIZARD_SAVE_BUTTON_ID);
                ui::button("Cancel", WIZARD_CANCEL_BUTTON_ID);
                ui::end_horizontal();

                if let Some(message) = &wizard_state.status_message {
                    ui::label(message);
                }
            }
        }

        ui::separator();
        ui::label("Button -> Scene mapping");

        if self.available_scenes.is_empty() {
            ui::label("No scenes available.");
        } else if self.remote_definitions.is_empty() {
            ui::label("Add a remote to configure mappings.");
        } else {
            for (remote_idx, remote) in self.remote_definitions.iter().enumerate() {
                ui::label(&remote.name);
                ui::begin_vertical();
                for button_index in 0..MAX_REMOTE_BUTTONS {
                    if let Some(button_id) =
                        Self::scene_selector_button_id(remote_idx, button_index)
                    {
                        ui::begin_horizontal();
                        ui::label(&format!("Button {}", button_index + 1));
                        let button_label = remote.button_scene_ids[button_index]
                            .and_then(|scene_id| {
                                self.scene_name(scene_id)
                                    .map(|name| format!("{name} (#{scene_id})"))
                            })
                            .unwrap_or_else(|| "Unassigned".to_string());
                        ui::button(&button_label, button_id);

                        if self.scene_selector_open == Some((remote_idx, button_index)) {
                            ui::begin_vertical();
                            for (scene_idx, (scene_id, scene_name)) in
                                self.available_scenes.iter().enumerate()
                            {
                                if scene_idx
                                    > (u8::MAX - SCENE_SELECT_OPTION_BASE_ID) as usize
                                {
                                    ui::label("Scene list truncated");
                                    break;
                                }

                                let option_id =
                                    SCENE_SELECT_OPTION_BASE_ID + scene_idx as u8;
                                ui::button(
                                    &format!("{} (#{})", scene_name, scene_id),
                                    option_id,
                                );
                            }
                            ui::end_vertical();
                        }

                        ui::end_horizontal();
                    }
                }
                ui::end_vertical();
            }
        }

        if !self.log.is_empty() {
            ui::separator();
            for item in &self.log {
                ui::label(item);
            }
        }

        for event in events {
            if let ControlEvent::PluginUi(ui_event, event_plugin_id) = event.body() {
                if event_plugin_id != plugin_id {
                    continue;
                }

                match ui_event {
                    PluginUiEvent::Checkbox { id, checked } => {
                        let idx = id.saturating_sub(REMOTE_CHECKBOX_BASE_ID) as usize;
                        if id >= REMOTE_CHECKBOX_BASE_ID && idx < self.remote_enabled.len() {
                            println!("Remote {} enabled -> {}", idx, checked);
                            self.remote_enabled[idx] = checked;
                            self.save_state();
                        }
                    }
                    PluginUiEvent::Button { id } => {
                        match id {
                            ADD_REMOTE_BUTTON_ID => {
                                if self.wizard.is_none() {
                                    self.wizard = Some(RemoteWizard::new());
                                }
                            }
                            WIZARD_SAVE_BUTTON_ID => {
                                if let Some(remote) = self
                                    .wizard
                                    .as_ref()
                                    .and_then(|wizard| {
                                        wizard.build_remote(self.remote_definitions.len())
                                    })
                                {
                                    let remote_name = remote.name.clone();
                                    self.remote_definitions.push(remote);
                                    self.remote_enabled.push(true);
                                    self.ensure_scene_assignments();
                                    self.scene_selector_open = None;
                                    self.save_state();
                                    self.push_log(format!("Added remote {}", remote_name));
                                    self.wizard = None;
                                } else if let Some(current) = self.wizard.as_mut() {
                                    current.status_message = Some(
                                        "Record all buttons before saving".to_string(),
                                    );
                                }
                            }
                            WIZARD_CANCEL_BUTTON_ID => {
                                self.wizard = None;
                            }
                            _ if id >= WIZARD_RECORD_BASE_ID
                                && id
                                    < WIZARD_RECORD_BASE_ID + MAX_REMOTE_BUTTONS as u8 =>
                            {
                                if let Some(wizard) = self.wizard.as_mut() {
                                    let button_index =
                                        (id - WIZARD_RECORD_BASE_ID) as usize;
                                    wizard.set_pending_button(button_index);
                                }
                            }
                            _ => {
                                if let Some((remote_idx, button_idx)) =
                                    Self::decode_scene_selector_button(id)
                                {
                                    if remote_idx < self.remote_definitions.len()
                                        && button_idx < MAX_REMOTE_BUTTONS
                                    {
                                        if self.scene_selector_open
                                            == Some((remote_idx, button_idx))
                                        {
                                            self.scene_selector_open = None;
                                        } else {
                                            self.scene_selector_open =
                                                Some((remote_idx, button_idx));
                                        }
                                    }
                                } else if let Some(scene_idx) =
                                    Self::decode_scene_option(id)
                                {
                                    if let Some((remote_idx, button_idx)) =
                                        self.scene_selector_open.take()
                                    {
                                        if scene_idx < self.available_scenes.len()
                                            && remote_idx < self.remote_definitions.len()
                                        {
                                            let (scene_id, scene_name) =
                                                self.available_scenes[scene_idx].clone();
                                            let mut remote_name = None;
                                            if let Some(remote) = self
                                                .remote_definitions
                                                .get_mut(remote_idx)
                                            {
                                                if button_idx < MAX_REMOTE_BUTTONS {
                                                    remote.button_scene_ids[button_idx] =
                                                        Some(scene_id);
                                                    remote_name =
                                                        Some(remote.name.clone());
                                                }
                                            }

                                            if let Some(remote_name) = remote_name {
                                                self.push_log(format!(
                                                    "{} button {} mapped to {} (#{})",
                                                    remote_name,
                                                    button_idx + 1,
                                                    scene_name,
                                                    scene_id
                                                ));
                                                self.save_state();
                                            }
                                        } else {
                                            self.push_log("Scene selection out of range");
                                        }
                                    }
                                }
                            }
                        }
                    }
                    PluginUiEvent::Text { id, text } => {
                        if id == WIZARD_NAME_TEXT_ID {
                            if let Some(wizard) = self.wizard.as_mut() {
                                wizard.remote_name = text.clone();
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

impl Plugin for SamplePlugin {
    fn initialize(&mut self, _input: TickInput) {
        self.load_state();
        let port_path = "/dev/antenna";
        println!("Open {port_path}...");
        let serial = match SerialConnection::open(&port_path, 115200) {
            Ok(p) => p,
            Err(e) => {
                panic!("Port error: {e}");
            }
        };
        self.conn = serial;
        println!("Antenna SERIAL plugin initialized");
    }

    fn run(&mut self, input: TickInput) {
        let state = bpf::get_dmx();
        let scenes = state
            .scenes
            .iter()
            .map(|(scene_id, scene)| (*scene_id, scene.name.clone()))
            .collect::<Vec<_>>();
        // Preserve ordering from engine state (already sorted by scene id)
        if scenes != self.available_scenes {
            self.available_scenes = scenes;
            self.ensure_scene_assignments();
        }

        self.clear_invalid_scene_selector();

        for ev in self.conn.poll() {
            let str = String::from_utf8_lossy(&ev.body);

            if str.starts_with("r: ") {
                let num = str.split("r: ").nth(1).unwrap().trim();
                println!("P: `{num}`: {:?}", num.as_bytes());
                let n: u32 = num.parse().expect("could not parse");

                self.handle_signal(n, input.clock);
            }

            println!("EV: {str} | {:?}", &ev.body);
        }

        self.draw_ui(&input.events.events, input.id);
    }
}

#[no_mangle]
extern "C" fn main() {
    bpf::hook_plugin(Box::new(SamplePlugin::default()));
}
