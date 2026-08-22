use std::collections::VecDeque;

use blaulicht_plugin_framework::prelude::println;
use blaulicht_plugin_framework::serial::SerialConnection;
use blaulicht_plugin_framework::{self as bpf, send_event};
use blaulicht_plugin_framework::{ui, Plugin};
use blaulicht_shared::{
    view::View, AnimationSpeedModifier, ControlEvent, ControlEventMessage, PluginUiEvent, TickInput,
};
use serde::{Deserialize, Serialize};

const MAX_REMOTE_BUTTONS: usize = 7;

const DEFAULT_REMOTE_SIGNALS: [[u32; MAX_REMOTE_BUTTONS]; 1] = [
    [
        11973516, // 1
        11973514, // 2
        11973513, // 3
        11973517, // 4
        11973515, // 5
        11973508, // 6
        11973506, // 7
                  //
    ],
    // [
    //     10194876, // 1
    //     10194874, // 2
    //     10194873, // 3
    //     10194877, // 4
    //     10194875, // 5
    // ],
];

const REMOTE_CHECKBOX_BASE_ID: u8 = 10;
const ADD_REMOTE_BUTTON_ID: u8 = 100;
const WIZARD_NAME_TEXT_ID: u8 = 110;
const WIZARD_RECORD_BASE_ID: u8 = 120;
const WIZARD_SAVE_BUTTON_ID: u8 = 130;
const WIZARD_CANCEL_BUTTON_ID: u8 = 131;
const VIEW_SELECT_BUTTON_BASE_ID: u8 = 150;
const VIEW_SELECT_OPTION_BASE_ID: u8 = 200;
const SPEED_SCENE_SELECT_BUTTON_BASE_ID: u8 = 230;
const SPEED_SCENE_SELECT_OPTION_BASE_ID: u8 = 250;
const SPEED_MODIFIER_SELECT_BUTTON_BASE_ID: u8 = 40;
const SPEED_MODIFIER_SELECT_OPTION_BASE_ID: u8 = 80;

#[derive(Clone, Serialize, Deserialize)]
struct RemoteDefinition {
    name: String,
    button_signals: [u32; MAX_REMOTE_BUTTONS],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    legacy_button_scene_ids: Option<[Option<u8>; MAX_REMOTE_BUTTONS]>,
    #[serde(default = "default_button_view_ids")]
    button_view_ids: [Option<u8>; MAX_REMOTE_BUTTONS],
    // #[serde(default = "default_button_speed_scene_ids")]
    // button_speed_scene_ids: [Option<u8>; MAX_REMOTE_BUTTONS],
    #[serde(default = "default_button_speed_cycle_start")]
    button_speed_cycle_start: [AnimationSpeedModifier; MAX_REMOTE_BUTTONS],
}

impl RemoteDefinition {
    fn from_signals(name: String, signals: [u32; MAX_REMOTE_BUTTONS]) -> Self {
        Self {
            name,
            button_signals: signals,
            legacy_button_scene_ids: None,
            button_view_ids: default_button_view_ids(),
            // button_speed_scene_ids: default_button_speed_scene_ids(),
            button_speed_cycle_start: default_button_speed_cycle_start(),
        }
    }

