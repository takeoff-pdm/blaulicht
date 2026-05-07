use std::collections::VecDeque;

use blaulicht_plugin_framework::MidiEvent;
use blaulicht_plugin_framework::{self as bpf, println, ui, MidiConnection};
use blaulicht_shared::{
    view::View, AnimationSpeedModifier, ControlEvent, ControlEventMessage, EngineState,
    PluginUiEvent, TickInput,
};
use map_range::MapRange;
use serde::{Deserialize, Serialize};

const COUNT_FADERS: usize = 2;

const FADER_STATUS_BYTES: [u8; COUNT_FADERS] = [0xB0, 0xB1];
const FADER_CC: u8 = 0x13;
const KNOB_STATUS: u8 = 0xB6;
const KNOB_CC_BYTES: [u8; COUNT_FADERS] = [0x17, 0x18];

const VIEW_SELECT_BUTTON_BASE_ID: u8 = 10;
const VIEW_SELECT_OPTION_BASE_ID: u8 = 80;

#[derive(Clone, Copy, PartialEq)]
enum Deck {
    Left,
    Right,
}

impl Deck {
    fn note_status(self) -> u8 {
        match self {
            Deck::Left => 0x90,
            Deck::Right => 0x91,
        }
    }

    fn pad_status(self) -> u8 {
        match self {
            Deck::Left => 0x97,
            Deck::Right => 0x99,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum ViewButtonKind {
    Pad(u8),
    Cue,
    LoopIn4Beat,
    LoopOut,
    ReloopExit,
    CueLoopPrevious,
    CueLoopNext,
}

impl ViewButtonKind {
    fn matches(self, deck: Deck, status: u8, kind: u8) -> bool {
        match self {
            ViewButtonKind::Pad(pad) => {
                status == deck.pad_status()
                    && (kind == pad.saturating_sub(1) || kind == 0x60 + pad.saturating_sub(1))
            }
            ViewButtonKind::Cue => status == deck.note_status() && kind == 0x0C,
            ViewButtonKind::LoopIn4Beat => status == deck.note_status() && kind == 0x10,
            ViewButtonKind::LoopOut => status == deck.note_status() && kind == 0x11,
            ViewButtonKind::ReloopExit => status == deck.note_status() && kind == 0x4D,
            ViewButtonKind::CueLoopPrevious => status == deck.note_status() && kind == 0x51,
            ViewButtonKind::CueLoopNext => status == deck.note_status() && kind == 0x53,
        }
    }
}

#[derive(Clone, Copy)]
struct ViewButtonDefinition {
    label: &'static str,
    deck: Deck,
    kind: ViewButtonKind,
}

impl ViewButtonDefinition {
    fn matches(&self, status: u8, kind: u8) -> bool {
        self.kind.matches(self.deck, status, kind)
    }
}

const VIEW_BUTTONS: [ViewButtonDefinition; 28] = [
    ViewButtonDefinition {
        label: "Deck 1 Pad 1",
        deck: Deck::Left,
        kind: ViewButtonKind::Pad(1),
    },
    ViewButtonDefinition {
        label: "Deck 1 Pad 2",
        deck: Deck::Left,
        kind: ViewButtonKind::Pad(2),
    },
    ViewButtonDefinition {
        label: "Deck 1 Pad 3",
        deck: Deck::Left,
        kind: ViewButtonKind::Pad(3),
    },
    ViewButtonDefinition {
        label: "Deck 1 Pad 4",
        deck: Deck::Left,
        kind: ViewButtonKind::Pad(4),
    },
    ViewButtonDefinition {
        label: "Deck 1 Pad 5",
        deck: Deck::Left,
        kind: ViewButtonKind::Pad(5),
    },
    ViewButtonDefinition {
        label: "Deck 1 Pad 6",
        deck: Deck::Left,
        kind: ViewButtonKind::Pad(6),
    },
    ViewButtonDefinition {
        label: "Deck 1 Pad 7",
        deck: Deck::Left,
        kind: ViewButtonKind::Pad(7),
    },
    ViewButtonDefinition {
        label: "Deck 1 Pad 8",
        deck: Deck::Left,
        kind: ViewButtonKind::Pad(8),
    },
    ViewButtonDefinition {
        label: "Deck 1 Cue",
        deck: Deck::Left,
        kind: ViewButtonKind::Cue,
    },
    ViewButtonDefinition {
        label: "Deck 2 Pad 1",
        deck: Deck::Right,
        kind: ViewButtonKind::Pad(1),
    },
    ViewButtonDefinition {
        label: "Deck 2 Pad 2",
        deck: Deck::Right,
        kind: ViewButtonKind::Pad(2),
    },
    ViewButtonDefinition {
        label: "Deck 2 Pad 3",
        deck: Deck::Right,
        kind: ViewButtonKind::Pad(3),
    },
    ViewButtonDefinition {
        label: "Deck 2 Pad 4",
        deck: Deck::Right,
        kind: ViewButtonKind::Pad(4),
    },
    ViewButtonDefinition {
        label: "Deck 2 Pad 5",
        deck: Deck::Right,
        kind: ViewButtonKind::Pad(5),
    },
    ViewButtonDefinition {
        label: "Deck 2 Pad 6",
        deck: Deck::Right,
        kind: ViewButtonKind::Pad(6),
    },
    ViewButtonDefinition {
        label: "Deck 2 Pad 7",
        deck: Deck::Right,
        kind: ViewButtonKind::Pad(7),
    },
    ViewButtonDefinition {
        label: "Deck 2 Pad 8",
        deck: Deck::Right,
        kind: ViewButtonKind::Pad(8),
    },
    ViewButtonDefinition {
        label: "Deck 2 Cue",
        deck: Deck::Right,
        kind: ViewButtonKind::Cue,
    },
    ViewButtonDefinition {
        label: "Deck 1 Loop In/4Beat",
        deck: Deck::Left,
        kind: ViewButtonKind::LoopIn4Beat,
    },
    ViewButtonDefinition {
        label: "Deck 1 Loop Out",
        deck: Deck::Left,
        kind: ViewButtonKind::LoopOut,
    },
    ViewButtonDefinition {
        label: "Deck 1 Reloop/Exit",
        deck: Deck::Left,
        kind: ViewButtonKind::ReloopExit,
    },
    ViewButtonDefinition {
        label: "Deck 1 Cue/Loop Call <",
        deck: Deck::Left,
        kind: ViewButtonKind::CueLoopPrevious,
    },
    ViewButtonDefinition {
        label: "Deck 1 Cue/Loop Call >",
        deck: Deck::Left,
        kind: ViewButtonKind::CueLoopNext,
    },
    ViewButtonDefinition {
        label: "Deck 2 Loop In/4Beat",
        deck: Deck::Right,
        kind: ViewButtonKind::LoopIn4Beat,
    },
    ViewButtonDefinition {
        label: "Deck 2 Loop Out",
        deck: Deck::Right,
        kind: ViewButtonKind::LoopOut,
    },
    ViewButtonDefinition {
        label: "Deck 2 Reloop/Exit",
        deck: Deck::Right,
        kind: ViewButtonKind::ReloopExit,
    },
    ViewButtonDefinition {
        label: "Deck 2 Cue/Loop Call <",
        deck: Deck::Right,
        kind: ViewButtonKind::CueLoopPrevious,
    },
    ViewButtonDefinition {
        label: "Deck 2 Cue/Loop Call >",
        deck: Deck::Right,
        kind: ViewButtonKind::CueLoopNext,
    },
];

#[derive(Serialize, Deserialize)]
struct PersistedState {
    button_view_ids: Vec<Option<u8>>,
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
            "{} (#{}) - base {} - overlays {}",
            self.view.name,
            self.id,
            self.base_label(),
            self.overlays_label()
        )
    }
}

pub struct DDJSubSystem {
    midi_handle: MidiConnection,
    last_sync: u32,

