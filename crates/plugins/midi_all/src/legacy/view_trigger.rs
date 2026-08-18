use std::collections::BTreeMap;

use blaulicht_plugin_framework::{self as bpf, ui, MidiConnection};
use blaulicht_plugin_framework::prelude::println;
use blaulicht_shared::{ControlEvent, ControlEventMessage, PluginUiEvent};
use serde::{Deserialize, Serialize};

use crate::legacy::LegacyState;

// LED color indices for the APC mini mk2. Existing code already uses 10
// (active scene) and 20 (intensity indicator), so pick distinct values.
pub const LED_DIM: u8 = 3;
pub const LED_BRIGHT: u8 = 21;
pub const LED_OFF: u8 = 0;
pub const APC_LED_STATUS: u8 = 0x96;

// Stable u8 UI element IDs. Range chosen to avoid colliding with the
// existing Fans switch which uses id 0 in legacy/mod.rs.
const TRIGGER_DROPDOWN_BASE_ID: u8 = 50; // up to 32 triggers -> 50..82
const TRIGGER_DELETE_BASE_ID: u8 = 82; //   up to 32 triggers -> 82..114
const ADD_TRIGGER_BUTTON_ID: u8 = 120;
const WIZARD_CANCEL_BUTTON_ID: u8 = 121;
const WIZARD_SAVE_BUTTON_ID: u8 = 122;
const WIZARD_DROPDOWN_BUTTON_ID: u8 = 123;
const VIEW_OPTION_BASE_ID: u8 = 130; // 130..=255

const MAX_TRIGGERS: usize = 32;

#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub struct ViewTrigger {
    pub pad: u8,
    pub view_id: u8,
}

#[derive(Serialize, Deserialize, Default)]
pub struct PersistedViewTriggers {
    pub triggers: Vec<ViewTrigger>,
}

pub enum ViewTriggerWizard {
    WaitingForPress,
    PadCaptured {
        pad: u8,
        selected_view: Option<u8>,
    },
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DropdownOpen {
    None,
    Wizard,
    Trigger(usize),
}

impl Default for DropdownOpen {
    fn default() -> Self {
        DropdownOpen::None
    }
}

// Reserved-pad detection. Mirrors the constants used elsewhere in the
// plugin so the wizard can refuse to capture pads that are already
// claimed by another mapping.
const SCENES: [u8; 8] = [56, 48, 40, 32, 24, 16, 8, 0];
const SCENES_INT: [u8; 5] = [60, 52, 44, 36, 28];
const VIDEO_PADS: [u8; 8] = [62, 54, 46, 38, 30, 22, 14, 6];
const PAGE_PADS: [u8; 8] = [63, 55, 47, 39, 31, 23, 15, 7];

pub fn is_reserved_pad(pad: u8) -> bool {
    SCENES.contains(&pad)
        || SCENES_INT.contains(&pad)
        || VIDEO_PADS.contains(&pad)
        || PAGE_PADS.contains(&pad)
}

impl LegacyState {
    pub fn load_view_triggers(&mut self) {
        if let Some(json) = bpf::load_plugin_state(bpf::PluginStateLocation::Showfile) {
            match serde_json::from_str::<PersistedViewTriggers>(&json) {
                Ok(state) => {
                    self.view_triggers = state.triggers;
                    println!("[view_trigger] loaded {} triggers", self.view_triggers.len());
                }
                Err(e) => {
                    println!("[view_trigger] failed to deserialize state: {e}");
                }
            }
        }
    }

    pub fn save_view_triggers(&self) {
        let state = PersistedViewTriggers {
            triggers: self.view_triggers.clone(),
        };
        match serde_json::to_string(&state) {
            Ok(json) => bpf::save_plugin_state(bpf::PluginStateLocation::Showfile, &json),
            Err(e) => println!("[view_trigger] failed to serialize state: {e}"),
        }
    }

    /// Handle an APC mini button press `(144, pad, 127)`. Returns `true`
    /// if the press was consumed (by the wizard or by an assigned
    /// trigger) so the caller should skip the legacy match arms.
    pub fn handle_view_trigger_press(&mut self, pad: u8) -> bool {
        // Wizard capture takes priority.
        if matches!(self.view_trigger_wizard, Some(ViewTriggerWizard::WaitingForPress)) {
            if is_reserved_pad(pad) {
                self.push_view_trigger_log(format!(
                    "Pad {pad} is reserved by another mapping — pick another"
                ));
                return true;
            }
            if self.view_triggers.iter().any(|t| t.pad == pad) {
                self.push_view_trigger_log(format!(
                    "Pad {pad} already has a trigger — delete it first"
                ));
                return true;
            }
            self.view_trigger_wizard = Some(ViewTriggerWizard::PadCaptured {
                pad,
                selected_view: None,
            });
            self.view_trigger_dropdown_open = DropdownOpen::None;
            self.push_view_trigger_log(format!("Captured pad {pad}"));
            return true;
        }

        // Assigned trigger fires the view.
        if let Some(trigger) = self.view_triggers.iter().find(|t| t.pad == pad).cloned() {
            let dmx = bpf::get_dmx();
            if let Some(view) = dmx.views.get(&trigger.view_id).cloned() {
                bpf::send_event(ControlEvent::Transaction(vec![
                    ControlEvent::SetSceneFocus(view.base_scene),
                    ControlEvent::SetOverlays(view.overlays.clone()),
                ]));
            } else {
                println!(
                    "[view_trigger] pad {pad} assigned to missing view #{}",
                    trigger.view_id
                );
            }
            self.last_pressed_view_pad = Some(pad);
            return true;
        }

        false
    }

