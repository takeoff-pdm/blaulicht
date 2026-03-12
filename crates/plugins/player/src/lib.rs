use blaulicht_plugin_framework as bpf;
use blaulicht_plugin_framework::prelude::println;
use blaulicht_plugin_framework::{ui, Plugin};
use blaulicht_shared::{
    AnimationSpeedModifier, ControlEvent, EngineState, PluginUiEvent, TickInput,
};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

const ID_START: u8 = 1;
const ID_PAUSE: u8 = 2;
const ID_STOP: u8 = 3;
const ID_ADD_CUE: u8 = 4;
const ID_SORT_CUES: u8 = 5;
const ID_SCENE_LIST: u8 = 6;
const ID_START_AT_TEXT: u8 = 7;

const CUE_ID_BASE: u8 = 20;
const CUE_ID_STRIDE: u8 = 6;
const CUE_ID_TIME: u8 = 0;
const CUE_ID_SCENE: u8 = 1;
const CUE_ID_SPEED: u8 = 2;
const CUE_ID_BRIGHTNESS: u8 = 3;
const CUE_ID_OVERLAY: u8 = 4;
const CUE_ID_REMOVE: u8 = 5;

const MAX_CUES: usize = 30;
const SPEED_MAX_INDEX: u8 = (AnimationSpeedModifier::ALL.len() - 1) as u8;
const DEFAULT_SPEED_INDEX: u8 = 4; // AnimationSpeedModifier::_1
const CUE_BOX_MAX_WIDTH: i32 = 520;
const CUE_BOX_RESET_WIDTH: i32 = 10000;
const CUE_BOX_MIN_WIDTH: i32 = 520;
const CUE_BOX_RESET_MIN_WIDTH: i32 = 0;


#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct CuePoint {
    time_text: String,
    scene_text: String,
    speed_index: u8,
    brightness: u8,
    overlay: bool,
    ui_id: u8,
}

#[derive(Clone)]
struct SceneChoice {
    key: SceneChoiceKey,
    label: String,
}

#[derive(Clone, Copy)]
enum SceneChoiceKey {
    RemoveAll,
    Scene(u8),
}