    fader_vals: [u8; COUNT_FADERS],
    fader_vals_updated: [bool; COUNT_FADERS],

    knob_vals: [u8; COUNT_FADERS],
    knob_vals_updated: [bool; COUNT_FADERS],

    dmx: EngineState,
    available_views: Vec<ViewInfo>,
    button_view_ids: Vec<Option<u8>>,
    view_selector_open: Option<usize>,
    log: VecDeque<String>,
}

impl Default for DDJSubSystem {
    fn default() -> Self {
        Self {
            midi_handle: unsafe { MidiConnection::dummy() },
            last_sync: 0,

            fader_vals: [0; COUNT_FADERS],
            fader_vals_updated: [false; COUNT_FADERS],

            knob_vals: [0; COUNT_FADERS],
            knob_vals_updated: [false; COUNT_FADERS],

            dmx: EngineState::default(),
            available_views: vec![],
            button_view_ids: vec![None; VIEW_BUTTONS.len()],
            view_selector_open: None,
            log: VecDeque::new(),
        }
    }
}

impl DDJSubSystem {
    pub fn init(&mut self) {
        self.load_state();

        println!("[DDJ_400] initializing...");

        let name = "DDJ-400";
        let midi_handle = MidiConnection::open(name).unwrap();
        println!(
            "Got MIDI handle to device! HANDLE ID: {}",
            midi_handle.get_meta().device_id
        );

        self.midi_handle = midi_handle;

        println!("[DDJ_400] done.");
    }