    /// Diff-sync the APC LEDs for view-trigger pads. Only sends MIDI
    /// messages for pads whose color changed since the last call.
    pub fn sync_view_trigger_leds(&mut self, conn: &MidiConnection) {
        let mut desired: BTreeMap<u8, u8> = BTreeMap::new();
        for t in &self.view_triggers {
            desired.insert(t.pad, LED_DIM);
        }
        if let Some(pad) = self.last_pressed_view_pad {
            if desired.contains_key(&pad) {
                desired.insert(pad, LED_BRIGHT);
            }
        }

        // Clear pads that were lit before but aren't any more.
        for pad in self.view_trigger_lit_colors.keys() {
            if !desired.contains_key(pad) {
                conn.send(APC_LED_STATUS, *pad, LED_OFF);
            }
        }
        // Send updates for pads whose color changed (or weren't lit before).
        for (pad, color) in &desired {
            if self.view_trigger_lit_colors.get(pad) != Some(color) {
                conn.send(APC_LED_STATUS, *pad, *color);
            }
        }
        self.view_trigger_lit_colors = desired;
    }

    /// Render the view-trigger UI section and process incoming
    /// `PluginUi` events targeted at this plugin.
    pub fn render_view_trigger_ui(
        &mut self,
        events: &[ControlEventMessage],
        plugin_id: u8,
    ) {
        // Refresh the cached list of views from the engine each tick.
        let dmx = bpf::get_dmx();
        self.available_view_ids = dmx
            .views
            .iter()
            .map(|(id, v)| (*id, v.name.clone()))
            .collect();

        ui::separator();
        ui::label("View triggers");

        if self.view_triggers.is_empty() {
            ui::label("No view triggers configured.");
        } else {
            for (idx, trigger) in self.view_triggers.clone().iter().enumerate() {
                if idx >= MAX_TRIGGERS {
                    ui::label("Trigger list truncated");
                    break;
                }
                ui::begin_horizontal();
                let view_label = self
                    .available_view_ids
                    .iter()
                    .find(|(id, _)| *id == trigger.view_id)
                    .map(|(id, name)| format!("{name} (#{id})"))
                    .unwrap_or_else(|| format!("View #{} (missing)", trigger.view_id));
                ui::label(&format!("Pad {} -> {}", trigger.pad, view_label));
                ui::button("Change", TRIGGER_DROPDOWN_BASE_ID + idx as u8);
                ui::button("Delete", TRIGGER_DELETE_BASE_ID + idx as u8);
                ui::end_horizontal();

                if self.view_trigger_dropdown_open == DropdownOpen::Trigger(idx) {
                    self.render_view_options();
                }
            }
        }

        ui::separator();

        match &self.view_trigger_wizard {
            None => {
                ui::button("+ Add view trigger", ADD_TRIGGER_BUTTON_ID);
            }
            Some(ViewTriggerWizard::WaitingForPress) => {
                ui::label("Press an unused APC pad…");
                ui::button("Cancel", WIZARD_CANCEL_BUTTON_ID);
            }
            Some(ViewTriggerWizard::PadCaptured { pad, selected_view }) => {
                ui::label(&format!("Captured pad: {pad}"));
                let dropdown_label = selected_view
                    .and_then(|id| {
                        self.available_view_ids
                            .iter()
                            .find(|(vid, _)| *vid == id)
                            .map(|(vid, name)| format!("{name} (#{vid})"))
                    })
                    .unwrap_or_else(|| "Pick a view".to_string());
                ui::begin_horizontal();
                ui::button(&dropdown_label, WIZARD_DROPDOWN_BUTTON_ID);
                ui::button("Save", WIZARD_SAVE_BUTTON_ID);
                ui::button("Cancel", WIZARD_CANCEL_BUTTON_ID);
                ui::end_horizontal();

                if self.view_trigger_dropdown_open == DropdownOpen::Wizard {
                    self.render_view_options();
                }
            }
        }

        if !self.view_trigger_log.is_empty() {
            ui::separator();
            for line in &self.view_trigger_log {
                ui::label(line);
            }
        }

        self.process_view_trigger_ui_events(events, plugin_id);
    }

    fn render_view_options(&self) {
        ui::begin_vertical();
        if self.available_view_ids.is_empty() {
            ui::label("No views available.");
        } else {
            let max_options = (u8::MAX - VIEW_OPTION_BASE_ID) as usize + 1;
            for (idx, (vid, name)) in self.available_view_ids.iter().enumerate() {
                if idx >= max_options {
                    ui::label("View list truncated");
                    break;
                }
                ui::button(
                    &format!("{name} (#{vid})"),
                    VIEW_OPTION_BASE_ID + idx as u8,
                );
            }
        }
        ui::end_vertical();
    }

