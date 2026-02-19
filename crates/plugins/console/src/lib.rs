use blaulicht_plugin_framework as bpf;
use blaulicht_plugin_framework::{ui, Plugin};
use blaulicht_shared::{
    AppPage, AnimationSpec, AnimationSpeedModifier, ControlEvent, EngineState, MainUiEvent,
    PluginUiEvent, TickInput,
};
use blaulicht_shared::{FixtureProperty, scene::FixtureSelection};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

const EVENT_TABS_ID: u8 = 1;

const TAB_SELECTION: u8 = 1;
const TAB_FIXTURE: u8 = 2;
const TAB_ANIMATION: u8 = 3;
const TAB_SCENE: u8 = 4;
const TAB_OVERRIDE: u8 = 5;
const TAB_MISC: u8 = 6;
const TAB_MAIN_UI: u8 = 7;
const TAB_PLUGIN_UI: u8 = 8;
const TAB_RAW: u8 = 9;

const BTN_SEND_EVENT: u8 = 2;
const BTN_ADD_TO_TX: u8 = 3;
const BTN_CLEAR_TX: u8 = 4;
const BTN_SEND_TX: u8 = 5;

const ID_SELECTION_GROUP: u8 = 10;
const ID_SELECTION_FIXTURE: u8 = 11;

const ID_SET_ENABLED: u8 = 20;
const ID_SET_ALPHA: u8 = 21;
const ID_STROBE: u8 = 22;
const ID_FOCUS: u8 = 23;
const ID_TILT: u8 = 24;
const ID_PAN: u8 = 25;
const ID_COLOR_PICKER: u8 = 26;
const ID_COLOR_HUE_TEXT: u8 = 27;
const ID_COLOR_SAT: u8 = 28;
const ID_COLOR_VAL: u8 = 29;
const ID_ADD_TO_DELTA_TEXT: u8 = 30;

const ID_ANIMATION_ID: u8 = 40;
const ID_ANIMATION_SPEC_JSON: u8 = 41;

const ID_MISC_DESCRIPTOR: u8 = 50;
const ID_MISC_VALUE: u8 = 51;
const ID_REMOVE_CHANGE_SELECTION_TEXT: u8 = 52;

const ID_SCENE_ID: u8 = 60;
const ID_OVERLAYS_TEXT: u8 = 61;
const ID_SCENE_MASTER_ALPHA: u8 = 62;

const ID_OVERRIDE_UNIVERSE_TEXT: u8 = 70;
const ID_OVERRIDE_CHANNEL_TEXT: u8 = 71;
const ID_OVERRIDE_VALUE: u8 = 72;

const ID_MAIN_UI_PLUGIN_ID: u8 = 80;
const ID_MAIN_UI_OPEN: u8 = 81;

const ID_PLUGIN_UI_TARGET_PLUGIN_ID: u8 = 90;
const ID_PLUGIN_UI_ID: u8 = 91;
const ID_PLUGIN_UI_VALUE: u8 = 92;
const ID_PLUGIN_UI_CHECKED: u8 = 93;
const ID_PLUGIN_UI_TEXT: u8 = 94;
const ID_PLUGIN_UI_COLOR: u8 = 95;
const ID_PLUGIN_UI_TABS_ID: u8 = 96;
const ID_PLUGIN_UI_TAB_ID: u8 = 97;
const ID_PLUGIN_UI_CANVAS_X: u8 = 98;
const ID_PLUGIN_UI_CANVAS_Y: u8 = 99;
const ID_PLUGIN_UI_CANVAS_DX: u8 = 101;
const ID_PLUGIN_UI_CANVAS_DY: u8 = 102;
const ID_PLUGIN_UI_CANVAS_DELTA: u8 = 103;

const ID_RAW_EVENT_JSON: u8 = 110;

const BTN_SELECTION_ACTION_BASE: u8 = 120;
const BTN_FIXTURE_ACTION_BASE: u8 = 130;
const BTN_ANIMATION_ACTION_BASE: u8 = 150;
const BTN_SCENE_ACTION_BASE: u8 = 160;
const BTN_OVERRIDE_ACTION_BASE: u8 = 170;
const BTN_MISC_ACTION_BASE: u8 = 172;
const BTN_MAIN_UI_ACTION_BASE: u8 = 174;
const BTN_PLUGIN_UI_ACTION_BASE: u8 = 176;
const BTN_PROPERTY_BASE: u8 = 200;
const BTN_SPEED_BASE: u8 = 210;
const BTN_APP_PAGE_BASE: u8 = 220;

const DMX_PAINTER_ID: u8 = 250;

const SELECTION_ACTIONS: [&str; 8] = [
    "Select Group",
    "De-Select Group",
    "Limit Selection",
    "Unlimit Selection",
    "Remove Selection",
    "Remove All Selection",
    "Push Selection",
    "Pop Selection",
];

const FIXTURE_ACTIONS: [&str; 11] = [
    "Set Enabled",
    "Set Alpha",
    "Set Strobe",
    "Set Focus",
    "Set Tilt",
    "Set Pan",
    "Set Color",
    "Set Color Hue",
    "Set Color Saturation",
    "Set Color Value",
    "Add To Property",
];

const ANIMATION_ACTIONS: [&str; 7] = [
    "Add Animation",
    "Remove Animation",
    "Reset Animation",
    "Pause Animation",
    "Play Animation",
    "Set Animation Speed",
    "Load Spec Into Animation",
];

const SCENE_ACTIONS: [&str; 6] = [
    "Set Scene Focus",
    "Set Overlays",
    "Remove Overlay Scene",
    "Add Overlay Scene",
    "Set Scene Master Alpha",
    "Set Scene Master Speed",
];

const OVERRIDE_ACTIONS: [&str; 2] = ["Set Channel Override", "Remove Channel Override"];

const MISC_ACTIONS: [&str; 2] = ["Misc Event", "Remove Change"];

const MAIN_UI_ACTIONS: [&str; 2] = ["Navigate Page", "Set Plugin UI Open"];

const PLUGIN_UI_ACTIONS: [&str; 11] = [
    "Button",
    "Checkbox",
    "Switch",
    "Slider",
    "HFader",
    "Text",
    "Color",
    "Tab Changed",
    "Canvas Click",
    "Canvas Drag",
    "Canvas Pinch",
];

const PROPERTY_LABELS: [&str; 8] = [
    "Alpha",
    "Strobe",
    "Focus",
    "Color Hue",
    "Color Sat",
    "Color Val",
    "Tilt",
    "Pan",
];

const APP_PAGE_LABELS: [&str; 8] = [
    "Logs",
    "System",
    "Audio",
    "Fixtures Setup",
    "View",
    "View Perf",
    "Fixtures Perf",
    "Animations",
];

#[derive(Serialize, Deserialize)]
#[serde(default)]
pub struct SaveState {
    restart_timer: usize,
    event_tab: u8,
    selection_action: u8,
    fixture_action: u8,
    animation_action: u8,
    scene_action: u8,
    override_action: u8,
    misc_action: u8,
    main_ui_action: u8,
    plugin_ui_action: u8,