impl Default for CuePoint {
    fn default() -> Self {
        Self {
            time_text: "0.00".to_string(),
            scene_text: "0".to_string(),
            speed_index: DEFAULT_SPEED_INDEX,
            brightness: 255,
            overlay: false,
            ui_id: 0,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(default)]
pub struct SaveState {
    cues: Vec<CuePoint>,
    #[serde(default = "default_start_at_text")]
    start_at_text: String,
}

impl Default for SaveState {
    fn default() -> Self {
        Self {
            cues: Vec::new(),
            start_at_text: default_start_at_text(),
        }
    }
}

pub struct TimelinePlugin {
    state: SaveState,
    dirty: bool,

    running: bool,
    start_clock: Option<u32>,
    paused_elapsed_ms: u32,
    paused_by_user: bool,
    fired: Vec<bool>,

    last_dmx_sync: u32,
    dmx_cache: EngineState,
}

impl Default for TimelinePlugin {
    fn default() -> Self {
        Self {
            state: SaveState::default(),
            dirty: false,
            running: false,
            start_clock: None,
            paused_elapsed_ms: 0,
            paused_by_user: false,
            fired: Vec::new(),
            last_dmx_sync: 0,
            dmx_cache: EngineState::default(),
        }
    }
}

impl TimelinePlugin {
    fn save(&self) {
        if let Ok(json) = serde_json::to_string(&self.state) {
            bpf::save_plugin_state(bpf::PluginStateLocation::Showfile, &json);
        }
    }

    fn load(&mut self) {
        if let Some(json) = bpf::load_plugin_state(bpf::PluginStateLocation::Showfile) {
            if let Ok(saved) = serde_json::from_str::<SaveState>(&json) {
                self.state = saved;
            } else {
                println!("Timeline: state loading failed, using defaults");
                self.state = SaveState::default();
            }
        }
        self.normalize_state();
    }

    fn normalize_state(&mut self) {
        if self.state.cues.len() > MAX_CUES {
            self.state.cues.truncate(MAX_CUES);
        }

        for cue in &mut self.state.cues {
            cue.speed_index = cue.speed_index.min(SPEED_MAX_INDEX);
            if cue.time_text.len() > 16 {
                cue.time_text.truncate(16);
            }
            if cue.scene_text.len() > 16 {
                cue.scene_text.truncate(16);
            }
        }

        if self.state.start_at_text.len() > 16 {
            self.state.start_at_text.truncate(16);
        }

        self.ensure_cue_ids();
    }

    fn ensure_cue_ids(&mut self) {
        let mut used = vec![false; MAX_CUES];
        for cue in &mut self.state.cues {
            if let Some(slot) = cue_id_slot(cue.ui_id) {
                if !used[slot] {
                    used[slot] = true;
                    continue;
                }
            }
            cue.ui_id = 0;
        }

        for cue in &mut self.state.cues {
            if cue.ui_id == 0 {
                if let Some((slot, base_id)) = next_available_cue_base_id_from_used(&used) {
                    cue.ui_id = base_id;
                    used[slot] = true;
                } else {
                    println!("Timeline: ran out of cue ids; cue will be read-only");
                }
            }
        }
    }

    fn sync_dmx(&mut self, now: u32) {
        if now - self.last_dmx_sync > 250 {
            self.last_dmx_sync = now;
            self.dmx_cache = bpf::get_dmx();
        }
    }

    fn elapsed_ms(&self, now: u32) -> u32 {
        if self.running {
            if let Some(start_clock) = self.start_clock {
                now.saturating_sub(start_clock)
            } else {
                0
            }
        } else {
            self.paused_elapsed_ms
        }
    }

    fn start(&mut self, now: u32) {
        if self.running {
            return;
        }

        if !self.paused_by_user {
            self.paused_elapsed_ms = self.start_at_ms();
        }
        let start_clock = now.saturating_sub(self.paused_elapsed_ms);
        self.start_clock = Some(start_clock);
        self.running = true;
        self.paused_by_user = false;
    }

    fn pause(&mut self, now: u32) {
        if !self.running {
            return;
        }

        if let Some(start_clock) = self.start_clock {
            self.paused_elapsed_ms = now.saturating_sub(start_clock);
        }
        self.state.start_at_text = format!("{:.2}", self.paused_elapsed_ms as f32 / 1000.0);
        self.dirty = true;
        self.running = false;
        self.start_clock = None;
        self.paused_by_user = true;
    }

    fn reset_transport(&mut self) {
        self.running = false;
        self.start_clock = None;
        self.paused_elapsed_ms = 0;
        self.state.start_at_text = "0.00".to_string();
        self.dirty = true;
        self.paused_by_user = false;
        self.fired = vec![false; self.state.cues.len()];
    }

    fn add_cue(&mut self) {
        if self.state.cues.len() >= MAX_CUES {
            println!("Timeline: cue limit reached ({MAX_CUES})");
            return;
        }

        self.ensure_cue_ids();
        let ui_id = match self.next_available_cue_base_id() {
            Some(id) => id,
            None => {
                println!("Timeline: ran out of cue ids; cannot add cue");
                return;
            }
        };

        let max_time = self
            .state
            .cues
            .iter()
            .filter_map(|cue| parse_time_secs(&cue.time_text))
            .fold(0.0_f32, |acc, t| if t > acc { t } else { acc });

        let next_time = if self.state.cues.is_empty() { 0.0 } else { max_time + 1.0 };
        let scene_id = self.dmx_cache.current_scene_focus;

        let cue = CuePoint {
            time_text: format!("{:.2}", next_time),
            scene_text: scene_id.to_string(),
            speed_index: DEFAULT_SPEED_INDEX,
            brightness: 255,
            overlay: false,
            ui_id,
        };

        self.state.cues.push(cue);
        self.dirty = true;
    }

    fn start_at_ms(&self) -> u32 {
        match parse_time_secs(&self.state.start_at_text) {
            Some(value) if value >= 0.0 => (value * 1000.0) as u32,
            _ => 0,
        }
    }

    fn remove_cue(&mut self, idx: usize) {
        if idx < self.state.cues.len() {
            self.state.cues.remove(idx);
            self.dirty = true;
        }
    }

    fn sort_cues_by_time(&mut self, preserve_fired: bool) {
        let mut indexed: Vec<(usize, CuePoint)> = self
            .state
            .cues
            .iter()
            .cloned()
            .enumerate()
            .collect();
        indexed.sort_by(|(_, a), (_, b)| {
            let ta = parse_time_secs(&a.time_text);
            let tb = parse_time_secs(&b.time_text);
            match (ta, tb) {
                (Some(ta), Some(tb)) => ta.partial_cmp(&tb).unwrap_or(Ordering::Equal),
                (Some(_), None) => Ordering::Less,
                (None, Some(_)) => Ordering::Greater,
                (None, None) => Ordering::Equal,
            }
        });
        self.state.cues = indexed.iter().map(|(_, cue)| cue.clone()).collect();
        if preserve_fired {
            let mut new_fired = vec![false; indexed.len()];
            for (new_idx, (old_idx, _)) in indexed.iter().enumerate() {
                if let Some(fired) = self.fired.get(*old_idx).copied() {
                    new_fired[new_idx] = fired;
                }
            }
            self.fired = new_fired;
        }
        self.dirty = true;
    }

    fn decode_cue_id(&self, id: u8) -> Option<(usize, u8)> {
        for (row, cue) in self.state.cues.iter().enumerate() {
            if let Some(base_id) = cue_base_id(cue) {
                if id >= base_id {
                    let offset = id - base_id;
                    if offset < CUE_ID_STRIDE {
                        return Some((row, offset));
                    }
                }
            }
        }
        None
    }

    fn next_available_cue_base_id(&self) -> Option<u8> {
        let mut used = vec![false; MAX_CUES];
        for cue in &self.state.cues {
            if let Some(slot) = cue_id_slot(cue.ui_id) {
                used[slot] = true;
            }
        }
        next_available_cue_base_id_from_used(&used).map(|(_, base_id)| base_id)
    }

    fn handle_ui_event(&mut self, event: &PluginUiEvent, now: u32) {
        match event {
            PluginUiEvent::Button { id } => match *id {
                ID_START => self.start(now),
                ID_PAUSE => self.pause(now),
                ID_STOP => self.reset_transport(),
                ID_ADD_CUE => self.add_cue(),
                ID_SORT_CUES => self.sort_cues_by_time(true),
                _ => {
                    if let Some((row, field)) = self.decode_cue_id(*id) {
                        if field == CUE_ID_REMOVE {
                            self.remove_cue(row);
                        }
                    }
                }
            },
            PluginUiEvent::Checkbox { id, checked } => {
                if let Some((row, field)) = self.decode_cue_id(*id) {
                    if field == CUE_ID_OVERLAY {
                        if let Some(cue) = self.state.cues.get_mut(row) {
                            cue.overlay = *checked;
                            if !cue.overlay && is_remove_all(&cue.scene_text) {
                                cue.scene_text = self.dmx_cache.current_scene_focus.to_string();
                            }
                            self.dirty = true;
                        }
                    }
                }
            }
            PluginUiEvent::Text { id, text } => {
                if let Some((row, field)) = self.decode_cue_id(*id) {
                    if let Some(cue) = self.state.cues.get_mut(row) {
                        let mut trimmed: String = text.chars().take(16).collect();
                        if trimmed.is_empty() {
                            trimmed = "0".to_string();
                        }
                        match field {
                            CUE_ID_TIME => cue.time_text = trimmed,
                            _ => {}
                        }
                        self.dirty = true;
                    }
                }
                if *id == ID_START_AT_TEXT {
                    let mut trimmed: String = text.chars().take(16).collect();
                    if trimmed.is_empty() {
                        trimmed = "0".to_string();
                    }
                    self.state.start_at_text = trimmed;
                    self.paused_by_user = false;
                    self.start_clock = None;
                    self.dirty = true;
                }
            }
            PluginUiEvent::ComboBox { id, selected } => {
                if let Some((row, field)) = self.decode_cue_id(*id) {
                    if field == CUE_ID_SCENE {
                        let overlay = self
                            .state
                            .cues
                            .get(row)
                            .map(|cue| cue.overlay)
                            .unwrap_or(false);
                        let choices = self.scene_choices(overlay);
                        if let Some(choice) = choices.get(*selected as usize) {
                            if let Some(cue) = self.state.cues.get_mut(row) {
                                match choice.key {
                                    SceneChoiceKey::RemoveAll => {
                                        cue.scene_text = "RemoveAll".to_string();
                                        cue.overlay = true;
                                    }
                                    SceneChoiceKey::Scene(scene_id) => {
                                        cue.scene_text = scene_id.to_string();
                                    }
                                }
                                self.dirty = true;
                            }
                        }
                    }
                }
            }
            PluginUiEvent::Slider { id, value } | PluginUiEvent::HFader { id, value } => {
                if let Some((row, field)) = self.decode_cue_id(*id) {
                    if let Some(cue) = self.state.cues.get_mut(row) {
                        match field {
                            CUE_ID_SPEED => cue.speed_index = (*value).min(SPEED_MAX_INDEX),
                            CUE_ID_BRIGHTNESS => cue.brightness = *value,
                            _ => {}
                        }
                        self.dirty = true;
                    }
                }
            }
            _ => {}
        }
    }

    fn fire_cues(&mut self, elapsed_ms: u32) {
        if self.fired.len() != self.state.cues.len() {
            self.fired = vec![false; self.state.cues.len()];
        }

        for (idx, cue) in self.state.cues.iter().enumerate() {
            if self.fired[idx] {
                continue;
            }

            let time_ms = match parse_time_secs(&cue.time_text) {
                Some(time_secs) if time_secs >= 0.0 => (time_secs * 1000.0) as u32,
                _ => continue,
            };

            if elapsed_ms < time_ms {
                continue;
            }

            let speed = speed_from_index(cue.speed_index);

            println!(
                "Timeline: firing cue {} -> scene {} (speed {}, brightness {})",
                idx + 1,
                cue.scene_text,
                speed.as_str(),
                cue.brightness
            );

            if cue.overlay {
                if is_remove_all(&cue.scene_text) {
                    bpf::send_event(ControlEvent::SetOverlays(Vec::new()));
                    self.fired[idx] = true;
                    continue;
                }

                let scene_id = match parse_scene_id(&cue.scene_text) {
                    Some(scene_id) => scene_id,
                    None => {
                        println!("Timeline: invalid scene id '{}'", cue.scene_text);
                        self.fired[idx] = true;
                        continue;
                    }
                };

                bpf::send_event(ControlEvent::AddOverlayScene(scene_id));
                bpf::send_event(ControlEvent::SetSceneMasterSpeed(scene_id, speed));
                bpf::send_event(ControlEvent::SetSceneMasterAlpha(scene_id, cue.brightness));
            } else {
                if is_remove_all(&cue.scene_text) {
                    println!("Timeline: RemoveAll is only valid for overlay cues.");
                    self.fired[idx] = true;
                    continue;
                }

                let scene_id = match parse_scene_id(&cue.scene_text) {
                    Some(scene_id) => scene_id,
                    None => {
                        println!("Timeline: invalid scene id '{}'", cue.scene_text);
                        self.fired[idx] = true;
                        continue;
                    }
                };
                bpf::send_event(ControlEvent::SetSceneFocus(scene_id));
                bpf::send_event(ControlEvent::SetSceneMasterSpeed(scene_id, speed));
                bpf::send_event(ControlEvent::SetSceneMasterAlpha(scene_id, cue.brightness));
            }

            self.fired[idx] = true;
        }
    }

    fn render_scene_list(&self) {
        ui::begin_collapsing(ID_SCENE_LIST, "Scenes", false);
        ui::label("RemoveAll (clears overlays)");
        if self.dmx_cache.scenes.is_empty() {
            ui::label("No scenes available.");
        } else {
            for (scene_id, scene) in &self.dmx_cache.scenes {
                let focus_marker = if *scene_id == self.dmx_cache.current_scene_focus {
                    "*"
                } else {
                    ""
                };
                ui::label(&format!("{}{}: {}", focus_marker, scene_id, scene.name));
            }
        }
        ui::end_collapsing();
    }

    fn scene_choices(&self, overlay: bool) -> Vec<SceneChoice> {
        let mut choices = Vec::new();
        if overlay {
            choices.push(SceneChoice {
                key: SceneChoiceKey::RemoveAll,
                label: "RemoveAll".to_string(),
            });
        }

        for (scene_id, scene) in &self.dmx_cache.scenes {
            choices.push(SceneChoice {
                key: SceneChoiceKey::Scene(*scene_id),
                label: truncate_label(&format!("{}: {}", scene_id, scene.name), 24),
            });
        }

        choices
    }

    fn render_cue_list(&self) {
        if self.state.cues.is_empty() {
            return;
        }

        let editing_enabled = !self.running;

        for (idx, cue) in self.state.cues.iter().enumerate() {
            let base_id = match cue_base_id(cue) {
                Some(id) => id,
                None => continue,
            };

            let fired = self.fired.get(idx).copied().unwrap_or(false);
            if fired {
                ui::begin_frame_styled_border(100 + idx as u8, "", 4, 4, 2, 2, 40, 200, 80, 255, 2);
            } else {
                ui::begin_frame_styled_border(100 + idx as u8, "", 4, 4, 2, 2, 110, 110, 110, 255, 2);
            }
            ui::set_min_width(CUE_BOX_MIN_WIDTH);
            ui::set_max_width(CUE_BOX_MAX_WIDTH);
            ui::separator();
            ui::label(&format!(
                "Cue {}{}",
                idx + 1,
                if fired {
                    " (fired)"
                } else {
                    ""
                }
            ));

            ui::begin_horizontal();
            ui::text_edit("Time (s)", base_id + CUE_ID_TIME, &cue.time_text);
            let choices = self.scene_choices(cue.overlay);
            let scene_index = scene_choice_index(&choices, &cue.scene_text);
            let option_labels: Vec<String> =
                choices.iter().map(|choice| choice.label.clone()).collect();
            if option_labels.is_empty() {
                ui::label("Scene: <none>");
            } else {
                ui::combo_box("Scene", base_id + CUE_ID_SCENE, &option_labels, scene_index);
            }
            ui::checkbox("Overlay", base_id + CUE_ID_OVERLAY, cue.overlay);
            ui::end_horizontal();

            ui::begin_horizontal();
            ui::slider(
                "Speed",
                base_id + CUE_ID_SPEED,
                0,
                SPEED_MAX_INDEX,
                cue.speed_index,
            );
            let speed = speed_from_index(cue.speed_index);
            ui::label(&format!("x{}", speed.as_str()));
            ui::slider(
                "Brightness",
                base_id + CUE_ID_BRIGHTNESS,
                0,
                u8::MAX,
                cue.brightness,
            );
            ui::button_styled("Remove", base_id + CUE_ID_REMOVE, editing_enabled);
            ui::end_horizontal();

            if parse_time_secs(&cue.time_text).is_none() {
                ui::label("Invalid time value.");
            }
            if cue.overlay {
                if !is_remove_all(&cue.scene_text) && parse_scene_id(&cue.scene_text).is_none() {
                    ui::label("Invalid scene id.");
                }
            } else if parse_scene_id(&cue.scene_text).is_none() {
                ui::label("Invalid scene id.");
            }
            ui::end_frame();
            ui::set_min_width(CUE_BOX_RESET_MIN_WIDTH);
            ui::set_max_width(CUE_BOX_RESET_WIDTH);
        }
    }

    fn render_ui(&self, now: u32) {
        let elapsed_ms = self.elapsed_ms(now);
        let elapsed_secs = elapsed_ms as f32 / 1000.0;

        ui::begin();
        ui::set_min_width(CUE_BOX_MIN_WIDTH);
        ui::set_max_width(CUE_BOX_MAX_WIDTH);
        ui::begin_frame_styled(42, "Scene Timeline", 4, 6, 2, 2);
        ui::begin_horizontal();
        let start_enabled = !self.running;
        let pause_enabled = self.running;
        let stop_enabled = self.running || self.paused_elapsed_ms > 0;
        ui::button_styled("START", ID_START, start_enabled);
        ui::text_edit("Start at (s)", ID_START_AT_TEXT, &self.state.start_at_text);
        ui::button_styled("PAUSE", ID_PAUSE, pause_enabled);
        ui::button_styled("STOP", ID_STOP, stop_enabled);
        ui::end_horizontal();
        ui::separator();
        ui::label("Create time-coded scene activations.");
        ui::label(&format!(
            "Status: {}",
            if self.running { "Running" } else { "Paused" }
        ));
        ui::label_styled(&format!("Time: {:.2}s", elapsed_secs), 18, true);

        ui::begin_horizontal();
        let editing_enabled = !self.running;
        ui::button_styled("Add Cue", ID_ADD_CUE, editing_enabled);
        ui::button_styled("Sort by Time", ID_SORT_CUES, editing_enabled);
        ui::end_horizontal();

        ui::label(&format!("Cues: {}/{}", self.state.cues.len(), MAX_CUES));

        self.render_scene_list();
        self.render_cue_list();

        ui::end_frame();
        ui::set_min_width(CUE_BOX_RESET_MIN_WIDTH);
        ui::set_max_width(CUE_BOX_RESET_WIDTH);
    }
}

impl Plugin for TimelinePlugin {
    fn initialize(&mut self, input: TickInput) {
        self.load();
        self.sync_dmx(input.clock);
        self.paused_elapsed_ms = self.start_at_ms();
        self.paused_by_user = false;
        self.fired = vec![false; self.state.cues.len()];
    }

    fn run(&mut self, input: TickInput) {
        self.sync_dmx(input.clock);

        for ev in &input.events.events {
            if let ControlEvent::PluginUi(ui_event, plugin_id) = ev.body() {
                if plugin_id == input.id {
                    self.handle_ui_event(&ui_event, input.clock);
                }
            }
        }

        if self.running {
            let elapsed_ms = self.elapsed_ms(input.clock);
            self.fire_cues(elapsed_ms);
        }

        self.render_ui(input.clock);

        if self.dirty {
            self.save();
            self.dirty = false;
        }
    }
}

#[no_mangle]
extern "C" fn main() {
    bpf::hook_plugin(Box::new(TimelinePlugin::default()));
}

fn parse_time_secs(text: &str) -> Option<f32> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    trimmed.parse::<f32>().ok()
}

fn parse_scene_id(text: &str) -> Option<u8> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    trimmed.parse::<u8>().ok()
}