    fn process_view_trigger_ui_events(
        &mut self,
        events: &[ControlEventMessage],
        plugin_id: u8,
    ) {
        for event in events {
            let (ui_event, ev_plugin_id) = match event.body() {
                ControlEvent::PluginUi(e, p) => (e, p),
                _ => continue,
            };
            if ev_plugin_id != plugin_id {
                continue;
            }
            let id = match ui_event {
                PluginUiEvent::Button { id } => id,
                _ => continue,
            };

            match id {
                ADD_TRIGGER_BUTTON_ID => {
                    self.view_trigger_wizard = Some(ViewTriggerWizard::WaitingForPress);
                    self.view_trigger_dropdown_open = DropdownOpen::None;
                    self.push_view_trigger_log("Waiting for pad press…".to_string());
                }
                WIZARD_CANCEL_BUTTON_ID => {
                    self.view_trigger_wizard = None;
                    self.view_trigger_dropdown_open = DropdownOpen::None;
                }
                WIZARD_SAVE_BUTTON_ID => {
                    if let Some(ViewTriggerWizard::PadCaptured { pad, selected_view }) =
                        &self.view_trigger_wizard
                    {
                        if let Some(view_id) = selected_view {
                            if self.view_triggers.len() >= MAX_TRIGGERS {
                                self.push_view_trigger_log(format!(
                                    "Trigger limit ({MAX_TRIGGERS}) reached"
                                ));
                            } else {
                                self.view_triggers.push(ViewTrigger {
                                    pad: *pad,
                                    view_id: *view_id,
                                });
                                self.save_view_triggers();
                                self.push_view_trigger_log(format!(
                                    "Saved trigger: pad {pad} -> view #{view_id}"
                                ));
                                self.view_trigger_wizard = None;
                                self.view_trigger_dropdown_open = DropdownOpen::None;
                            }
                        } else {
                            self.push_view_trigger_log("Pick a view first".to_string());
                        }
                    }
                }
                WIZARD_DROPDOWN_BUTTON_ID => {
                    self.view_trigger_dropdown_open =
                        if self.view_trigger_dropdown_open == DropdownOpen::Wizard {
                            DropdownOpen::None
                        } else {
                            DropdownOpen::Wizard
                        };
                }
                id if id >= TRIGGER_DROPDOWN_BASE_ID && id < TRIGGER_DELETE_BASE_ID => {
                    let idx = (id - TRIGGER_DROPDOWN_BASE_ID) as usize;
                    if idx < self.view_triggers.len() {
                        self.view_trigger_dropdown_open =
                            if self.view_trigger_dropdown_open == DropdownOpen::Trigger(idx) {
                                DropdownOpen::None
                            } else {
                                DropdownOpen::Trigger(idx)
                            };
                    }
                }
                id if id >= TRIGGER_DELETE_BASE_ID && id < ADD_TRIGGER_BUTTON_ID => {
                    let idx = (id - TRIGGER_DELETE_BASE_ID) as usize;
                    if idx < self.view_triggers.len() {
                        let removed = self.view_triggers.remove(idx);
                        if self.last_pressed_view_pad == Some(removed.pad) {
                            self.last_pressed_view_pad = None;
                        }
                        if self.view_trigger_dropdown_open == DropdownOpen::Trigger(idx) {
                            self.view_trigger_dropdown_open = DropdownOpen::None;
                        }
                        self.save_view_triggers();
                        self.push_view_trigger_log(format!(
                            "Deleted trigger for pad {}",
                            removed.pad
                        ));
                    }
                }
                id if id >= VIEW_OPTION_BASE_ID => {
                    let opt_idx = (id - VIEW_OPTION_BASE_ID) as usize;
                    if let Some((view_id, _name)) =
                        self.available_view_ids.get(opt_idx).cloned()
                    {
                        match self.view_trigger_dropdown_open {
                            DropdownOpen::Wizard => {
                                if let Some(ViewTriggerWizard::PadCaptured {
                                    selected_view, ..
                                }) = &mut self.view_trigger_wizard
                                {
                                    *selected_view = Some(view_id);
                                }
                                self.view_trigger_dropdown_open = DropdownOpen::None;
                            }
                            DropdownOpen::Trigger(idx) => {
                                let pad = if let Some(trigger) =
                                    self.view_triggers.get_mut(idx)
                                {
                                    trigger.view_id = view_id;
                                    Some(trigger.pad)
                                } else {
                                    None
                                };
                                if let Some(pad) = pad {
                                    self.save_view_triggers();
                                    self.push_view_trigger_log(format!(
                                        "Reassigned pad {pad} to view #{view_id}"
                                    ));
                                }
                                self.view_trigger_dropdown_open = DropdownOpen::None;
                            }
                            DropdownOpen::None => {}
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn push_view_trigger_log(&mut self, msg: String) {
        println!("[view_trigger] {msg}");
        self.view_trigger_log.push_back(msg);
        while self.view_trigger_log.len() > 5 {
            self.view_trigger_log.pop_front();
        }
    }
}