    selection_group_id: u8,
    selection_fixture_id: u8,

    set_enabled: bool,
    set_alpha: u8,
    strobe_speed: u8,
    focus: u8,
    tilt: u8,
    pan: u8,
    color_r: u8,
    color_g: u8,
    color_b: u8,
    color_a: u8,
    color_hue_text: String,
    color_saturation: u8,
    color_value: u8,
    add_to_property: u8,
    add_to_property_delta: String,

    animation_id: u8,
    animation_speed: u8,
    animation_spec_json: String,

    misc_descriptor: u8,
    misc_value: u8,
    remove_change_selection: String,
    remove_change_property: u8,

    scene_id: u8,
    overlays_text: String,
    scene_master_alpha: u8,
    scene_master_speed: u8,

    override_universe_text: String,
    override_channel_text: String,
    override_value: u8,

    main_ui_page: u8,
    main_ui_plugin_id: u8,
    main_ui_open: bool,

    plugin_ui_target_plugin_id: u8,
    plugin_ui_id: u8,
    plugin_ui_value: u8,
    plugin_ui_checked: bool,
    plugin_ui_text: String,
    plugin_ui_color_r: u8,
    plugin_ui_color_g: u8,
    plugin_ui_color_b: u8,
    plugin_ui_color_a: u8,
    plugin_ui_tabs_id: u8,
    plugin_ui_tab_id: u8,
    plugin_ui_canvas_x: String,
    plugin_ui_canvas_y: String,
    plugin_ui_canvas_dx: String,
    plugin_ui_canvas_dy: String,
    plugin_ui_canvas_delta: String,

    raw_event_json: String,
}

impl Default for SaveState {
    fn default() -> Self {
        Self {
            restart_timer: 0,
            event_tab: TAB_SELECTION,
            selection_action: 0,
            fixture_action: 0,
            animation_action: 0,
            scene_action: 0,
            override_action: 0,
            misc_action: 0,
            main_ui_action: 0,
            plugin_ui_action: 0,

            selection_group_id: 0,
            selection_fixture_id: 0,

            set_enabled: true,
            set_alpha: 255,
            strobe_speed: 0,
            focus: 0,
            tilt: 0,
            pan: 0,
            color_r: 255,
            color_g: 0,
            color_b: 0,
            color_a: 255,
            color_hue_text: "0".to_string(),
            color_saturation: 255,
            color_value: 255,
            add_to_property: 0,
            add_to_property_delta: "10".to_string(),

            animation_id: 0,
            animation_speed: AnimationSpeedModifier::_1.as_index() as u8,
            animation_spec_json:
                "{\n  \"name\": \"EMPTY\",\n  \"body\": {\n    \"BeatClock\": {}\n  },\n  \"property\": \"Alpha\"\n}".to_string(),

            misc_descriptor: 0,
            misc_value: 0,
            remove_change_selection: "0:0".to_string(),
            remove_change_property: 0,

            scene_id: 0,
            overlays_text: "0".to_string(),
            scene_master_alpha: 255,
            scene_master_speed: AnimationSpeedModifier::_1.as_index() as u8,

            override_universe_text: "0".to_string(),
            override_channel_text: "1".to_string(),
            override_value: 255,

            main_ui_page: 0,
            main_ui_plugin_id: 0,
            main_ui_open: true,

            plugin_ui_target_plugin_id: 0,
            plugin_ui_id: 0,
            plugin_ui_value: 127,
            plugin_ui_checked: false,
            plugin_ui_text: "".to_string(),
            plugin_ui_color_r: 255,
            plugin_ui_color_g: 0,
            plugin_ui_color_b: 0,
            plugin_ui_color_a: 255,
            plugin_ui_tabs_id: 0,
            plugin_ui_tab_id: 0,
            plugin_ui_canvas_x: "0".to_string(),
            plugin_ui_canvas_y: "0".to_string(),
            plugin_ui_canvas_dx: "0".to_string(),
            plugin_ui_canvas_dy: "0".to_string(),
            plugin_ui_canvas_delta: "1.0".to_string(),

            raw_event_json: "".to_string(),
        }
    }
}

impl SaveState {
    fn sanitize(&mut self) {
        if self.event_tab < TAB_SELECTION || self.event_tab > TAB_RAW {
            self.event_tab = TAB_SELECTION;
        }
        self.selection_action = clamp_index_u8(self.selection_action, SELECTION_ACTIONS.len());
        self.fixture_action = clamp_index_u8(self.fixture_action, FIXTURE_ACTIONS.len());
        self.animation_action = clamp_index_u8(self.animation_action, ANIMATION_ACTIONS.len());
        self.scene_action = clamp_index_u8(self.scene_action, SCENE_ACTIONS.len());
        self.override_action = clamp_index_u8(self.override_action, OVERRIDE_ACTIONS.len());
        self.misc_action = clamp_index_u8(self.misc_action, MISC_ACTIONS.len());
        self.main_ui_action = clamp_index_u8(self.main_ui_action, MAIN_UI_ACTIONS.len());
        self.plugin_ui_action = clamp_index_u8(self.plugin_ui_action, PLUGIN_UI_ACTIONS.len());
        self.add_to_property = clamp_index_u8(self.add_to_property, PROPERTY_LABELS.len());
        self.remove_change_property =
            clamp_index_u8(self.remove_change_property, PROPERTY_LABELS.len());
        self.animation_speed = clamp_index_u8(self.animation_speed, AnimationSpeedModifier::ALL.len());
        self.scene_master_speed =
            clamp_index_u8(self.scene_master_speed, AnimationSpeedModifier::ALL.len());
        self.main_ui_page = clamp_index_u8(self.main_ui_page, APP_PAGE_LABELS.len());
    }
}

pub struct SamplePlugin {
    state: SaveState,
    saved: bool,
    staged_events: Vec<ControlEvent>,
    last_error: String,
    last_sent: String,
}