fn is_remove_all(text: &str) -> bool {
    text.trim().eq_ignore_ascii_case("RemoveAll")
}

fn default_start_at_text() -> String {
    "0.00".to_string()
}

fn truncate_label(text: &str, max_len: usize) -> String {
    if text.chars().count() <= max_len {
        return text.to_string();
    }
    let take_len = max_len.saturating_sub(3);
    let mut out: String = text.chars().take(take_len).collect();
    out.push_str("...");
    out
}

fn speed_from_index(index: u8) -> AnimationSpeedModifier {
    let idx = index.min(SPEED_MAX_INDEX) as usize;
    AnimationSpeedModifier::from_index(idx)
}

fn scene_choice_index(choices: &[SceneChoice], scene_text: &str) -> u8 {
    if is_remove_all(scene_text) {
        if let Some(idx) = choices
            .iter()
            .position(|choice| matches!(choice.key, SceneChoiceKey::RemoveAll))
        {
            return idx as u8;
        }
    }

    if let Some(scene_id) = parse_scene_id(scene_text) {
        if let Some(idx) = choices.iter().position(|choice| match choice.key {
            SceneChoiceKey::Scene(id) => id == scene_id,
            SceneChoiceKey::RemoveAll => false,
        }) {
            return idx as u8;
        }
    }

    0
}

