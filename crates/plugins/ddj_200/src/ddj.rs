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

const VIEW_SELECT_OPTION_BASE_ID: u8 = 80;
const ALLOW_MISSING_DDJ_200: bool = cfg!(debug_assertions) || cfg!(feature = "debug-without-ddj");
const VIEW_BUTTON_COUNT: usize = 16;

// DDJ-200 Play/Pause buttons. Note On, channel 1/2, note 0x0B.
const PLAY_PAUSE_STATUS_BYTES: [u8; 2] = [0x90, 0x91];
const PLAY_PAUSE_KIND: u8 = 0x0B;
const DDJ_CANVAS_ID: u8 = 5;
const DDJ_CANVAS_WIDTH: i32 = 760;
const DDJ_CANVAS_HEIGHT: i32 = 280;
const PAD_EVENT_HIGHLIGHT_MS: u32 = 650;

#[derive(Clone, Copy, PartialEq)]
enum Deck {
    Left,
    Right,
}

impl Deck {
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
}

impl ViewButtonKind {
    fn matches(self, deck: Deck, status: u8, kind: u8) -> bool {
        match self {
            ViewButtonKind::Pad(pad) => {
                status == deck.pad_status()
                    && (kind == pad.saturating_sub(1) || kind == 0x60 + pad.saturating_sub(1))
            }
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

const VIEW_BUTTONS: [ViewButtonDefinition; VIEW_BUTTON_COUNT] = [
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
    midi_handle: Option<MidiConnection>,
    last_sync: u32,

    fader_vals: [u8; COUNT_FADERS],
    fader_vals_updated: [bool; COUNT_FADERS],

    knob_vals: [u8; COUNT_FADERS],
    knob_vals_updated: [bool; COUNT_FADERS],

    dmx: EngineState,
    available_views: Vec<ViewInfo>,
    button_view_ids: Vec<Option<u8>>,
    view_selector_open: Option<usize>,
    pad_event_times: [Option<u32>; VIEW_BUTTON_COUNT],
    current_time: u32,
    log: VecDeque<String>,
}

impl Default for DDJSubSystem {
    fn default() -> Self {
        Self {
            midi_handle: None,
            last_sync: 0,

            fader_vals: [0; COUNT_FADERS],
            fader_vals_updated: [false; COUNT_FADERS],

            knob_vals: [0; COUNT_FADERS],
            knob_vals_updated: [false; COUNT_FADERS],

            dmx: EngineState::default(),
            available_views: vec![],
            button_view_ids: vec![None; VIEW_BUTTONS.len()],
            view_selector_open: None,
            pad_event_times: [None; VIEW_BUTTON_COUNT],
            current_time: 0,
            log: VecDeque::new(),
        }
    }
}

impl DDJSubSystem {
    pub fn init(&mut self) {
        self.load_state();

        println!("[DDJ_200] initializing...");

        let name = "DDJ-200";
        match MidiConnection::open(name) {
            Ok(midi_handle) => {
                println!(
                    "Got MIDI handle to device! HANDLE ID: {}",
                    midi_handle.get_meta().device_id
                );
                self.midi_handle = Some(midi_handle);
            }
            Err(err) if ALLOW_MISSING_DDJ_200 => {
                println!("[DDJ_200] MIDI device unavailable, continuing without hardware: {err}");
                self.push_log("DDJ-200 not connected; UI debug mode active");
            }
            Err(err) => {
                panic!("DDJ-200 MIDI device unavailable: {err}");
            }
        }

        println!("[DDJ_200] done.");
    }

    pub fn run(&mut self, input: TickInput) {
        self.current_time = input.clock;
        self.sync(input.clock);

        if let Some(midi_handle) = self.midi_handle {
            let res = midi_handle.poll();
            self.midi_in(res, input.clock);
        }

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

        if self.normalize_button_view_ids() {
            self.push_log("Removed obsolete DDJ-200 non-pad assignments");
            self.save_state();
        }
    }

    fn normalize_button_view_ids(&mut self) -> bool {
        let mut changed = false;

        if self.button_view_ids.len() >= 18 {
            self.button_view_ids.remove(17);
            self.button_view_ids.remove(8);
            changed = true;
        }

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

    fn decode_view_option(id: u8) -> Option<usize> {
        if id < VIEW_SELECT_OPTION_BASE_ID {
            return None;
        }

        Some((id - VIEW_SELECT_OPTION_BASE_ID) as usize)
    }

    fn midi_in(&mut self, ev: Vec<MidiEvent>, current_time: u32) {
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
                (status, PLAY_PAUSE_KIND, value)
                    if value > 0 && PLAY_PAUSE_STATUS_BYTES.contains(&status) =>
                {
                    self.toggle_first_scene_graph();
                }
                (status, kind, value) if value > 0 => {
                    if let Some(button_idx) = self.view_button_for_midi(status, kind) {
                        if self.is_deck_pad_button(button_idx) {
                            self.pad_event_times[button_idx] = Some(current_time);
                        }
                        self.activate_view(button_idx);
                    } else {
                        println!("DDJ-200 MIDI: {:?}", e);
                    }
                }
                _ => {}
            }
        }
    }

    fn toggle_first_scene_graph(&mut self) {
        let Some((&graph_id, graph)) = self.dmx.scene_graphs.graphs.iter().next() else {
            self.push_log("Play/Pause: no scene graph to toggle");
            return;
        };

        // Optimistic local toggle so the UI logs the right state until the next sync.
        let new_enabled = !graph.enabled;
        bpf::send_event(ControlEvent::ToggleSceneGraphEnabled(graph_id));
        self.push_log(format!(
            "Scene graph #{graph_id} {}",
            if new_enabled { "enabled" } else { "disabled" }
        ));
    }

    fn view_button_for_midi(&self, status: u8, kind: u8) -> Option<usize> {
        VIEW_BUTTONS
            .iter()
            .position(|button| button.matches(status, kind))
    }

    fn is_deck_pad_button(&self, button_idx: usize) -> bool {
        VIEW_BUTTONS
            .get(button_idx)
            .map(|button| matches!(button.kind, ViewButtonKind::Pad(_)))
            .unwrap_or(false)
    }

    fn pad_position(&self, button_idx: usize) -> Option<(Deck, u8)> {
        let button = VIEW_BUTTONS.get(button_idx)?;
        match button.kind {
            ViewButtonKind::Pad(pad) => Some((button.deck, pad)),
        }
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

    fn deck_rect(deck: Deck) -> (i32, i32, i32, i32) {
        match deck {
            Deck::Left => (54, 34, 270, 210),
            Deck::Right => (436, 34, 270, 210),
        }
    }

    fn pad_rect(deck: Deck, pad: u8) -> Option<(i32, i32, i32, i32)> {
        if !(1..=8).contains(&pad) {
            return None;
        }

        let (deck_x, deck_y, _, _) = Self::deck_rect(deck);
        let col = ((pad - 1) % 4) as i32;
        let row = ((pad - 1) / 4) as i32;
        let w = 50;
        let h = 38;
        let gap = 9;
        let x = deck_x + 20 + col * (w + gap);
        let y = deck_y + 126 + row * (h + gap);

        Some((x, y, w, h))
    }

    fn pad_button_at_canvas(&self, x: i32, y: i32) -> Option<usize> {
        VIEW_BUTTONS
            .iter()
            .enumerate()
            .filter_map(|(button_idx, _)| {
                let (deck, pad) = self.pad_position(button_idx)?;
                let (pad_x, pad_y, pad_w, pad_h) = Self::pad_rect(deck, pad)?;
                Some((button_idx, pad_x, pad_y, pad_w, pad_h))
            })
            .find(|(_, pad_x, pad_y, pad_w, pad_h)| {
                x >= *pad_x && x <= *pad_x + *pad_w && y >= *pad_y && y <= *pad_y + *pad_h
            })
            .map(|(button_idx, _, _, _, _)| button_idx)
    }

    fn pad_fill_color(&self, button_idx: usize) -> (u8, u8, u8) {
        let selected = self.view_selector_open == Some(button_idx);
        let active = self.pad_event_times[button_idx]
            .map(|event_time| {
                self.current_time.saturating_sub(event_time) <= PAD_EVENT_HIGHLIGHT_MS
            })
            .unwrap_or(false);
        let assigned = self
            .button_view_ids
            .get(button_idx)
            .copied()
            .flatten()
            .is_some();

        if active {
            (235, 160, 36)
        } else if selected {
            (55, 140, 220)
        } else if assigned {
            (42, 82, 125)
        } else {
            (55, 60, 70)
        }
    }

    fn draw_fader(&self, index: usize, x: i32, y: i32, label: &str) {
        let track_w = 16;
        let track_h = 148;
        let thumb_h = 26;
        let value = self.fader_vals[index].min(127) as i32;
        let thumb_y = y + ((127 - value) * (track_h - thumb_h) / 127);

        ui::painter_text(x - 4, y - 24, 12, 185, 194, 208, 255, label);
        ui::painter_rect(x, y, track_w, track_h, 48, 55, 67, 255);
        ui::painter_rect_stroke(x, y, track_w, track_h, 106, 116, 132, 255, 1);
        ui::painter_rect(x - 7, thumb_y, track_w + 14, thumb_h, 72, 130, 180, 255);
        ui::painter_rect_stroke(x - 7, thumb_y, track_w + 14, thumb_h, 158, 182, 208, 255, 2);
        ui::painter_text(
            x - 8,
            y + track_h + 8,
            10,
            165,
            174,
            188,
            255,
            &format!("{}", self.fader_vals[index]),
        );
    }

    fn draw_deck(&self, deck: Deck, label: &str) {
        let (x, y, _, _) = Self::deck_rect(deck);

        ui::painter_circle(x + 134, y + 58, 54, 45, 50, 59, 255);
        ui::painter_circle_stroke(x + 134, y + 58, 54, 125, 136, 154, 255, 2);
        ui::painter_circle_stroke(x + 134, y + 58, 28, 78, 86, 100, 255, 2);
        ui::painter_text(x + 124, y + 50, 13, 188, 198, 214, 220, label);

        for (button_idx, _) in VIEW_BUTTONS.iter().enumerate() {
            let Some((button_deck, pad)) = self.pad_position(button_idx) else {
                continue;
            };
            if button_deck != deck {
                continue;
            }
            let Some((pad_x, pad_y, pad_w, pad_h)) = Self::pad_rect(deck, pad) else {
                continue;
            };

            let (r, g, b) = self.pad_fill_color(button_idx);
            ui::painter_rect(pad_x, pad_y, pad_w, pad_h, r, g, b, 255);
            let selected = self.view_selector_open == Some(button_idx);
            let (sr, sg, sb) = if selected {
                (250, 218, 90)
            } else {
                (125, 136, 150)
            };
            ui::painter_rect_stroke(pad_x, pad_y, pad_w, pad_h, sr, sg, sb, 255, 2);
            ui::painter_text(
                pad_x + 6,
                pad_y + 5,
                12,
                236,
                240,
                246,
                255,
                &format!("P{pad}"),
            );

            if let Some(view_id) = self.button_view_ids.get(button_idx).copied().flatten() {
                let label = self
                    .view_info(view_id)
                    .map(|info| info.view.name.as_str())
                    .unwrap_or("?");
                let short_label = label.chars().take(7).collect::<String>();
                ui::painter_text(pad_x + 6, pad_y + 21, 10, 230, 235, 242, 235, &short_label);
            }
        }
    }

    fn draw_digital_twin(&self) {
        ui::painter_begin(DDJ_CANVAS_ID, DDJ_CANVAS_WIDTH, DDJ_CANVAS_HEIGHT);
        ui::painter_rect(0, 0, DDJ_CANVAS_WIDTH, DDJ_CANVAS_HEIGHT, 16, 18, 23, 255);
        ui::painter_rect_stroke(
            0,
            0,
            DDJ_CANVAS_WIDTH,
            DDJ_CANVAS_HEIGHT,
            72,
            80,
            94,
            255,
            2,
        );
        self.draw_deck(Deck::Left, "D1");
        self.draw_deck(Deck::Right, "D2");
        self.draw_fader(0, 360, 58, "L");
        self.draw_fader(1, 392, 58, "R");

        ui::painter_text(
            28,
            252,
            12,
            160,
            168,
            180,
            255,
            "Click a deck pad to assign a view",
        );
        ui::painter_end();
    }

    fn draw_view_options(&self) {
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

    fn draw_ui(&mut self, events: &[ControlEventMessage], plugin_id: u8) {
        ui::begin();

        ui::label("DDJ-200 view activation");

        if self.available_views.is_empty() {
            ui::label("No views available.");
        }

        self.draw_digital_twin();

        if let Some(button_idx) = self.view_selector_open {
            if self.is_deck_pad_button(button_idx) {
                ui::label(&format!("Assign {}", VIEW_BUTTONS[button_idx].label));
                self.draw_view_options();
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
                    PluginUiEvent::Button { id } => {
                        if let Some(view_idx) = Self::decode_view_option(id) {
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
                    PluginUiEvent::CanvasClick { id, x, y } if id == DDJ_CANVAS_ID => {
                        if let Some(button_idx) = self.pad_button_at_canvas(x, y) {
                            self.view_selector_open = Some(button_idx);
                            self.push_log(format!("Selected {}", VIEW_BUTTONS[button_idx].label));
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}