    pub fn run(&mut self, input: TickInput) {
        self.sync(input.clock);

        let res = self.midi_handle.poll();
        self.midi_in(res);

        for i in 0..COUNT_FADERS {
            self.handle_fader_input(i);
            self.handle_knob_input(i);
        }

        self.draw_ui(&input.events.events, input.id);
    }
}

impl DDJSubSystem {
    fn save_state(&self) {
        if let Ok(json) = serde_json::to_string(&PersistedState {
            button_view_ids: self.button_view_ids.clone(),
        }) {
            bpf::save_plugin_state(bpf::PluginStateLocation::Showfile, &json);
        }
    }

    fn load_state(&mut self) {
        if let Some(json) = bpf::load_plugin_state(bpf::PluginStateLocation::Showfile) {
            if let Ok(saved) = serde_json::from_str::<PersistedState>(&json) {
                self.button_view_ids = saved.button_view_ids;
            }
        }

        self.normalize_button_view_ids();
    }

    fn normalize_button_view_ids(&mut self) -> bool {
        let mut changed = false;

        if self.button_view_ids.len() < VIEW_BUTTONS.len() {
            self.button_view_ids.resize(VIEW_BUTTONS.len(), None);
            changed = true;
        } else if self.button_view_ids.len() > VIEW_BUTTONS.len() {
            self.button_view_ids.truncate(VIEW_BUTTONS.len());
            changed = true;
        }

        changed
    }

    fn push_log(&mut self, message: impl Into<String>) {
        self.log.push_back(message.into());
        while self.log.len() > 5 {
            self.log.pop_front();
        }
    }

    fn view_info(&self, view_id: u8) -> Option<&ViewInfo> {
        self.available_views.iter().find(|info| info.id == view_id)
    }

    fn ensure_view_assignments(&mut self) {
        let mut changed = self.normalize_button_view_ids();

        for (button_index, slot) in self.button_view_ids.iter_mut().enumerate() {
            if slot.is_none() {
                if let Some(view_info) = self.available_views.get(button_index) {
                    *slot = Some(view_info.id);
                    changed = true;
                }
            }
        }

        if changed {
            self.save_state();
        }
    }

    fn clear_invalid_view_selector(&mut self) {
        if let Some(button_idx) = self.view_selector_open {
            if button_idx >= VIEW_BUTTONS.len() {
                self.view_selector_open = None;
            }
        }
    }

    fn view_selector_button_id(button_idx: usize) -> Option<u8> {
        let max_offset = (VIEW_SELECT_OPTION_BASE_ID - VIEW_SELECT_BUTTON_BASE_ID - 1) as usize;
        if button_idx > max_offset {
            return None;
        }

        Some(VIEW_SELECT_BUTTON_BASE_ID + button_idx as u8)
    }