impl Default for SamplePlugin {
    fn default() -> Self {
        Self {
            state: SaveState::default(),
            saved: false,
            staged_events: Vec::new(),
            last_error: String::new(),
            last_sent: String::new(),
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
        self.state.sanitize();
    }

    fn handle_ui_event(&mut self, event: &PluginUiEvent, plugin_id: u8, my_id: u8) {
        if plugin_id != my_id {
            return;
        }

        match event {
            PluginUiEvent::Button { id } => self.handle_button(*id),
            PluginUiEvent::Checkbox { id, checked } => match *id {
                ID_PLUGIN_UI_CHECKED => self.state.plugin_ui_checked = *checked,
                _ => {}
            },
            PluginUiEvent::Switch { id, value } => match *id {
                ID_SET_ENABLED => self.state.set_enabled = *value,
                ID_MAIN_UI_OPEN => self.state.main_ui_open = *value,
                ID_PLUGIN_UI_CHECKED => self.state.plugin_ui_checked = *value,
                _ => {}
            },
            PluginUiEvent::Slider { id, value } | PluginUiEvent::HFader { id, value } => {
                self.handle_slider(*id, *value);
            }
            PluginUiEvent::Text { id, text } => match *id {
                ID_COLOR_HUE_TEXT => self.state.color_hue_text = text.clone(),
                ID_ADD_TO_DELTA_TEXT => self.state.add_to_property_delta = text.clone(),
                ID_ANIMATION_SPEC_JSON => self.state.animation_spec_json = text.clone(),
                ID_REMOVE_CHANGE_SELECTION_TEXT => self.state.remove_change_selection = text.clone(),
                ID_OVERLAYS_TEXT => self.state.overlays_text = text.clone(),
                ID_OVERRIDE_UNIVERSE_TEXT => self.state.override_universe_text = text.clone(),
                ID_OVERRIDE_CHANNEL_TEXT => self.state.override_channel_text = text.clone(),
                ID_PLUGIN_UI_TEXT => self.state.plugin_ui_text = text.clone(),
                ID_PLUGIN_UI_CANVAS_X => self.state.plugin_ui_canvas_x = text.clone(),
                ID_PLUGIN_UI_CANVAS_Y => self.state.plugin_ui_canvas_y = text.clone(),
                ID_PLUGIN_UI_CANVAS_DX => self.state.plugin_ui_canvas_dx = text.clone(),
                ID_PLUGIN_UI_CANVAS_DY => self.state.plugin_ui_canvas_dy = text.clone(),
                ID_PLUGIN_UI_CANVAS_DELTA => self.state.plugin_ui_canvas_delta = text.clone(),
                ID_RAW_EVENT_JSON => self.state.raw_event_json = text.clone(),
                _ => {}
            },
            PluginUiEvent::Color { id, r, g, b, a } => match *id {
                ID_COLOR_PICKER => {
                    self.state.color_r = *r;
                    self.state.color_g = *g;
                    self.state.color_b = *b;
                    self.state.color_a = *a;
                }
                ID_PLUGIN_UI_COLOR => {
                    self.state.plugin_ui_color_r = *r;
                    self.state.plugin_ui_color_g = *g;
                    self.state.plugin_ui_color_b = *b;
                    self.state.plugin_ui_color_a = *a;
                }
                _ => {}
            },
            PluginUiEvent::TabChanged { tabs_id, tab_id } => {
                if *tabs_id == EVENT_TABS_ID {
                    self.state.event_tab = *tab_id;
                }
            }
            _ => {}
        }
    }

    fn handle_slider(&mut self, id: u8, value: u8) {
        match id {
            ID_SELECTION_GROUP => self.state.selection_group_id = value,
            ID_SELECTION_FIXTURE => self.state.selection_fixture_id = value,
            ID_SET_ALPHA => self.state.set_alpha = value,
            ID_STROBE => self.state.strobe_speed = value,
            ID_FOCUS => self.state.focus = value,
            ID_TILT => self.state.tilt = value,
            ID_PAN => self.state.pan = value,
            ID_COLOR_SAT => self.state.color_saturation = value,
            ID_COLOR_VAL => self.state.color_value = value,
            ID_ANIMATION_ID => self.state.animation_id = value,
            ID_MISC_DESCRIPTOR => self.state.misc_descriptor = value,
            ID_MISC_VALUE => self.state.misc_value = value,
            ID_SCENE_ID => self.state.scene_id = value,
            ID_SCENE_MASTER_ALPHA => self.state.scene_master_alpha = value,
            ID_OVERRIDE_VALUE => self.state.override_value = value,
            ID_MAIN_UI_PLUGIN_ID => self.state.main_ui_plugin_id = value,
            ID_PLUGIN_UI_TARGET_PLUGIN_ID => self.state.plugin_ui_target_plugin_id = value,
            ID_PLUGIN_UI_ID => self.state.plugin_ui_id = value,
            ID_PLUGIN_UI_VALUE => self.state.plugin_ui_value = value,
            ID_PLUGIN_UI_TABS_ID => self.state.plugin_ui_tabs_id = value,
            ID_PLUGIN_UI_TAB_ID => self.state.plugin_ui_tab_id = value,
            _ => {}
        }
    }

    fn handle_button(&mut self, id: u8) {
        if id == BTN_SEND_EVENT {
            self.send_event();
            return;
        }
        if id == BTN_ADD_TO_TX {
            self.add_to_transaction();
            return;
        }
        if id == BTN_CLEAR_TX {
            self.staged_events.clear();
            self.last_error.clear();
            return;
        }
        if id == BTN_SEND_TX {
            self.send_transaction();
            return;
        }

        if let Some(index) = checked_action_index(id, BTN_SELECTION_ACTION_BASE, SELECTION_ACTIONS.len()) {
            self.state.selection_action = index;
            return;
        }
        if let Some(index) = checked_action_index(id, BTN_FIXTURE_ACTION_BASE, FIXTURE_ACTIONS.len()) {
            self.state.fixture_action = index;
            return;
        }
        if let Some(index) = checked_action_index(id, BTN_ANIMATION_ACTION_BASE, ANIMATION_ACTIONS.len()) {
            self.state.animation_action = index;
            return;
        }
        if let Some(index) = checked_action_index(id, BTN_SCENE_ACTION_BASE, SCENE_ACTIONS.len()) {
            self.state.scene_action = index;
            return;
        }
        if let Some(index) = checked_action_index(id, BTN_OVERRIDE_ACTION_BASE, OVERRIDE_ACTIONS.len()) {
            self.state.override_action = index;
            return;
        }
        if let Some(index) = checked_action_index(id, BTN_MISC_ACTION_BASE, MISC_ACTIONS.len()) {
            self.state.misc_action = index;
            return;
        }
        if let Some(index) = checked_action_index(id, BTN_MAIN_UI_ACTION_BASE, MAIN_UI_ACTIONS.len()) {
            self.state.main_ui_action = index;
            return;
        }
        if let Some(index) = checked_action_index(id, BTN_PLUGIN_UI_ACTION_BASE, PLUGIN_UI_ACTIONS.len()) {
            self.state.plugin_ui_action = index;
            return;
        }
        if let Some(index) = checked_action_index(id, BTN_PROPERTY_BASE, PROPERTY_LABELS.len()) {
            if self.state.fixture_action == 10 {
                self.state.add_to_property = index;
            } else if self.state.misc_action == 1 {
                self.state.remove_change_property = index;
            }
            return;
        }
        if let Some(index) = checked_action_index(id, BTN_SPEED_BASE, AnimationSpeedModifier::ALL.len()) {
            if self.state.animation_action == 5 {
                self.state.animation_speed = index;
            } else if self.state.scene_action == 5 {
                self.state.scene_master_speed = index;
            }
            return;
        }
        if let Some(index) = checked_action_index(id, BTN_APP_PAGE_BASE, APP_PAGE_LABELS.len()) {
            self.state.main_ui_page = index;
            return;
        }
    }

    fn send_event(&mut self) {
        match self.build_event() {
            Ok(ev) => {
                bpf::send_event(ev.clone());
                self.last_sent = format!("{ev:?}");
                self.last_error.clear();
            }
            Err(err) => {
                self.last_error = err;
            }
        }
    }

    fn add_to_transaction(&mut self) {
        match self.build_event() {
            Ok(ev) => {
                self.staged_events.push(ev);
                self.last_error.clear();
            }
            Err(err) => {
                self.last_error = err;
            }
        }
    }

    fn send_transaction(&mut self) {
        if self.staged_events.is_empty() {
            self.last_error = "Transaction is empty".to_string();
            return;
        }
        let evs = self.staged_events.clone();
        bpf::send_event(ControlEvent::Transaction(evs));
        self.staged_events.clear();
        self.last_sent = "Transaction(...)".to_string();
        self.last_error.clear();
    }

    fn build_event(&self) -> Result<ControlEvent, String> {
        match self.state.event_tab {
            TAB_SELECTION => self.build_selection_event(),
            TAB_FIXTURE => self.build_fixture_event(),
            TAB_ANIMATION => self.build_animation_event(),
            TAB_SCENE => self.build_scene_event(),
            TAB_OVERRIDE => self.build_override_event(),
            TAB_MISC => self.build_misc_event(),
            TAB_MAIN_UI => self.build_main_ui_event(),
            TAB_PLUGIN_UI => self.build_plugin_ui_event(),
            TAB_RAW => self.build_raw_event(),
            _ => Err("Unknown event tab".to_string()),
        }
    }

    fn build_selection_event(&self) -> Result<ControlEvent, String> {
        let idx = clamp_index(self.state.selection_action, SELECTION_ACTIONS.len());
        Ok(match idx {
            0 => ControlEvent::SelectGroup(self.state.selection_group_id),
            1 => ControlEvent::DeSelectGroup(self.state.selection_group_id),
            2 => ControlEvent::LimitSelectionToFixtureInCurrentGroup(self.state.selection_fixture_id),
            3 => ControlEvent::UnLimitSelectionToFixtureInCurrentGroup(self.state.selection_fixture_id),
            4 => ControlEvent::RemoveSelection,
            5 => ControlEvent::RemoveAllSelection,
            6 => ControlEvent::PushSelection,
            7 => ControlEvent::PopSelection,
            _ => return Err("Invalid selection action".to_string()),
        })
    }

    fn build_fixture_event(&self) -> Result<ControlEvent, String> {
        let idx = clamp_index(self.state.fixture_action, FIXTURE_ACTIONS.len());
        Ok(match idx {
            0 => ControlEvent::SetEnabled(self.state.set_enabled),
            1 => ControlEvent::SetAlpha(self.state.set_alpha),
            2 => ControlEvent::SetStrobeSpeed(self.state.strobe_speed),
            3 => ControlEvent::SetFocus(self.state.focus),
            4 => ControlEvent::SetTilt(self.state.tilt),
            5 => ControlEvent::SetPan(self.state.pan),
            6 => ControlEvent::SetColor((self.state.color_r, self.state.color_g, self.state.color_b)),
            7 => {
                let hue = parse_u16(&self.state.color_hue_text, "Hue")?;
                ControlEvent::SetColorHue(hue)
            }
            8 => ControlEvent::SetColorSaturation(self.state.color_saturation),
            9 => ControlEvent::SetColorValue(self.state.color_value),
            10 => {
                let delta = parse_i8(&self.state.add_to_property_delta, "Delta")?;
                ControlEvent::AddToProperty(property_from_index(self.state.add_to_property), delta)
            }
            _ => return Err("Invalid fixture action".to_string()),
        })
    }

    fn build_animation_event(&self) -> Result<ControlEvent, String> {
        let idx = clamp_index(self.state.animation_action, ANIMATION_ACTIONS.len());
        let anim_id = self.state.animation_id;
        Ok(match idx {
            0 => ControlEvent::AddAnimation(anim_id),
            1 => ControlEvent::RemoveAnimation(anim_id),
            2 => ControlEvent::ResetAnimation(anim_id),
            3 => ControlEvent::PauseAnimation(anim_id),
            4 => ControlEvent::PlayAnimation(anim_id),
            5 => ControlEvent::SetAnimationSpeed(anim_id, speed_from_index(self.state.animation_speed)),
            6 => {
                let spec: AnimationSpec = serde_json::from_str(&self.state.animation_spec_json)
                    .map_err(|e| format!("Spec JSON error: {e}"))?;
                ControlEvent::LoadSpecIntoAnimation(anim_id, spec)
            }
            _ => return Err("Invalid animation action".to_string()),
        })
    }

    fn build_scene_event(&self) -> Result<ControlEvent, String> {
        let idx = clamp_index(self.state.scene_action, SCENE_ACTIONS.len());
        let scene_id = self.state.scene_id;
        Ok(match idx {
            0 => ControlEvent::SetSceneFocus(scene_id),
            1 => {
                let overlays = parse_u8_list(&self.state.overlays_text, "Overlays", true)?;
                ControlEvent::SetOverlays(overlays)
            }
            2 => ControlEvent::RemoveOverlayScene(scene_id),
            3 => ControlEvent::AddOverlayScene(scene_id),
            4 => ControlEvent::SetSceneMasterAlpha(scene_id, self.state.scene_master_alpha),
            5 => ControlEvent::SetSceneMasterSpeed(scene_id, speed_from_index(self.state.scene_master_speed)),
            _ => return Err("Invalid scene action".to_string()),
        })
    }

    fn build_override_event(&self) -> Result<ControlEvent, String> {
        let idx = clamp_index(self.state.override_action, OVERRIDE_ACTIONS.len());
        let universe = parse_u16(&self.state.override_universe_text, "Universe")?;
        let channel = parse_u16(&self.state.override_channel_text, "Channel")?;
        Ok(match idx {
            0 => ControlEvent::SetChannelOverride(universe, channel, self.state.override_value),
            1 => ControlEvent::RemoveChannelOverride(universe, channel),
            _ => return Err("Invalid override action".to_string()),
        })
    }

    fn build_misc_event(&self) -> Result<ControlEvent, String> {
        let idx = clamp_index(self.state.misc_action, MISC_ACTIONS.len());
        Ok(match idx {
            0 => ControlEvent::MiscEvent {
                descriptor: self.state.misc_descriptor,
                value: self.state.misc_value,
            },
            1 => {
                let selection = parse_fixture_selection(&self.state.remove_change_selection)?;
                let property = property_from_index(self.state.remove_change_property);
                ControlEvent::RemoveChange { selection, property }
            }
            _ => return Err("Invalid misc action".to_string()),
        })
    }

    fn build_main_ui_event(&self) -> Result<ControlEvent, String> {
        let idx = clamp_index(self.state.main_ui_action, MAIN_UI_ACTIONS.len());
        Ok(match idx {
            0 => ControlEvent::MainUi(MainUiEvent::NavigatePage(page_from_index(self.state.main_ui_page))),
            1 => ControlEvent::MainUi(MainUiEvent::SetPluginUIOpen {
                plugin_id: self.state.main_ui_plugin_id,
                open: self.state.main_ui_open,
            }),
            _ => return Err("Invalid main UI action".to_string()),
        })
    }

    fn build_plugin_ui_event(&self) -> Result<ControlEvent, String> {
        let idx = clamp_index(self.state.plugin_ui_action, PLUGIN_UI_ACTIONS.len());
        let target_id = self.state.plugin_ui_target_plugin_id;
        let ev = match idx {
            0 => PluginUiEvent::Button { id: self.state.plugin_ui_id },
            1 => PluginUiEvent::Checkbox {
                id: self.state.plugin_ui_id,
                checked: self.state.plugin_ui_checked,
            },
            2 => PluginUiEvent::Switch {
                id: self.state.plugin_ui_id,
                value: self.state.plugin_ui_checked,
            },
            3 => PluginUiEvent::Slider {
                id: self.state.plugin_ui_id,
                value: self.state.plugin_ui_value,
            },
            4 => PluginUiEvent::HFader {
                id: self.state.plugin_ui_id,
                value: self.state.plugin_ui_value,
            },
            5 => PluginUiEvent::Text {
                id: self.state.plugin_ui_id,
                text: self.state.plugin_ui_text.clone(),
            },
            6 => PluginUiEvent::Color {
                id: self.state.plugin_ui_id,
                r: self.state.plugin_ui_color_r,
                g: self.state.plugin_ui_color_g,
                b: self.state.plugin_ui_color_b,
                a: self.state.plugin_ui_color_a,
            },
            7 => PluginUiEvent::TabChanged {
                tabs_id: self.state.plugin_ui_tabs_id,
                tab_id: self.state.plugin_ui_tab_id,
            },
            8 => PluginUiEvent::CanvasClick {
                id: self.state.plugin_ui_id,
                x: parse_i32(&self.state.plugin_ui_canvas_x, "X")?,
                y: parse_i32(&self.state.plugin_ui_canvas_y, "Y")?,
            },
            9 => PluginUiEvent::CanvasDrag {
                id: self.state.plugin_ui_id,
                x: parse_i32(&self.state.plugin_ui_canvas_x, "X")?,
                y: parse_i32(&self.state.plugin_ui_canvas_y, "Y")?,
                dx: parse_i32(&self.state.plugin_ui_canvas_dx, "dX")?,
                dy: parse_i32(&self.state.plugin_ui_canvas_dy, "dY")?,
            },
            10 => PluginUiEvent::CanvasPinch {
                id: self.state.plugin_ui_id,
                x: parse_i32(&self.state.plugin_ui_canvas_x, "X")?,
                y: parse_i32(&self.state.plugin_ui_canvas_y, "Y")?,
                delta: parse_f32(&self.state.plugin_ui_canvas_delta, "Delta")?,
            },
            _ => return Err("Invalid plugin UI action".to_string()),
        };
        Ok(ControlEvent::PluginUi(ev, target_id))
    }

    fn build_raw_event(&self) -> Result<ControlEvent, String> {
        let txt = self.state.raw_event_json.trim();
        if txt.is_empty() {
            return Err("Raw JSON is empty".to_string());
        }
        serde_json::from_str::<ControlEvent>(txt).map_err(|e| format!("Raw JSON error: {e}"))
    }

    fn render_ui(&self, dmx: &EngineState) {
        ui::begin();
        ui::begin_horizontal();

        ui::begin_frame_styled(42, "Event Console", 8, 8, 4, 4);
        self.render_event_console();
        ui::end_frame();

        ui::begin_frame_styled(43, "DMX Snapshot", 8, 8, 4, 4);
        self.render_dmx_snapshot(dmx);
        ui::end_frame();

        ui::end_horizontal();
    }

    fn render_event_console(&self) {
        ui::label("Craft ControlEvents and send them into the DMX engine.");
        ui::begin_tabs(EVENT_TABS_ID);

        ui::begin_tab(EVENT_TABS_ID, TAB_SELECTION, "Selection");
        self.render_selection_tab();
        ui::end_tab();

        ui::begin_tab(EVENT_TABS_ID, TAB_FIXTURE, "Fixtures");
        self.render_fixture_tab();
        ui::end_tab();

        ui::begin_tab(EVENT_TABS_ID, TAB_ANIMATION, "Animations");
        self.render_animation_tab();
        ui::end_tab();

        ui::begin_tab(EVENT_TABS_ID, TAB_SCENE, "Scenes");
        self.render_scene_tab();
        ui::end_tab();

        ui::begin_tab(EVENT_TABS_ID, TAB_OVERRIDE, "Overrides");
        self.render_override_tab();
        ui::end_tab();

        ui::begin_tab(EVENT_TABS_ID, TAB_MISC, "Misc");
        self.render_misc_tab();
        ui::end_tab();

        ui::begin_tab(EVENT_TABS_ID, TAB_MAIN_UI, "Main UI");
        self.render_main_ui_tab();
        ui::end_tab();

        ui::begin_tab(EVENT_TABS_ID, TAB_PLUGIN_UI, "Plugin UI");
        self.render_plugin_ui_tab();
        ui::end_tab();

        ui::begin_tab(EVENT_TABS_ID, TAB_RAW, "Raw");
        self.render_raw_tab();
        ui::end_tab();

        ui::end_tabs();

        ui::separator();
        ui::label(&format!("Staged events: {}", self.staged_events.len()));
        for (idx, ev) in self.staged_events.iter().take(6).enumerate() {
            ui::label(&format!("#{} {ev:?}", idx + 1));
        }
        if self.staged_events.len() > 6 {
            ui::label(&format!("... and {} more", self.staged_events.len() - 6));
        }
        if !self.last_sent.is_empty() {
            ui::label(&format!("Last sent: {}", self.last_sent));
        }
        if !self.last_error.is_empty() {
            ui::label(&format!("Last error: {}", self.last_error));
        }

        ui::begin_horizontal();
        ui::button("Send Event", BTN_SEND_EVENT);
        ui::button("Add To Transaction", BTN_ADD_TO_TX);
        ui::button("Clear Transaction", BTN_CLEAR_TX);
        ui::button("Send Transaction", BTN_SEND_TX);
        ui::end_horizontal();
    }

    fn render_selection_tab(&self) {
        ui::label("Action");
        render_buttons(SELECTION_ACTIONS.as_slice(), BTN_SELECTION_ACTION_BASE, 4);
        let idx = clamp_index(self.state.selection_action, SELECTION_ACTIONS.len());
        ui::label(&format!("Selected: {}", SELECTION_ACTIONS[idx]));

        match idx {
            0 | 1 => {
                ui::slider("Group Id", ID_SELECTION_GROUP, 0, u8::MAX, self.state.selection_group_id);
            }
            2 | 3 => {
                ui::slider(
                    "Fixture Id",
                    ID_SELECTION_FIXTURE,
                    0,
                    u8::MAX,
                    self.state.selection_fixture_id,
                );
            }
            _ => {
                ui::label("No parameters.");
            }
        }
    }

    fn render_fixture_tab(&self) {
        ui::label("Action");
        render_buttons(FIXTURE_ACTIONS.as_slice(), BTN_FIXTURE_ACTION_BASE, 4);
        let idx = clamp_index(self.state.fixture_action, FIXTURE_ACTIONS.len());
        ui::label(&format!("Selected: {}", FIXTURE_ACTIONS[idx]));

        match idx {
            0 => ui::switch("Enabled", ID_SET_ENABLED, self.state.set_enabled),
            1 => ui::slider("Alpha", ID_SET_ALPHA, 0, u8::MAX, self.state.set_alpha),
            2 => ui::slider("Strobe", ID_STROBE, 0, u8::MAX, self.state.strobe_speed),
            3 => ui::slider("Focus", ID_FOCUS, 0, u8::MAX, self.state.focus),
            4 => ui::slider("Tilt", ID_TILT, 0, u8::MAX, self.state.tilt),
            5 => ui::slider("Pan", ID_PAN, 0, u8::MAX, self.state.pan),
            6 => {
                ui::label("RGB Color");
                ui::color_picker(
                    ID_COLOR_PICKER,
                    self.state.color_r,
                    self.state.color_g,
                    self.state.color_b,
                    self.state.color_a,
                );
            }
            7 => {
                ui::text_edit("Hue (0-360)", ID_COLOR_HUE_TEXT, &self.state.color_hue_text);
            }
            8 => ui::slider(
                "Saturation",
                ID_COLOR_SAT,
                0,
                u8::MAX,
                self.state.color_saturation,
            ),
            9 => ui::slider("Value", ID_COLOR_VAL, 0, u8::MAX, self.state.color_value),
            10 => {
                ui::label("Property");
                render_buttons(PROPERTY_LABELS.as_slice(), BTN_PROPERTY_BASE, 4);
                let prop_idx = clamp_index(self.state.add_to_property, PROPERTY_LABELS.len());
                ui::label(&format!("Selected: {}", PROPERTY_LABELS[prop_idx]));
                ui::text_edit("Delta (-128..127)", ID_ADD_TO_DELTA_TEXT, &self.state.add_to_property_delta);
            }
            _ => {}
        }
    }

    fn render_animation_tab(&self) {
        ui::label("Action");
        render_buttons(ANIMATION_ACTIONS.as_slice(), BTN_ANIMATION_ACTION_BASE, 3);
        let idx = clamp_index(self.state.animation_action, ANIMATION_ACTIONS.len());
        ui::label(&format!("Selected: {}", ANIMATION_ACTIONS[idx]));

        ui::slider("Animation Id", ID_ANIMATION_ID, 0, u8::MAX, self.state.animation_id);

        match idx {
            5 => {
                ui::label("Speed Modifier");
                render_speed_buttons(BTN_SPEED_BASE, 5);
                let s_idx = clamp_index(self.state.animation_speed, AnimationSpeedModifier::ALL.len());
                ui::label(&format!("Selected: {}", AnimationSpeedModifier::ALL[s_idx].as_str()));
            }
            6 => {
                ui::text_edit_multiline(
                    "AnimationSpec JSON",
                    ID_ANIMATION_SPEC_JSON,
                    &self.state.animation_spec_json,
                );
            }
            _ => {}
        }
    }

    fn render_scene_tab(&self) {
        ui::label("Action");
        render_buttons(SCENE_ACTIONS.as_slice(), BTN_SCENE_ACTION_BASE, 3);
        let idx = clamp_index(self.state.scene_action, SCENE_ACTIONS.len());
        ui::label(&format!("Selected: {}", SCENE_ACTIONS[idx]));

        ui::slider("Scene Id", ID_SCENE_ID, 0, u8::MAX, self.state.scene_id);

        match idx {
            1 => {
                ui::text_edit("Overlay Ids (comma)", ID_OVERLAYS_TEXT, &self.state.overlays_text);
            }
            4 => {
                ui::slider(
                    "Master Alpha",
                    ID_SCENE_MASTER_ALPHA,
                    0,
                    u8::MAX,
                    self.state.scene_master_alpha,
                );
            }
            5 => {
                ui::label("Speed Modifier");
                render_speed_buttons(BTN_SPEED_BASE, 5);
                let s_idx = clamp_index(self.state.scene_master_speed, AnimationSpeedModifier::ALL.len());
                ui::label(&format!("Selected: {}", AnimationSpeedModifier::ALL[s_idx].as_str()));
            }
            _ => {}
        }
    }

    fn render_override_tab(&self) {
        ui::label("Action");
        render_buttons(OVERRIDE_ACTIONS.as_slice(), BTN_OVERRIDE_ACTION_BASE, 2);
        let idx = clamp_index(self.state.override_action, OVERRIDE_ACTIONS.len());
        ui::label(&format!("Selected: {}", OVERRIDE_ACTIONS[idx]));

        ui::text_edit("Universe", ID_OVERRIDE_UNIVERSE_TEXT, &self.state.override_universe_text);
        ui::text_edit("Channel", ID_OVERRIDE_CHANNEL_TEXT, &self.state.override_channel_text);

        if idx == 0 {
            ui::slider(
                "Value",
                ID_OVERRIDE_VALUE,
                0,
                u8::MAX,
                self.state.override_value,
            );
        }
    }

    fn render_misc_tab(&self) {
        ui::label("Action");
        render_buttons(MISC_ACTIONS.as_slice(), BTN_MISC_ACTION_BASE, 2);
        let idx = clamp_index(self.state.misc_action, MISC_ACTIONS.len());
        ui::label(&format!("Selected: {}", MISC_ACTIONS[idx]));

        match idx {
            0 => {
                ui::slider(
                    "Descriptor",
                    ID_MISC_DESCRIPTOR,
                    0,
                    u8::MAX,
                    self.state.misc_descriptor,
                );
                ui::slider("Value", ID_MISC_VALUE, 0, u8::MAX, self.state.misc_value);
            }
            1 => {
                ui::text_edit(
                    "Selection (g:f,g:f)",
                    ID_REMOVE_CHANGE_SELECTION_TEXT,
                    &self.state.remove_change_selection,
                );
                ui::label("Property");
                render_buttons(PROPERTY_LABELS.as_slice(), BTN_PROPERTY_BASE, 4);
                let prop_idx = clamp_index(self.state.remove_change_property, PROPERTY_LABELS.len());
                ui::label(&format!("Selected: {}", PROPERTY_LABELS[prop_idx]));
            }
            _ => {}
        }
    }

    fn render_main_ui_tab(&self) {
        ui::label("Action");
        render_buttons(MAIN_UI_ACTIONS.as_slice(), BTN_MAIN_UI_ACTION_BASE, 2);
        let idx = clamp_index(self.state.main_ui_action, MAIN_UI_ACTIONS.len());
        ui::label(&format!("Selected: {}", MAIN_UI_ACTIONS[idx]));

        match idx {
            0 => {
                render_buttons(APP_PAGE_LABELS.as_slice(), BTN_APP_PAGE_BASE, 4);
                let page_idx = clamp_index(self.state.main_ui_page, APP_PAGE_LABELS.len());
                ui::label(&format!("Selected: {}", APP_PAGE_LABELS[page_idx]));
            }
            1 => {
                ui::slider(
                    "Plugin Id",
                    ID_MAIN_UI_PLUGIN_ID,
                    0,
                    u8::MAX,
                    self.state.main_ui_plugin_id,
                );
                ui::switch("Open", ID_MAIN_UI_OPEN, self.state.main_ui_open);
            }
            _ => {}
        }
    }

    fn render_plugin_ui_tab(&self) {
        ui::label("Action");
        render_buttons(PLUGIN_UI_ACTIONS.as_slice(), BTN_PLUGIN_UI_ACTION_BASE, 3);
        let idx = clamp_index(self.state.plugin_ui_action, PLUGIN_UI_ACTIONS.len());
        ui::label(&format!("Selected: {}", PLUGIN_UI_ACTIONS[idx]));

        ui::slider(
            "Target Plugin Id",
            ID_PLUGIN_UI_TARGET_PLUGIN_ID,
            0,
            u8::MAX,
            self.state.plugin_ui_target_plugin_id,
        );

        match idx {
            0 => {
                ui::slider("Control Id", ID_PLUGIN_UI_ID, 0, u8::MAX, self.state.plugin_ui_id);
            }
            1 => {
                ui::slider("Control Id", ID_PLUGIN_UI_ID, 0, u8::MAX, self.state.plugin_ui_id);
                ui::checkbox("Checked", ID_PLUGIN_UI_CHECKED, self.state.plugin_ui_checked);
            }
            2 => {
                ui::slider("Control Id", ID_PLUGIN_UI_ID, 0, u8::MAX, self.state.plugin_ui_id);
                ui::switch("Value", ID_PLUGIN_UI_CHECKED, self.state.plugin_ui_checked);
            }
            3 | 4 => {
                ui::slider("Control Id", ID_PLUGIN_UI_ID, 0, u8::MAX, self.state.plugin_ui_id);
                ui::slider("Value", ID_PLUGIN_UI_VALUE, 0, u8::MAX, self.state.plugin_ui_value);
            }
            5 => {
                ui::slider("Control Id", ID_PLUGIN_UI_ID, 0, u8::MAX, self.state.plugin_ui_id);
                ui::text_edit("Text", ID_PLUGIN_UI_TEXT, &self.state.plugin_ui_text);
            }
            6 => {
                ui::slider("Control Id", ID_PLUGIN_UI_ID, 0, u8::MAX, self.state.plugin_ui_id);
                ui::color_picker(
                    ID_PLUGIN_UI_COLOR,
                    self.state.plugin_ui_color_r,
                    self.state.plugin_ui_color_g,
                    self.state.plugin_ui_color_b,
                    self.state.plugin_ui_color_a,
                );
            }
            7 => {
                ui::slider("Tabs Id", ID_PLUGIN_UI_TABS_ID, 0, u8::MAX, self.state.plugin_ui_tabs_id);
                ui::slider("Tab Id", ID_PLUGIN_UI_TAB_ID, 0, u8::MAX, self.state.plugin_ui_tab_id);
            }
            8 => {
                ui::slider("Canvas Id", ID_PLUGIN_UI_ID, 0, u8::MAX, self.state.plugin_ui_id);
                ui::text_edit("X", ID_PLUGIN_UI_CANVAS_X, &self.state.plugin_ui_canvas_x);
                ui::text_edit("Y", ID_PLUGIN_UI_CANVAS_Y, &self.state.plugin_ui_canvas_y);
            }
            9 => {
                ui::slider("Canvas Id", ID_PLUGIN_UI_ID, 0, u8::MAX, self.state.plugin_ui_id);
                ui::text_edit("X", ID_PLUGIN_UI_CANVAS_X, &self.state.plugin_ui_canvas_x);
                ui::text_edit("Y", ID_PLUGIN_UI_CANVAS_Y, &self.state.plugin_ui_canvas_y);
                ui::text_edit("dX", ID_PLUGIN_UI_CANVAS_DX, &self.state.plugin_ui_canvas_dx);
                ui::text_edit("dY", ID_PLUGIN_UI_CANVAS_DY, &self.state.plugin_ui_canvas_dy);
            }
            10 => {
                ui::slider("Canvas Id", ID_PLUGIN_UI_ID, 0, u8::MAX, self.state.plugin_ui_id);
                ui::text_edit("X", ID_PLUGIN_UI_CANVAS_X, &self.state.plugin_ui_canvas_x);
                ui::text_edit("Y", ID_PLUGIN_UI_CANVAS_Y, &self.state.plugin_ui_canvas_y);
                ui::text_edit("Delta", ID_PLUGIN_UI_CANVAS_DELTA, &self.state.plugin_ui_canvas_delta);
            }
            _ => {}
        }
    }

    fn render_raw_tab(&self) {
        ui::label("ControlEvent JSON (serde) for advanced cases.");
        ui::text_edit_multiline("Raw ControlEvent", ID_RAW_EVENT_JSON, &self.state.raw_event_json);
    }

    fn render_dmx_snapshot(&self, dmx: &EngineState) {
        let total_groups = dmx.groups.len();
        let total_fixtures: usize = dmx.groups.values().map(|g| g.fixtures.len()).sum();
        let scene_name = dmx
            .scenes
            .get(&dmx.current_scene_focus)
            .map(|s| s.name.as_str())
            .unwrap_or("unknown");

        ui::label(&format!("Groups: {} | Fixtures: {}", total_groups, total_fixtures));
        ui::label(&format!(
            "Scene focus: {} ({})",
            dmx.current_scene_focus, scene_name
        ));
        ui::label(&format!(
            "Overlays: {}",
            format_u8_list(&dmx.current_overlay_scenes)
        ));
        ui::label(&format!(
            "Selection groups: {}",
            format_u8_set(&dmx.selection.group_ids)
        ));
        ui::label(&format!(
            "Selection fixtures: {}",
            format_u8_set(&dmx.selection.fixtures_in_group)
        ));
        ui::label(&format!("Selection stack: {}", dmx.selection_stack.len()));
        ui::label(&format!("Overrides: {}", dmx.overrides.len()));
        ui::label(&format!("Animation templates: {}", dmx.animation_templates.len()));
        ui::label(&format!("Scenes: {} | Views: {}", dmx.scenes.len(), dmx.views.len()));

        ui::separator();
        ui::label("Groups (fixture counts)");
        self.render_group_bars(dmx);

        if !dmx.overrides.is_empty() {
            ui::separator();
            ui::label("Overrides (first 6)");
            for ((universe, channel), value) in dmx.overrides.iter().take(6) {
                ui::label(&format!("U{} Ch{} = {}", universe, channel, value));
            }
        }
    }

    fn render_group_bars(&self, dmx: &EngineState) {
        let mut groups: Vec<(u8, usize)> = dmx
            .groups
            .iter()
            .map(|(id, g)| (*id, g.fixtures.len()))
            .collect();
        groups.sort_by_key(|(id, _)| *id);

        if groups.is_empty() {
            ui::label("No groups.");
            return;
        }

        let max_count = groups.iter().map(|(_, c)| *c).max().unwrap_or(1).max(1);
        let max_display = groups.len().min(10);
        let width = 240;
        let height = 120;

        ui::painter_begin(DMX_PAINTER_ID, width, height);
        ui::painter_rect(0, 0, width, height, 20, 20, 20, 255);

        let bar_width = 16;
        let spacing = 8;
        let base_y = height - 20;
        let mut x = 10;

        for (gid, count) in groups.iter().take(max_display) {
            let ratio = *count as f32 / max_count as f32;
            let bar_height = (ratio * (height as f32 - 40.0)) as i32;
            let color = group_color(*gid);
            let y = base_y - bar_height;
            ui::painter_rect(x, y, bar_width, bar_height, color.0, color.1, color.2, 255);
            ui::painter_text(x - 2, base_y + 2, 10, 200, 200, 200, 255, &format!("{gid}"));
            ui::painter_text(
                x - 2,
                y - 12,
                10,
                200,
                200,
                200,
                255,
                &format!("{count}"),
            );
            x += bar_width + spacing;
        }

        ui::painter_end();

        if groups.len() > max_display {
            ui::label(&format!("Showing first {} groups", max_display));
        }
    }
}

impl Plugin for SamplePlugin {
    fn initialize(&mut self, input: TickInput) {
        self.initialize(input);
        self.state.restart_timer += 1;
        self.save();
    }