    fn drop_legacy_scene_mappings(&mut self) -> bool {
        if let Some(legacy) = self.legacy_button_scene_ids.take() {
            // Only trigger a save if there was at least one legacy assignment.
            let had_assignment = legacy.iter().any(|entry| entry.is_some());
            self.button_view_ids = default_button_view_ids();
            had_assignment
        } else {
            false
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
            self.status_message = Some(format!(
                "Captured signal {signal} for button {}",
                button_index + 1
            ));
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

fn default_button_view_ids() -> [Option<u8>; MAX_REMOTE_BUTTONS] {
    [None; MAX_REMOTE_BUTTONS]
}

fn default_button_speed_scene_ids() -> [Option<u8>; MAX_REMOTE_BUTTONS] {
    [None; MAX_REMOTE_BUTTONS]
}

fn default_button_speed_cycle_start() -> [AnimationSpeedModifier; MAX_REMOTE_BUTTONS] {
    [AnimationSpeedModifier::_1; MAX_REMOTE_BUTTONS]
}

#[derive(Clone, PartialEq)]
struct ViewInfo {
    id: u8,
    view: View,
    base_scene_name: Option<String>,
    overlay_names: Vec<(u8, Option<String>)>,
}

impl ViewInfo {
    fn base_label(&self) -> String {
        match &self.base_scene_name {
            Some(name) => format!("{name} (#{})", self.view.base_scene),
            None => format!("Scene #{}", self.view.base_scene),
        }
    }

    fn overlays_label(&self) -> String {
        if self.view.overlays.is_empty() {
            return "none".to_string();
        }

        self.overlay_names
            .iter()
            .map(|(overlay_id, name)| match name {
                Some(name) => format!("{name} (#{overlay_id})"),
                None => format!("Scene #{overlay_id}"),
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn button_label(&self) -> String {
        format!("{} (#{})", self.view.name, self.id)
    }

    fn option_label(&self) -> String {
        format!(
            "{} (#{}) – base {} – overlays {}",
            self.view.name,
            self.id,
            self.base_label(),
            self.overlays_label()
        )
    }
}

pub struct SamplePlugin {
    conn: SerialConnection,
    remote_definitions: Vec<RemoteDefinition>,
    remote_enabled: Vec<bool>,
    log: VecDeque<String>,
    drums_toggle_time: u32,
    wizard: Option<RemoteWizard>,
    available_views: Vec<ViewInfo>,
    available_scenes: Vec<(u8, String)>,
    scene_speeds: Vec<(u8, AnimationSpeedModifier)>,
    view_selector_open: Option<(usize, usize)>,
    speed_selector_open: Option<(usize, usize)>,
    speed_modifier_selector_open: Option<(usize, usize)>,
}

impl Default for SamplePlugin {
    fn default() -> Self {
        let remote_definitions = DEFAULT_REMOTE_SIGNALS
            .iter()
            .enumerate()
            .map(|(idx, signals)| {
                RemoteDefinition::from_signals(format!("Remote {}", idx + 1), *signals)
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
            available_views: vec![],
            available_scenes: vec![],
            scene_speeds: vec![],
            view_selector_open: None,
            speed_selector_open: None,
            speed_modifier_selector_open: None,
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
            bpf::save_plugin_state(bpf::PluginStateLocation::Showfile, &json);
        }
    }

    fn load_state(&mut self) {
        let mut needs_save = false;

        if let Some(json) = bpf::load_plugin_state(bpf::PluginStateLocation::Showfile) {
            if let Ok(saved) = serde_json::from_str::<PersistedState>(&json) {
                self.remote_definitions = saved.remote_definitions;
                self.remote_enabled = saved.remote_enabled;
                if self.remote_enabled.len() < self.remote_definitions.len() {
                    self.remote_enabled
                        .resize(self.remote_definitions.len(), true);
                    needs_save = true;
                } else if self.remote_enabled.len() > self.remote_definitions.len() {
                    self.remote_enabled.truncate(self.remote_definitions.len());
                    needs_save = true;
                }
            }
        }

        let mut migrated_remote_names = Vec::new();
        for remote in &mut self.remote_definitions {
            if remote.drop_legacy_scene_mappings() {
                migrated_remote_names.push(remote.name.clone());
            }
        }

        if !migrated_remote_names.is_empty() {
            for name in migrated_remote_names {
                self.push_log(format!(
                    "Cleared scene assignments for '{name}'. Please assign a view."
                ));
            }
            needs_save = true;
        }

        if self.remote_definitions.is_empty() {
            self.remote_definitions = DEFAULT_REMOTE_SIGNALS
                .iter()
                .enumerate()
                .map(|(idx, signals)| {
                    RemoteDefinition::from_signals(format!("Remote {}", idx + 1), *signals)
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

    fn view_info(&self, view_id: u8) -> Option<&ViewInfo> {
        self.available_views.iter().find(|info| info.id == view_id)
    }

    fn scene_name(&self, scene_id: u8) -> Option<&str> {
        self.available_scenes
            .iter()
            .find(|(id, _)| *id == scene_id)
            .map(|(_, name)| name.as_str())
    }

    fn current_scene_speed(&self, scene_id: u8) -> AnimationSpeedModifier {
        self.scene_speeds
            .iter()
            .find(|(id, _)| *id == scene_id)
            .map(|(_, speed)| *speed)
            .unwrap_or(AnimationSpeedModifier::_1)
    }

    fn cache_scene_speed(&mut self, scene_id: u8, speed: AnimationSpeedModifier) {
        if let Some(entry) = self.scene_speeds.iter_mut().find(|(id, _)| *id == scene_id) {
            entry.1 = speed;
        } else {
            self.scene_speeds.push((scene_id, speed));
        }
    }

    fn next_cycle_speed(
        current: AnimationSpeedModifier,
        cycle_start: AnimationSpeedModifier,
    ) -> AnimationSpeedModifier {
        let all = AnimationSpeedModifier::ALL;
        let curr_idx = all
            .iter()
            .position(|value| *value == current)
            .unwrap_or_else(|| {
                all.iter()
                    .position(|value| *value == cycle_start)
                    .unwrap_or(0)
            });
        let mut next_idx = curr_idx + 1;
        if next_idx >= all.len() {
            next_idx = all
                .iter()
                .position(|value| *value == cycle_start)
                .unwrap_or(0);
        }
        all[next_idx]
    }

    fn ensure_view_assignments(&mut self) {
        for remote in &mut self.remote_definitions {
            for (button_index, slot) in remote.button_view_ids.iter_mut().enumerate() {
                if slot.is_none() {
                    if let Some(view_info) = self.available_views.get(button_index) {
                        *slot = Some(view_info.id);
                    }
                }
            }
        }
    }

    fn clear_invalid_view_selector(&mut self) {
        if let Some((remote_idx, button_idx)) = self.view_selector_open {
            if remote_idx >= self.remote_definitions.len() || button_idx >= MAX_REMOTE_BUTTONS {
                self.view_selector_open = None;
            }
        }
    }

    fn clear_invalid_speed_selector(&mut self) {
        if let Some((remote_idx, button_idx)) = self.speed_selector_open {
            if remote_idx >= self.remote_definitions.len() || button_idx >= MAX_REMOTE_BUTTONS {
                self.speed_selector_open = None;
            }
        }
    }

    fn clear_invalid_speed_modifier_selector(&mut self) {
        if let Some((remote_idx, button_idx)) = self.speed_modifier_selector_open {
            if remote_idx >= self.remote_definitions.len() || button_idx >= MAX_REMOTE_BUTTONS {
                self.speed_modifier_selector_open = None;
            }
        }
    }

    fn view_selector_button_id(remote_idx: usize, button_idx: usize) -> Option<u8> {
        let offset = remote_idx
            .checked_mul(MAX_REMOTE_BUTTONS)?
            .checked_add(button_idx)?;
        let max_offset = (VIEW_SELECT_OPTION_BASE_ID - VIEW_SELECT_BUTTON_BASE_ID - 1) as usize;
        if offset > max_offset {
            return None;
        }
        Some(VIEW_SELECT_BUTTON_BASE_ID + offset as u8)
    }

    fn decode_view_selector_button(id: u8) -> Option<(usize, usize)> {
        if id < VIEW_SELECT_BUTTON_BASE_ID || id >= VIEW_SELECT_OPTION_BASE_ID {
            return None;
        }
        let relative = id - VIEW_SELECT_BUTTON_BASE_ID;
        let remote_idx = (relative as usize) / MAX_REMOTE_BUTTONS;
        let button_idx = (relative as usize) % MAX_REMOTE_BUTTONS;
        Some((remote_idx, button_idx))
    }

    fn decode_view_option(id: u8) -> Option<usize> {
        if id < VIEW_SELECT_OPTION_BASE_ID || id >= SPEED_SCENE_SELECT_BUTTON_BASE_ID {
            return None;
        }
        Some((id - VIEW_SELECT_OPTION_BASE_ID) as usize)
    }

    fn speed_scene_selector_button_id(remote_idx: usize, button_idx: usize) -> Option<u8> {
        let offset = remote_idx
            .checked_mul(MAX_REMOTE_BUTTONS)?
            .checked_add(button_idx)?;
        let max_offset =
            (SPEED_SCENE_SELECT_OPTION_BASE_ID - SPEED_SCENE_SELECT_BUTTON_BASE_ID - 1) as usize;
        if offset > max_offset {
            return None;
        }
        Some(SPEED_SCENE_SELECT_BUTTON_BASE_ID + offset as u8)
    }

    fn decode_speed_scene_selector_button(id: u8) -> Option<(usize, usize)> {
        if id < SPEED_SCENE_SELECT_BUTTON_BASE_ID || id >= SPEED_SCENE_SELECT_OPTION_BASE_ID {
            return None;
        }
        let relative = id - SPEED_SCENE_SELECT_BUTTON_BASE_ID;
        let remote_idx = (relative as usize) / MAX_REMOTE_BUTTONS;
        let button_idx = (relative as usize) % MAX_REMOTE_BUTTONS;
        Some((remote_idx, button_idx))
    }

    fn decode_speed_scene_option(id: u8) -> Option<usize> {
        if id < SPEED_SCENE_SELECT_OPTION_BASE_ID {
            return None;
        }
        Some((id - SPEED_SCENE_SELECT_OPTION_BASE_ID) as usize)
    }

    fn speed_modifier_selector_button_id(remote_idx: usize, button_idx: usize) -> Option<u8> {
        let offset = remote_idx
            .checked_mul(MAX_REMOTE_BUTTONS)?
            .checked_add(button_idx)?;
        let max_offset =
            (SPEED_MODIFIER_SELECT_OPTION_BASE_ID - SPEED_MODIFIER_SELECT_BUTTON_BASE_ID - 1) as usize;
        if offset > max_offset {
            return None;
        }
        Some(SPEED_MODIFIER_SELECT_BUTTON_BASE_ID + offset as u8)
    }

    fn decode_speed_modifier_selector_button(id: u8) -> Option<(usize, usize)> {
        if id < SPEED_MODIFIER_SELECT_BUTTON_BASE_ID || id >= SPEED_MODIFIER_SELECT_OPTION_BASE_ID {
            return None;
        }
        let relative = id - SPEED_MODIFIER_SELECT_BUTTON_BASE_ID;
        let remote_idx = (relative as usize) / MAX_REMOTE_BUTTONS;
        let button_idx = (relative as usize) % MAX_REMOTE_BUTTONS;
        Some((remote_idx, button_idx))
    }

    fn decode_speed_modifier_option(id: u8) -> Option<usize> {
        if id < SPEED_MODIFIER_SELECT_OPTION_BASE_ID || id >= WIZARD_RECORD_BASE_ID {
            return None;
        }
        Some((id - SPEED_MODIFIER_SELECT_OPTION_BASE_ID) as usize)
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
            if self
                .remote_enabled
                .get(remote_idx)
                .copied()
                .unwrap_or(false)
            {
                if let Some(remote) = self.remote_definitions.get(remote_idx).cloned() {
                    let remote_name = remote.name.clone();

                    if let Some(view_id) = remote.button_view_ids.get(btn_idx).copied().flatten() {
                        if let Some(view_info) = self.view_info(view_id).cloned() {
                            let cycle_start = remote.button_speed_cycle_start[btn_idx];
                            let mut events = Vec::with_capacity(2 + view_info.view.overlays.len());
                            events.push(ControlEvent::SetSceneMasterSpeed(
                                view_info.view.base_scene,
                                cycle_start,
                            ));
                            for overlay in &view_info.view.overlays {
                                events
                                    .push(ControlEvent::SetSceneMasterSpeed(*overlay, cycle_start));
                            }
                            events.push(ControlEvent::SetSceneFocus(view_info.view.base_scene));
                            events.push(ControlEvent::SetOverlays(view_info.view.overlays.clone()));
                            bpf::send_event(ControlEvent::Transaction(events));

                            self.cache_scene_speed(view_info.view.base_scene, cycle_start);
                            for overlay in &view_info.view.overlays {
                                self.cache_scene_speed(*overlay, cycle_start);
                            }

                            self.push_log(format!(
                                "{} button {} -> {}",
                                remote_name,
                                btn_idx + 1,
                                view_info.option_label()
                            ));
                        } else {
                            self.push_log(format!(
                                "{} button {} view #{view_id} unavailable",
                                remote_name,
                                btn_idx + 1
                            ));
                        }
                    } else {
                        self.push_log(format!(
                            "{} button {} has no view assigned",
                            remote_name,
                            btn_idx + 1
                        ));
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
                if idx > 0 {
                    ui::separator();
                }
                let checkbox_id = REMOTE_CHECKBOX_BASE_ID.saturating_add(idx as u8);
                ui::switch(
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
        ui::label("Button -> View mapping");

        if self.remote_definitions.is_empty() {
            ui::label("Add a remote to configure mappings.");
        } else {
            if self.available_views.is_empty() {
                ui::label("No views available.");
            }
            if self.available_scenes.is_empty() {
                ui::label("No scenes available for speed control.");
            }
            for (remote_idx, remote) in self.remote_definitions.iter().enumerate() {
                if remote_idx > 0 {
                    ui::separator();
                }
                ui::label(&remote.name);
                ui::begin_vertical();
                for button_index in 0..MAX_REMOTE_BUTTONS {
                    if let Some(view_button_id) =
                        Self::view_selector_button_id(remote_idx, button_index)
                    {
                        ui::begin_horizontal();
                        ui::label(&format!("Button {}", button_index + 1));
                        let view_button_label = remote.button_view_ids[button_index]
                            .and_then(|view_id| {
                                self.view_info(view_id).map(|info| info.button_label())
                            })
                            .unwrap_or_else(|| "Unassigned".to_string());
                        ui::button(&view_button_label, view_button_id);

                        if self.view_selector_open == Some((remote_idx, button_index)) {
                            ui::begin_vertical();
                            for (view_idx, view_info) in self.available_views.iter().enumerate() {
                                if view_idx
                                    > (SPEED_SCENE_SELECT_BUTTON_BASE_ID
                                        - VIEW_SELECT_OPTION_BASE_ID
                                        - 1) as usize
                                {
                                    ui::label("View list truncated");
                                    break;
                                }

                                let option_id = VIEW_SELECT_OPTION_BASE_ID + view_idx as u8;
                                ui::button(&view_info.option_label(), option_id);
                            }
                            ui::end_vertical();
                        }

                        if let Some(speed_cycle_button_id) =
                            Self::speed_modifier_selector_button_id(remote_idx, button_index)
                        {
                            let cycle_label =
                                remote.button_speed_cycle_start[button_index].as_str();
                            ui::button(
                                &format!("Cycle start: {}", cycle_label),
                                speed_cycle_button_id,
                            );

                            if self.speed_modifier_selector_open == Some((remote_idx, button_index))
                            {
                                ui::begin_vertical();
                                for (modifier_idx, modifier) in
                                    AnimationSpeedModifier::ALL.iter().enumerate()
                                {
                                    if modifier_idx
                                        > (WIZARD_RECORD_BASE_ID
                                            - SPEED_MODIFIER_SELECT_OPTION_BASE_ID
                                            - 1) as usize
                                    {
                                        ui::label("Speed list truncated");
                                        break;
                                    }

                                    let option_id =
                                        SPEED_MODIFIER_SELECT_OPTION_BASE_ID + modifier_idx as u8;
                                    ui::button(&format!("Speed {}", modifier.as_str()), option_id);
                                }
                                ui::end_vertical();
                            }
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
                    PluginUiEvent::Switch { id, value } => {
                        let idx = id.saturating_sub(REMOTE_CHECKBOX_BASE_ID) as usize;
                        if id >= REMOTE_CHECKBOX_BASE_ID && idx < self.remote_enabled.len() {
                            println!("Remote {} enabled -> {}", idx, value);
                            self.remote_enabled[idx] = value;
                            self.save_state();
                        }
                    }
                    PluginUiEvent::Checkbox { id, checked } => {
                        let idx = id.saturating_sub(REMOTE_CHECKBOX_BASE_ID) as usize;
                        if id >= REMOTE_CHECKBOX_BASE_ID && idx < self.remote_enabled.len() {
                            println!("Remote {} enabled -> {}", idx, checked);
                            self.remote_enabled[idx] = checked;
                            self.save_state();
                        }
                    }
                    PluginUiEvent::Button { id } => match id {
                        ADD_REMOTE_BUTTON_ID => {
                            if self.wizard.is_none() {
                                self.wizard = Some(RemoteWizard::new());
                            }
                        }
                        WIZARD_SAVE_BUTTON_ID => {
                            if let Some(remote) = self.wizard.as_ref().and_then(|wizard| {
                                wizard.build_remote(self.remote_definitions.len())
                            }) {
                                let remote_name = remote.name.clone();
                                self.remote_definitions.push(remote);
                                self.remote_enabled.push(true);
                                self.ensure_view_assignments();
                                self.view_selector_open = None;
                                self.speed_selector_open = None;
                                self.speed_modifier_selector_open = None;
                                self.save_state();
                                self.push_log(format!("Added remote {}", remote_name));
                                self.wizard = None;
                            } else if let Some(current) = self.wizard.as_mut() {
                                current.status_message =
                                    Some("Record all buttons before saving".to_string());
                            }
                        }
                        WIZARD_CANCEL_BUTTON_ID => {
                            self.wizard = None;
                        }
                        _ if id >= WIZARD_RECORD_BASE_ID
                            && id < WIZARD_RECORD_BASE_ID + MAX_REMOTE_BUTTONS as u8 =>
                        {
                            if let Some(wizard) = self.wizard.as_mut() {
                                let button_index = (id - WIZARD_RECORD_BASE_ID) as usize;
                                wizard.set_pending_button(button_index);
                            }
                        }
                        _ => {
                            if let Some((remote_idx, button_idx)) =
                                Self::decode_view_selector_button(id)
                            {
                                if remote_idx < self.remote_definitions.len()
                                    && button_idx < MAX_REMOTE_BUTTONS
                                {
                                    if self.view_selector_open == Some((remote_idx, button_idx)) {
                                        self.view_selector_open = None;
                                    } else {
                                        self.view_selector_open = Some((remote_idx, button_idx));
                                    }
                                }
                            } else if let Some(view_idx) = Self::decode_view_option(id) {
                                if let Some((remote_idx, button_idx)) =
                                    self.view_selector_open.take()
                                {
                                    if view_idx < self.available_views.len()
                                        && remote_idx < self.remote_definitions.len()
                                    {
                                        let view_info = self.available_views[view_idx].clone();
                                        let mut remote_name = None;
                                        if let Some(remote) =
                                            self.remote_definitions.get_mut(remote_idx)
                                        {
                                            if button_idx < MAX_REMOTE_BUTTONS {
                                                remote.button_view_ids[button_idx] =
                                                    Some(view_info.id);
                                                remote_name = Some(remote.name.clone());
                                            }
                                        }

                                        if let Some(remote_name) = remote_name {
                                            self.push_log(format!(
                                                "{} button {} mapped to {}",
                                                remote_name,
                                                button_idx + 1,
                                                view_info.option_label()
                                            ));
                                            self.save_state();
                                        }
                                    } else {
                                        self.push_log("View selection out of range");
                                    }
                                }
                            } else if let Some((remote_idx, button_idx)) =
                                Self::decode_speed_scene_selector_button(id)
                            {
                                if remote_idx < self.remote_definitions.len()
                                    && button_idx < MAX_REMOTE_BUTTONS
                                {
                                    if self.speed_selector_open == Some((remote_idx, button_idx)) {
                                        self.speed_selector_open = None;
                                    } else {
                                        self.speed_selector_open = Some((remote_idx, button_idx));
                                    }
                                }
                            } else if let Some((remote_idx, button_idx)) =
                                Self::decode_speed_modifier_selector_button(id)
                            {
                                if remote_idx < self.remote_definitions.len()
                                    && button_idx < MAX_REMOTE_BUTTONS
                                {
                                    if self.speed_modifier_selector_open
                                        == Some((remote_idx, button_idx))
                                    {
                                        self.speed_modifier_selector_open = None;
                                    } else {
                                        self.speed_modifier_selector_open =
                                            Some((remote_idx, button_idx));
                                    }
                                }
                            } else if let Some(modifier_idx) =
                                Self::decode_speed_modifier_option(id)
                            {
                                if let Some((remote_idx, button_idx)) =
                                    self.speed_modifier_selector_open.take()
                                {
                                    if remote_idx < self.remote_definitions.len()
                                        && button_idx < MAX_REMOTE_BUTTONS
                                    {
                                        if let Some(modifier) =
                                            AnimationSpeedModifier::ALL.get(modifier_idx)
                                        {
                                            let mut remote_name = None;
                                            if let Some(remote) =
                                                self.remote_definitions.get_mut(remote_idx)
                                            {
                                                remote.button_speed_cycle_start[button_idx] =
                                                    *modifier;
                                                remote_name = Some(remote.name.clone());
                                            }

                                            if let Some(remote_name) = remote_name {
                                                self.push_log(format!(
                                                    "{} button {} cycle start -> {}",
                                                    remote_name,
                                                    button_idx + 1,
                                                    modifier.as_str()
                                                ));
                                                self.save_state();
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    },
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
        if scenes != self.available_scenes {
            self.available_scenes = scenes;
        }

        let scene_speeds = state
            .scenes
            .iter()
            .map(|(scene_id, scene)| (*scene_id, scene.sink.master_speed))
            .collect::<Vec<_>>();
        if scene_speeds != self.scene_speeds {
            self.scene_speeds = scene_speeds;
        }

        let view_infos = state
            .views
            .iter()
            .map(|(view_id, view)| {
                let base_scene_name = state
                    .scenes
                    .get(&view.base_scene)
                    .map(|scene| scene.name.clone());
                let overlay_names = view
                    .overlays
                    .iter()
                    .map(|overlay_id| {
                        (
                            *overlay_id,
                            state.scenes.get(overlay_id).map(|scene| scene.name.clone()),
                        )
                    })
                    .collect();
                ViewInfo {
                    id: *view_id,
                    view: view.clone(),
                    base_scene_name,
                    overlay_names,
                }
            })
            .collect::<Vec<_>>();
        // Preserve ordering from engine state (already sorted by view id)
        if view_infos != self.available_views {
            self.available_views = view_infos;
            self.ensure_view_assignments();
        }

        self.clear_invalid_view_selector();
        self.clear_invalid_speed_selector();
        self.clear_invalid_speed_modifier_selector();

        for ev in self.conn.poll() {
            let str = String::from_utf8_lossy(&ev.body);

            if str.starts_with("r: ") {
                let num = str.split("r: ").nth(1).unwrap_or("").trim();
                println!("P: `{num}`: {:?}", num.as_bytes());
                match num.parse::<u32>() {
                    Ok(n) => self.handle_signal(n, input.clock),
                    Err(err) => {
                        println!("could not parse signal `{num}`: {err}");
                    }
                }
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