fn cue_id_slot(id: u8) -> Option<usize> {
    if id < CUE_ID_BASE {
        return None;
    }
    let offset = id - CUE_ID_BASE;
    if offset % CUE_ID_STRIDE != 0 {
        return None;
    }
    let slot = (offset / CUE_ID_STRIDE) as usize;
    if slot >= MAX_CUES {
        return None;
    }
    let max_id = id as usize + CUE_ID_REMOVE as usize;
    if max_id > u8::MAX as usize {
        return None;
    }
    Some(slot)
}

fn cue_base_id_from_slot(slot: usize) -> Option<u8> {
    if slot >= MAX_CUES {
        return None;
    }
    let stride = CUE_ID_STRIDE as usize;
    let base = CUE_ID_BASE as usize + slot * stride;
    let max_id = base + CUE_ID_REMOVE as usize;
    if max_id > u8::MAX as usize {
        return None;
    }
    Some(base as u8)
}

fn cue_base_id(cue: &CuePoint) -> Option<u8> {
    cue_id_slot(cue.ui_id).map(|_| cue.ui_id)
}

fn next_available_cue_base_id_from_used(used: &[bool]) -> Option<(usize, u8)> {
    for slot in 0..MAX_CUES {
        if !used[slot] {
            if let Some(base_id) = cue_base_id_from_slot(slot) {
                return Some((slot, base_id));
            }
        }
    }
    None
}