    fn run(&mut self, input: TickInput) {
        let dmx_state = bpf::get_dmx();

        for ev in &input.events.events {
            match ev.body() {
                ControlEvent::PluginUi(ui_event, plugin_id) => {
                    self.handle_ui_event(&ui_event, plugin_id, input.id);
                }
                _ => {}
            }
        }

        self.render_ui(&dmx_state);

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

fn clamp_index(idx: u8, max: usize) -> usize {
    let i = idx as usize;
    if max == 0 {
        0
    } else if i >= max {
        max - 1
    } else {
        i
    }
}

fn clamp_index_u8(idx: u8, max: usize) -> u8 {
    clamp_index(idx, max) as u8
}

fn checked_action_index(id: u8, base: u8, len: usize) -> Option<u8> {
    if id < base {
        return None;
    }
    let idx = id - base;
    if (idx as usize) < len {
        Some(idx)
    } else {
        None
    }
}

fn parse_u16(text: &str, label: &str) -> Result<u16, String> {
    text.trim()
        .parse::<u16>()
        .map_err(|_| format!("{label} must be a valid u16"))
}

fn parse_i8(text: &str, label: &str) -> Result<i8, String> {
    text.trim()
        .parse::<i8>()
        .map_err(|_| format!("{label} must be a valid i8"))
}

fn parse_i32(text: &str, label: &str) -> Result<i32, String> {
    text.trim()
        .parse::<i32>()
        .map_err(|_| format!("{label} must be a valid i32"))
}

fn parse_f32(text: &str, label: &str) -> Result<f32, String> {
    text.trim()
        .parse::<f32>()
        .map_err(|_| format!("{label} must be a valid f32"))
}

fn parse_u8_list(text: &str, label: &str, allow_empty: bool) -> Result<Vec<u8>, String> {
    let mut res = Vec::new();
    for item in text
        .split(|c| c == ',' || c == ' ' || c == ';' || c == '\n' || c == '\t')
        .filter(|s| !s.trim().is_empty())
    {
        let v = item
            .trim()
            .parse::<u8>()
            .map_err(|_| format!("{label} must contain valid u8 values"))?;
        res.push(v);
    }
    if res.is_empty() && !allow_empty {
        return Err(format!("{label} cannot be empty"));
    }
    Ok(res)
}

fn parse_fixture_selection(text: &str) -> Result<FixtureSelection, String> {
    let mut fixtures = Vec::new();
    for item in text
        .split(|c| c == ',' || c == ' ' || c == ';' || c == '\n' || c == '\t')
        .filter(|s| !s.trim().is_empty())
    {
        let trimmed = item.trim();
        let (g, f) = trimmed
            .split_once(':')
            .ok_or_else(|| "Selection must be in g:f format".to_string())?;
        let gid = g
            .trim()
            .parse::<u8>()
            .map_err(|_| "Selection group must be u8".to_string())?;
        let fid = f
            .trim()
            .parse::<u8>()
            .map_err(|_| "Selection fixture must be u8".to_string())?;
        fixtures.push((gid, fid));
    }
    if fixtures.is_empty() {
        return Err("Selection cannot be empty".to_string());
    }
    Ok(FixtureSelection { fixtures })
}

fn property_from_index(idx: u8) -> FixtureProperty {
    match clamp_index(idx, PROPERTY_LABELS.len()) {
        0 => FixtureProperty::Alpha,
        1 => FixtureProperty::Strobe,
        2 => FixtureProperty::Focus,
        3 => FixtureProperty::ColorHue,
        4 => FixtureProperty::ColorSaturation,
        5 => FixtureProperty::ColorValue,
        6 => FixtureProperty::Tilt,
        7 => FixtureProperty::Pan,
        _ => FixtureProperty::Alpha,
    }
}

fn speed_from_index(idx: u8) -> AnimationSpeedModifier {
    let i = clamp_index(idx, AnimationSpeedModifier::ALL.len());
    AnimationSpeedModifier::ALL[i]
}

fn page_from_index(idx: u8) -> AppPage {
    match clamp_index(idx, APP_PAGE_LABELS.len()) {
        0 => AppPage::Logs,
        1 => AppPage::System,
        2 => AppPage::Audio,
        3 => AppPage::FixturesSetup,
        4 => AppPage::View,
        5 => AppPage::ViewPerformance,
        6 => AppPage::FixturesPerformance,
        7 => AppPage::Animations,
        8 => AppPage::Visualizer,
        _ => AppPage::Logs,
    }
}

fn render_buttons(labels: &[&str], base: u8, per_row: usize) {
    let mut i = 0;
    while i < labels.len() {
        ui::begin_horizontal();
        for j in 0..per_row {
            if i + j >= labels.len() {
                break;
            }
            let id = base + (i + j) as u8;
            ui::button(labels[i + j], id);
        }
        ui::end_horizontal();
        i += per_row;
    }
}

fn render_speed_buttons(base: u8, per_row: usize) {
    let labels: Vec<&str> = AnimationSpeedModifier::ALL.iter().map(|s| s.as_str()).collect();
    render_buttons(labels.as_slice(), base, per_row);
}

fn format_u8_list(list: &[u8]) -> String {
    if list.is_empty() {
        return "none".to_string();
    }
    list.iter()
        .map(|v| v.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

fn format_u8_set(set: &HashSet<u8>) -> String {
    if set.is_empty() {
        return "none".to_string();
    }
    let mut ids: Vec<u8> = set.iter().copied().collect();
    ids.sort();
    ids.iter()
        .map(|v| v.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

fn group_color(group_id: u8) -> (u8, u8, u8) {
    let r = 50u8.wrapping_add(group_id.wrapping_mul(29));
    let g = 80u8.wrapping_add(group_id.wrapping_mul(53));
    let b = 120u8.wrapping_add(group_id.wrapping_mul(17));
    (r, g, b)
}