    fn decode_view_selector_button(id: u8) -> Option<usize> {
        if id < VIEW_SELECT_BUTTON_BASE_ID || id >= VIEW_SELECT_OPTION_BASE_ID {
            return None;
        }

        let button_idx = (id - VIEW_SELECT_BUTTON_BASE_ID) as usize;
        if button_idx >= VIEW_BUTTONS.len() {
            return None;
        }

        Some(button_idx)
    }

    fn decode_view_option(id: u8) -> Option<usize> {
        if id < VIEW_SELECT_OPTION_BASE_ID {
            return None;
        }

        Some((id - VIEW_SELECT_OPTION_BASE_ID) as usize)
    }

    fn midi_in(&mut self, ev: Vec<MidiEvent>) {
        for e in ev {
            match (e.status, e.kind, e.value) {
                (fader_status, FADER_CC, value) if FADER_STATUS_BYTES.contains(&fader_status) => {
                    let index = FADER_STATUS_BYTES
                        .iter()
                        .position(|status| *status == fader_status)
                        .unwrap();
                    self.fader_vals[index] = value;
                    self.fader_vals_updated[index] = true;
                }
                (KNOB_STATUS, knob_cc, value) if KNOB_CC_BYTES.contains(&knob_cc) => {
                    let index = KNOB_CC_BYTES.iter().position(|cc| *cc == knob_cc).unwrap();
                    self.knob_vals[index] = value;
                    self.knob_vals_updated[index] = true;
                }
                (status, kind, value) if value > 0 => {
                    if let Some(button_idx) = self.view_button_for_midi(status, kind) {
                        self.activate_view(button_idx);
                    } else {
                        println!("{}: {:?}", self.midi_handle.get_meta().device_id, e);
                    }
                }
                _ => {}
            }
        }
    }

    fn view_button_for_midi(&self, status: u8, kind: u8) -> Option<usize> {
        VIEW_BUTTONS
            .iter()
            .position(|button| button.matches(status, kind))
    }

    fn activate_view(&mut self, button_idx: usize) {
        let button_label = VIEW_BUTTONS
            .get(button_idx)
            .map(|button| button.label)
            .unwrap_or("Unknown button");

        let Some(view_id) = self.button_view_ids.get(button_idx).copied().flatten() else {
            self.push_log(format!("{button_label} has no view assigned"));
            return;
        };

        let Some(view_info) = self.view_info(view_id).cloned() else {
            self.push_log(format!("{button_label} view #{view_id} unavailable"));
            return;
        };

        self.dmx.current_scene_focus = view_info.view.base_scene;
        self.dmx.current_overlay_scenes = view_info.view.overlays.clone();

        bpf::send_event(ControlEvent::Transaction(vec![
            ControlEvent::SetSceneFocus(view_info.view.base_scene),
            ControlEvent::SetOverlays(view_info.view.overlays.clone()),
        ]));

        self.push_log(format!("{button_label} -> {}", view_info.option_label()));
    }

    fn sync(&mut self, current_time: u32) {
        if current_time - self.last_sync <= 100 {
            return;
        }

        self.last_sync = current_time;

        let dmx = bpf::get_dmx();
        self.dmx = dmx.clone();

        let view_infos = dmx
            .views
            .iter()
            .map(|(view_id, view)| {
                let base_scene_name = dmx
                    .scenes
                    .get(&view.base_scene)
                    .map(|scene| scene.name.clone());
                let overlay_names = view
                    .overlays
                    .iter()
                    .map(|overlay_id| {
                        (
                            *overlay_id,
                            dmx.scenes.get(overlay_id).map(|scene| scene.name.clone()),
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

        if view_infos != self.available_views {
            self.available_views = view_infos;
            self.ensure_view_assignments();
        }

        self.clear_invalid_view_selector();
    }

    fn target_scene_for_control(&self, control_index: usize) -> Option<u8> {
        match control_index {
            0 => Some(self.dmx.current_scene_focus),
            1 => self.dmx.current_overlay_scenes.first().copied(),
            _ => None,
        }
    }

    fn handle_fader_input(&mut self, fader: usize) {
        if !self.fader_vals_updated[fader] {
            return;
        }

        self.fader_vals_updated[fader] = false;

        let alpha = self.fader_vals[fader];
        let alpha = (alpha as u16).map_range(0..127, 0..100) as u8;

        let Some(scene_id) = self.target_scene_for_control(fader) else {
            return;
        };

        bpf::send_event(ControlEvent::SetSceneMasterAlpha(scene_id, alpha));
    }

    fn handle_knob_input(&mut self, knob: usize) {
        if !self.knob_vals_updated[knob] {
            return;
        }

        self.knob_vals_updated[knob] = false;
        let knob_val = self.knob_vals[knob];
        let max_index = AnimationSpeedModifier::ALL.len() as u16 - 1;
        let index = (knob_val as u16).map_range(0..127, 0..max_index);

        let Some(scene_id) = self.target_scene_for_control(knob) else {
            return;
        };

        let speed = AnimationSpeedModifier::from_index(index as usize);

        bpf::send_event(ControlEvent::SetSceneMasterSpeed(scene_id, speed));
    }

    fn draw_ui(&mut self, events: &[ControlEventMessage], plugin_id: u8) {
        ui::begin();

        ui::label("DDJ-400 view activation");

        if self.available_views.is_empty() {
            ui::label("No views available.");
        }

        for (button_idx, button) in VIEW_BUTTONS.iter().enumerate() {
            if button_idx == 0 {
                ui::separator();
                ui::label("Deck 1 cue buttons");
            } else if button_idx == 9 {
                ui::separator();
                ui::label("Deck 2 cue buttons");
            } else if button_idx == 18 {
                ui::separator();
                ui::label("Loop controls");
            }

            let Some(view_button_id) = Self::view_selector_button_id(button_idx) else {
                continue;
            };

            ui::begin_horizontal();
            ui::label(button.label);
            let view_button_label = self
                .button_view_ids
                .get(button_idx)
                .copied()
                .flatten()
                .and_then(|view_id| self.view_info(view_id).map(|info| info.button_label()))
                .unwrap_or_else(|| "Unassigned".to_string());
            ui::button(&view_button_label, view_button_id);

            if self.view_selector_open == Some(button_idx) {
                ui::begin_vertical();
                for (view_idx, view_info) in self.available_views.iter().enumerate() {
                    if view_idx > (u8::MAX - VIEW_SELECT_OPTION_BASE_ID) as usize {
                        ui::label("View list truncated");
                        break;
                    }

                    let option_id = VIEW_SELECT_OPTION_BASE_ID + view_idx as u8;
                    ui::button(&view_info.option_label(), option_id);
                }
                ui::end_vertical();
            }

            ui::end_horizontal();
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

                if let PluginUiEvent::Button { id } = ui_event {
                    if let Some(button_idx) = Self::decode_view_selector_button(id) {
                        if self.view_selector_open == Some(button_idx) {
                            self.view_selector_open = None;
                        } else {
                            self.view_selector_open = Some(button_idx);
                        }
                    } else if let Some(view_idx) = Self::decode_view_option(id) {
                        if let Some(button_idx) = self.view_selector_open.take() {
                            if button_idx < VIEW_BUTTONS.len()
                                && view_idx < self.available_views.len()
                            {
                                let view_info = self.available_views[view_idx].clone();
                                if let Some(slot) = self.button_view_ids.get_mut(button_idx) {
                                    *slot = Some(view_info.id);
                                }
                                self.push_log(format!(
                                    "{} mapped to {}",
                                    VIEW_BUTTONS[button_idx].label,
                                    view_info.option_label()
                                ));
                                self.save_state();
                            } else {
                                self.push_log("View selection out of range");
                            }
                        }
                    }
                }
            }
        }
    }
}
