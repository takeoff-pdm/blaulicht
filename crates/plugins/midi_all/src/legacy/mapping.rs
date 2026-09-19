//! User-configurable control mappings for the APC mini.
//!
//! Flow: "+ New mapping" puts the plugin into *learning* mode where every
//! assignable control flashes (on the digital twin and on the hardware). The
//! first control pressed is captured, then the user picks an action and an LED
//! colour. Mapped buttons glow dim in their colour and bright while their
//! target is active. Mappings persist in the showfile plugin state and
//! supersede the old pad -> view "view triggers", which are migrated on load.

use std::collections::BTreeMap;

use blaulicht_plugin_framework::{self as bpf, println, ui, MidiEvent};
use blaulicht_shared::{
    AnimationSpeedModifier, AppPage, ControlEvent, ControlEventMessage, EngineState,
    MainUiEvent, PluginUiEvent,
};
use map_range::MapRange;
use serde::{Deserialize, Serialize};

use crate::legacy::virtual_midi::{VirtualMidi, VIRTUAL_DEVICE_ID};
use crate::legacy::LegacyState;

// ---------------------------------------------------------------------------
// Widget ids (see the id table in mod.rs: 0 fans, 40 canvas, 41..44 tabs).
// ---------------------------------------------------------------------------
const NEW_MAPPING_BUTTON_ID: u8 = 45;
const CANCEL_BUTTON_ID: u8 = 46;
const SAVE_BUTTON_ID: u8 = 47;
const DELETE_BUTTON_ID: u8 = 48;
const RELEARN_BUTTON_ID: u8 = 49;
const ACTION_COMBO_ID: u8 = 51;
const TARGET_COMBO_ID: u8 = 52;
const COLOR_COMBO_ID: u8 = 53;
const EDITOR_FRAME_ID: u8 = 54;
const LIST_FRAME_ID: u8 = 55;
/// Row `i` uses `ROW_BASE_ID + 2*i` (Edit) and `ROW_BASE_ID + 2*i + 1` (Delete).
const ROW_BASE_ID: u8 = 100;
pub const MAX_MAPPINGS: usize = 64;

// ---------------------------------------------------------------------------
// LED palette (APC mini mk2 velocities).
// ---------------------------------------------------------------------------
pub const APC_LED_STATUS: u8 = 0x96;
pub const LED_OFF: u8 = 0;
pub const LED_WHITE: u8 = 3;
const LED_GREEN: u8 = 21;

/// Named colours offered in the editor. Index + 2 is the dim variant.
const COLORS: [(&str, u8); 10] = [
    ("Off", 0),
    ("White", 3),
    ("Red", 5),
    ("Orange", 9),
    ("Yellow", 13),
    ("Green", 21),
    ("Cyan", 37),
    ("Blue", 45),
    ("Magenta", 53),
    ("Pink", 57),
];

fn dim(color: u8) -> u8 {
    match color {
        0 => 0,
        1..=3 => 1,
        c => c.saturating_add(2).min(127),
    }
}

/// Learn-mode pulse: on for `LEARN_FLASH_ON` of every `LEARN_FLASH_PERIOD` ms.
/// Mostly-on reads as "these are the candidates" rather than a blink.
const LEARN_FLASH_PERIOD: u32 = 600;
const LEARN_FLASH_ON: u32 = 420;

// ---------------------------------------------------------------------------
// Reserved pads: still driven by the hardcoded legacy arms in mod.rs.
// ---------------------------------------------------------------------------
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

/// All pages, in navbar order. Stored in mappings by value (serde name).
const PAGES: [AppPage; 11] = [
    AppPage::Logs,
    AppPage::System,
    AppPage::Audio,
    AppPage::FixturesSetup,
    AppPage::View,
    AppPage::ViewPerformance,
    AppPage::FixturesPerformance,
    AppPage::Animations,
    AppPage::Palettes,
    AppPage::SceneGraph,
    AppPage::Visualizer,
];

fn page_name(page: &AppPage) -> &'static str {
    match page {
        AppPage::Logs => "Logs",
        AppPage::System => "System",
        AppPage::Audio => "Audio",
        AppPage::FixturesSetup => "Fixtures Setup",
        AppPage::View => "View",
        AppPage::ViewPerformance => "View Performance",
        AppPage::FixturesPerformance => "Fixtures Performance",
        AppPage::Animations => "Animations",
        AppPage::Palettes => "Palettes",
        AppPage::SceneGraph => "Scene Graph",
        AppPage::Visualizer => "Visualizer",
    }
}

/// Plugin ids offered for "Toggle plugin window".
const PLUGIN_ID_CHOICES: u8 = 8;

// ---------------------------------------------------------------------------
// Data model.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ControlId {
    /// Any APC note: grid pads 0..64, track buttons 100..108, scene launch
    /// 112..120, shift 122.
    ApcNote(u8),
    /// APC faders: CC 48..=55 channels, 56 master.
    ApcCc(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlKind {
    Button,
    Fader,
}

impl ControlId {
    pub fn kind(&self) -> ControlKind {
        match self {
            ControlId::ApcNote(_) => ControlKind::Button,
            ControlId::ApcCc(_) => ControlKind::Fader,
        }
    }

    pub fn label(&self) -> String {
        match *self {
            ControlId::ApcNote(n) if n < 64 => format!("Pad {n}"),
            ControlId::ApcNote(n) if (100..108).contains(&n) => format!("Track {}", n - 99),
            ControlId::ApcNote(n) if (112..120).contains(&n) => format!("Scene {}", n - 111),
            ControlId::ApcNote(122) => "Shift".to_string(),
            ControlId::ApcNote(n) => format!("Note {n}"),
            ControlId::ApcCc(c) if (48..56).contains(&c) => format!("Fader {}", c - 47),
            ControlId::ApcCc(56) => "Master".to_string(),
            ControlId::ApcCc(c) => format!("CC {c}"),
        }
    }

    /// Group (pads, side buttons, faders) then number, for a stable list order.
    fn sort_key(&self) -> (u8, u8) {
        match *self {
            ControlId::ApcNote(n) if n < 64 => (0, n),
            ControlId::ApcNote(n) => (1, n),
            ControlId::ApcCc(c) => (2, c),
        }
    }

    fn note(&self) -> Option<u8> {
        match self {
            ControlId::ApcNote(n) => Some(*n),
            ControlId::ApcCc(_) => None,
        }
    }

    /// Every control the learn mode may offer.
    fn all_assignable_notes() -> impl Iterator<Item = u8> {
        (0..64u8)
            .filter(|n| !is_reserved_pad(*n))
            .chain(100..108)
            .chain(112..120)
            .chain(std::iter::once(122))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Action {
    TriggerView(u8),
    FocusScene(u8),
    ToggleOverlay(u8),
    /// Overlay active while the button is held (press adds, release removes).
    HoldOverlay(u8),
    NavigatePage(AppPage),
    TogglePluginUi(u8),
    SceneMasterAlpha(u8),
    SceneMasterSpeed(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionKind {
    TriggerView,
    FocusScene,
    ToggleOverlay,
    HoldOverlay,
    NavigatePage,
    TogglePluginUi,
    SceneMasterAlpha,
    SceneMasterSpeed,
}

impl ActionKind {
    const BUTTON_KINDS: [ActionKind; 6] = [
        ActionKind::TriggerView,
        ActionKind::FocusScene,
        ActionKind::ToggleOverlay,
        ActionKind::HoldOverlay,
        ActionKind::NavigatePage,
        ActionKind::TogglePluginUi,
    ];
    const FADER_KINDS: [ActionKind; 2] =
        [ActionKind::SceneMasterAlpha, ActionKind::SceneMasterSpeed];

    fn for_control(kind: ControlKind) -> &'static [ActionKind] {
        match kind {
            ControlKind::Button => &Self::BUTTON_KINDS,
            ControlKind::Fader => &Self::FADER_KINDS,
        }
    }

    fn label(&self) -> &'static str {
        match self {
            ActionKind::TriggerView => "Trigger view",
            ActionKind::FocusScene => "Focus scene",
            ActionKind::ToggleOverlay => "Toggle overlay scene",
            ActionKind::HoldOverlay => "Hold overlay scene",
            ActionKind::NavigatePage => "Navigate to page",
            ActionKind::TogglePluginUi => "Toggle plugin window",
            ActionKind::SceneMasterAlpha => "Scene master alpha",
            ActionKind::SceneMasterSpeed => "Scene master speed",
        }
    }

    fn default_color(&self) -> u8 {
        match self {
            ActionKind::TriggerView => 21,      // green
            ActionKind::FocusScene => 9,        // orange
            ActionKind::ToggleOverlay => 37,    // cyan
            ActionKind::HoldOverlay => 13,      // yellow
            ActionKind::NavigatePage => 45,     // blue
            ActionKind::TogglePluginUi => 53,   // magenta
            ActionKind::SceneMasterAlpha | ActionKind::SceneMasterSpeed => 0,
        }
    }

    /// Target choices as `(code, name)`; `code` round-trips through `Action::with_target`.
    fn targets(&self, dmx: &EngineState) -> Vec<(u8, String)> {
        match self {
            ActionKind::TriggerView => dmx
                .views
                .iter()
                .map(|(id, v)| (*id, format!("{} (#{id})", v.name)))
                .collect(),
            ActionKind::FocusScene
            | ActionKind::ToggleOverlay
            | ActionKind::HoldOverlay
            | ActionKind::SceneMasterAlpha
            | ActionKind::SceneMasterSpeed => dmx
                .scenes
                .iter()
                .map(|(id, s)| (*id, format!("{} (#{id})", s.name)))
                .collect(),
            ActionKind::NavigatePage => PAGES
                .iter()
                .enumerate()
                .map(|(i, p)| (i as u8, page_name(p).to_string()))
                .collect(),
            ActionKind::TogglePluginUi => (0..PLUGIN_ID_CHOICES)
                .map(|id| (id, format!("Plugin window #{id}")))
                .collect(),
        }
    }

    fn with_target(&self, code: u8) -> Action {
        match self {
            ActionKind::TriggerView => Action::TriggerView(code),
            ActionKind::FocusScene => Action::FocusScene(code),
            ActionKind::ToggleOverlay => Action::ToggleOverlay(code),
            ActionKind::HoldOverlay => Action::HoldOverlay(code),
            ActionKind::NavigatePage => {
                Action::NavigatePage(PAGES[(code as usize).min(PAGES.len() - 1)])
            }
            ActionKind::TogglePluginUi => Action::TogglePluginUi(code),
            ActionKind::SceneMasterAlpha => Action::SceneMasterAlpha(code),
            ActionKind::SceneMasterSpeed => Action::SceneMasterSpeed(code),
        }
    }
}

impl Action {
    fn kind(&self) -> ActionKind {
        match self {
            Action::TriggerView(_) => ActionKind::TriggerView,
            Action::FocusScene(_) => ActionKind::FocusScene,
            Action::ToggleOverlay(_) => ActionKind::ToggleOverlay,
            Action::HoldOverlay(_) => ActionKind::HoldOverlay,
            Action::NavigatePage(_) => ActionKind::NavigatePage,
            Action::TogglePluginUi(_) => ActionKind::TogglePluginUi,
            Action::SceneMasterAlpha(_) => ActionKind::SceneMasterAlpha,
            Action::SceneMasterSpeed(_) => ActionKind::SceneMasterSpeed,
        }
    }

    fn target_code(&self) -> u8 {
        match self {
            Action::TriggerView(t)
            | Action::FocusScene(t)
            | Action::ToggleOverlay(t)
            | Action::HoldOverlay(t)
            | Action::TogglePluginUi(t)
            | Action::SceneMasterAlpha(t)
            | Action::SceneMasterSpeed(t) => *t,
            Action::NavigatePage(p) => PAGES.iter().position(|q| q == p).unwrap_or(0) as u8,
        }
    }

    /// Human description for list rows, e.g. "View: Default View".
    fn describe(&self, dmx: &EngineState) -> String {
        let missing = |what: &str, id: u8| format!("{what}: #{id} (missing)");
        match self {
            Action::TriggerView(id) => dmx
                .views
                .get(id)
                .map(|v| format!("View: {}", v.name))
                .unwrap_or_else(|| missing("View", *id)),
            Action::FocusScene(id) => dmx
                .scenes
                .get(id)
                .map(|s| format!("Focus scene: {}", s.name))
                .unwrap_or_else(|| missing("Focus scene", *id)),
            Action::ToggleOverlay(id) => dmx
                .scenes
                .get(id)
                .map(|s| format!("Overlay: {}", s.name))
                .unwrap_or_else(|| missing("Overlay", *id)),
            Action::HoldOverlay(id) => dmx
                .scenes
                .get(id)
                .map(|s| format!("Hold overlay: {}", s.name))
                .unwrap_or_else(|| missing("Hold overlay", *id)),
            Action::NavigatePage(p) => format!("Page: {}", page_name(p)),
            Action::TogglePluginUi(id) => format!("Plugin window #{id}"),
            Action::SceneMasterAlpha(id) => dmx
                .scenes
                .get(id)
                .map(|s| format!("Master alpha: {}", s.name))
                .unwrap_or_else(|| missing("Master alpha", *id)),
            Action::SceneMasterSpeed(id) => dmx
                .scenes
                .get(id)
                .map(|s| format!("Master speed: {}", s.name))
                .unwrap_or_else(|| missing("Master speed", *id)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Mapping {
    pub control: ControlId,
    pub action: Action,
    /// APC LED colour index (bright); 0 = off.
    pub color: u8,
}

#[derive(Serialize, Deserialize, Default)]
struct PersistedMappings {
    version: u32,
    mappings: Vec<Mapping>,
}

/// Shape written by the retired view-trigger wizard; migrated on load.
#[derive(Deserialize)]
struct OldViewTrigger {
    pad: u8,
    view_id: u8,
}
#[derive(Deserialize)]
struct OldPersistedViewTriggers {
    triggers: Vec<OldViewTrigger>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MappingEditor {
    Idle,
    /// Waiting for a control press. `editing` is the row being re-learned.
    Learning { editing: Option<usize> },
    /// Configuring a captured control. `index` is `None` for a new mapping.
    Editing { index: Option<usize>, draft: Mapping },
}

impl Default for MappingEditor {
    fn default() -> Self {
        MappingEditor::Idle
    }
}

// ---------------------------------------------------------------------------
// Persistence.
// ---------------------------------------------------------------------------

impl LegacyState {
    pub fn load_mappings(&mut self) {
        let Some(json) = bpf::load_plugin_state(bpf::PluginStateLocation::Showfile) else {
            return;
        };

        if let Ok(state) = serde_json::from_str::<PersistedMappings>(&json) {
            self.mappings = state.mappings;
            self.sort_mappings();
            println!("[mapping] loaded {} mappings", self.mappings.len());
            return;
        }

        match serde_json::from_str::<OldPersistedViewTriggers>(&json) {
            Ok(old) => {
                self.mappings = old
                    .triggers
                    .into_iter()
                    .map(|t| Mapping {
                        control: ControlId::ApcNote(t.pad),
                        action: Action::TriggerView(t.view_id),
                        color: LED_GREEN,
                    })
                    .collect();
                self.sort_mappings();
                println!(
                    "[mapping] migrated {} view triggers to mappings",
                    self.mappings.len()
                );
                self.save_mappings();
            }
            Err(e) => println!("[mapping] failed to deserialize plugin state: {e}"),
        }
    }

    pub fn save_mappings(&self) {
        let state = PersistedMappings {
            version: 1,
            mappings: self.mappings.clone(),
        };
        match serde_json::to_string(&state) {
            Ok(json) => bpf::save_plugin_state(bpf::PluginStateLocation::Showfile, &json),
            Err(e) => println!("[mapping] failed to serialize state: {e}"),
        }
    }

    fn sort_mappings(&mut self) {
        self.mappings.sort_by_key(|m| m.control.sort_key());
    }

    fn mapping_index(&self, control: ControlId) -> Option<usize> {
        self.mappings.iter().position(|m| m.control == control)
    }

    fn push_mapping_log(&mut self, msg: String) {
        println!("[mapping] {msg}");
        self.mapping_log.push_back(msg);
        while self.mapping_log.len() > 4 {
            self.mapping_log.pop_front();
        }
    }
}

// ---------------------------------------------------------------------------
// Input handling and action execution.
// ---------------------------------------------------------------------------

impl LegacyState {
    /// Handles one APC input event. Returns `true` when the event was consumed
    /// (captured by the learn mode or executed as a mapping) so the legacy
    /// hardcoded arms in `apc()` are skipped.
    pub fn handle_mapped_input(&mut self, e: &MidiEvent) -> bool {
        let synthetic = e.device == VIRTUAL_DEVICE_ID;
        let (control, value) = match (e.status, e.kind, e.value) {
            (144, note, 127) => (ControlId::ApcNote(note), None),
            // Swallow the release of a mapped/learnable pad so legacy note-off
            // arms don't fire for it; everything else passes through.
            (128, note, _) => {
                let control = ControlId::ApcNote(note);
                if let Some(idx) = self.mapping_index(control) {
                    // Twin clicks deliver press+release together; their hold
                    // actions toggled on press, so ignore the release.
                    if !synthetic && !self.editor_owns(control) {
                        let action = self.mappings[idx].action.clone();
                        self.execute_release(&action);
                    }
                    return true;
                }
                return matches!(self.mapping_editor, MappingEditor::Learning { .. })
                    && !is_reserved_pad(note);
            }
            (176, cc, v) if (48..=56).contains(&cc) => (ControlId::ApcCc(cc), Some(v)),
            _ => return false,
        };

        match self.mapping_editor.clone() {
            MappingEditor::Learning { editing } => {
                if let ControlId::ApcNote(n) = control {
                    if is_reserved_pad(n) {
                        self.push_mapping_log(format!(
                            "{} is fixed (scene/page/video column) — pick another",
                            control.label()
                        ));
                        return true;
                    }
                }

                match self.mapping_index(control) {
                    Some(existing) if Some(existing) != editing => {
                        // Already mapped: open that mapping instead of failing.
                        let draft = self.mappings[existing].clone();
                        self.mapping_editor = MappingEditor::Editing {
                            index: Some(existing),
                            draft,
                        };
                        self.push_mapping_log(format!(
                            "{} is already mapped — editing it",
                            control.label()
                        ));
                    }
                    _ => {
                        let draft = match editing {
                            Some(idx) => {
                                let mut m = self.mappings[idx].clone();
                                m.control = control;
                                if m.control.kind() != m.action.kind_control() {
                                    // Re-learned onto a different control kind:
                                    // fall back to the default action.
                                    let kind = ActionKind::for_control(control.kind())[0];
                                    m.action = kind.with_target(
                                        self.first_target(kind).unwrap_or(0),
                                    );
                                    m.color = kind.default_color();
                                }
                                m
                            }
                            None => {
                                let kind = ActionKind::for_control(control.kind())[0];
                                Mapping {
                                    control,
                                    action: kind
                                        .with_target(self.first_target(kind).unwrap_or(0)),
                                    color: kind.default_color(),
                                }
                            }
                        };
                        self.mapping_editor = MappingEditor::Editing {
                            index: editing,
                            draft,
                        };
                        self.push_mapping_log(format!("Captured {}", control.label()));
                    }
                }
                true
            }
            MappingEditor::Editing { ref draft, .. } if draft.control == control => {
                // The control being configured does nothing until saved.
                true
            }
            _ => match self.mapping_index(control) {
                Some(idx) => {
                    let action = self.mappings[idx].action.clone();
                    match action {
                        // A twin click cannot be held, so it toggles instead.
                        Action::HoldOverlay(scene) if synthetic => self.toggle_overlay(scene),
                        _ => self.execute(&action, value),
                    }
                    true
                }
                None => false,
            },
        }
    }

    /// Whether the editor currently holds `control` as its draft (its presses
    /// and releases must not execute anything).
    fn editor_owns(&self, control: ControlId) -> bool {
        matches!(&self.mapping_editor, MappingEditor::Editing { draft, .. } if draft.control == control)
    }

    fn first_target(&self, kind: ActionKind) -> Option<u8> {
        let dmx = bpf::get_dmx();
        kind.targets(&dmx).first().map(|(code, _)| *code)
    }

    fn toggle_overlay(&self, scene: u8) {
        if bpf::get_dmx().current_overlay_scenes.contains(&scene) {
            bpf::send_event(ControlEvent::RemoveOverlayScene(scene));
        } else {
            bpf::send_event(ControlEvent::AddOverlayScene(scene));
        }
    }

    /// Button release counterpart of `execute` (only hold actions care).
    fn execute_release(&mut self, action: &Action) {
        if let Action::HoldOverlay(scene) = action {
            if bpf::get_dmx().current_overlay_scenes.contains(scene) {
                bpf::send_event(ControlEvent::RemoveOverlayScene(*scene));
            }
        }
    }

    /// Fires an action. `value` is the fader position (0..=127) for
    /// continuous actions; buttons pass `None`.
    pub fn execute(&mut self, action: &Action, value: Option<u8>) {
        match action {
            Action::TriggerView(view_id) => {
                let dmx = bpf::get_dmx();
                match dmx.views.get(view_id) {
                    Some(view) => {
                        bpf::send_event(ControlEvent::Transaction(view.apply_events()))
                    }
                    None => self.push_mapping_log(format!("View #{view_id} no longer exists")),
                }
            }
            Action::FocusScene(scene) => bpf::send_event(ControlEvent::SetSceneFocus(*scene)),
            Action::ToggleOverlay(scene) => self.toggle_overlay(*scene),
            Action::HoldOverlay(scene) => {
                if !bpf::get_dmx().current_overlay_scenes.contains(scene) {
                    bpf::send_event(ControlEvent::AddOverlayScene(*scene));
                }
            }
            Action::NavigatePage(page) => {
                // Route through the page sync so the page-pad LEDs follow and
                // the NavigatePage event is emitted exactly once.
                self.current_app_page = Some(*page);
                self.page_update_from_ui = false;
                self.pending_app_page_sync = true;
            }
            Action::TogglePluginUi(plugin_id) => {
                let open = !self.plugin_ui_open.get(plugin_id).copied().unwrap_or(false);
                self.plugin_ui_open.insert(*plugin_id, open);
                bpf::send_event(ControlEvent::MainUi(MainUiEvent::SetPluginUIOpen {
                    plugin_id: *plugin_id,
                    open,
                }));
            }
            Action::SceneMasterAlpha(scene) => {
                let v = value.unwrap_or(127).min(127) as u16;
                let alpha = v.map_range(0..127, 0..100) as u8;
                bpf::send_event(ControlEvent::SetSceneMasterAlpha(*scene, alpha));
            }
            Action::SceneMasterSpeed(scene) => {
                let v = value.unwrap_or(64).min(127) as u16;
                let max_index = AnimationSpeedModifier::ALL.len() as u16 - 1;
                let index = v.map_range(0..127, 0..max_index) as usize;
                bpf::send_event(ControlEvent::SetSceneMasterSpeed(
                    *scene,
                    AnimationSpeedModifier::from_index(index),
                ));
            }
        }
    }

    /// Whether the action's target is currently active (for bright LEDs).
    fn action_active(&self, action: &Action, dmx: &EngineState) -> bool {
        match action {
            Action::TriggerView(id) => dmx.views.get(id).is_some_and(|v| {
                v.base_scene == dmx.current_scene_focus
                    && v.overlays == dmx.current_overlay_scenes
            }),
            Action::FocusScene(s) => dmx.current_scene_focus == *s,
            Action::ToggleOverlay(s) | Action::HoldOverlay(s) => {
                dmx.current_overlay_scenes.contains(s)
            }
            Action::NavigatePage(p) => self.current_app_page == Some(*p),
            Action::TogglePluginUi(id) => self.plugin_ui_open.get(id).copied().unwrap_or(false),
            Action::SceneMasterAlpha(_) | Action::SceneMasterSpeed(_) => false,
        }
    }
}

impl Action {
    fn kind_control(&self) -> ControlKind {
        match self.kind() {
            ActionKind::SceneMasterAlpha | ActionKind::SceneMasterSpeed => ControlKind::Fader,
            _ => ControlKind::Button,
        }
    }
}

// ---------------------------------------------------------------------------
// LEDs.
// ---------------------------------------------------------------------------

impl LegacyState {
    fn desired_leds(&self, clock: u32, dmx: &EngineState) -> BTreeMap<u8, u8> {
        let mut desired = BTreeMap::new();

        for m in &self.mappings {
            if let Some(note) = m.control.note() {
                let color = if self.action_active(&m.action, dmx) {
                    m.color
                } else {
                    dim(m.color)
                };
                desired.insert(note, color);
            }
        }

        match &self.mapping_editor {
            MappingEditor::Idle => {}
            MappingEditor::Learning { editing } => {
                let phase_on = clock % LEARN_FLASH_PERIOD < LEARN_FLASH_ON;
                let flash = if phase_on { LED_WHITE } else { LED_OFF };
                let relearn_note = editing.and_then(|i| self.mappings.get(i)?.control.note());
                for note in ControlId::all_assignable_notes() {
                    let mapped = self.mapping_index(ControlId::ApcNote(note)).is_some();
                    if !mapped || Some(note) == relearn_note {
                        desired.insert(note, flash);
                    }
                }
            }
            MappingEditor::Editing { draft, .. } => {
                if let Some(note) = draft.control.note() {
                    desired.insert(note, LED_WHITE);
                }
            }
        }

        desired
    }

    /// Diff-syncs mapping LEDs to the device (and therefore the twin).
    pub fn sync_mapping_leds(&mut self, conn: &VirtualMidi, clock: u32) {
        let dmx = bpf::get_dmx();
        let desired = self.desired_leds(clock, &dmx);

        for note in self.mapping_lit_colors.keys() {
            if !desired.contains_key(note) {
                conn.send(APC_LED_STATUS, *note, LED_OFF);
            }
        }
        for (note, color) in &desired {
            if self.mapping_lit_colors.get(note) != Some(color) {
                conn.send(APC_LED_STATUS, *note, *color);
            }
        }
        self.mapping_lit_colors = desired;
    }

    pub fn is_learning(&self) -> bool {
        matches!(self.mapping_editor, MappingEditor::Learning { .. })
    }

    /// Status line for the twin while the editor is active.
    pub fn twin_status(&self) -> Option<(String, (u8, u8, u8))> {
        match &self.mapping_editor {
            MappingEditor::Idle => None,
            MappingEditor::Learning { .. } => Some((
                "LEARN — click a flashing control to map it".to_string(),
                (240, 170, 60),
            )),
            MappingEditor::Editing { draft, .. } => Some((
                format!(
                    "Editing {} — choose the action in the Mappings tab",
                    draft.control.label()
                ),
                (235, 235, 240),
            )),
        }
    }
}

// ---------------------------------------------------------------------------
// UI.
// ---------------------------------------------------------------------------

impl LegacyState {
    pub fn render_mappings_ui(&mut self, events: &[ControlEventMessage], plugin_id: u8) {
        let dmx = bpf::get_dmx();

        match self.mapping_editor.clone() {
            MappingEditor::Idle => {
                ui::begin_horizontal();
                ui::label_styled("Mappings", 16, false);
                ui::button("+ New mapping", NEW_MAPPING_BUTTON_ID);
                ui::end_horizontal();
                ui::label("Map any pad, side button or fader of the APC mini to an action.");
            }
            MappingEditor::Learning { editing } => {
                ui::begin_frame_styled_border(
                    EDITOR_FRAME_ID,
                    if editing.is_some() {
                        "Re-learn control"
                    } else {
                        "New mapping"
                    },
                    8,
                    6,
                    0,
                    4,
                    240,
                    170,
                    60,
                    255,
                    1,
                );
                ui::label_styled(
                    "Press a control on the APC mini, or click it on the twin.",
                    14,
                    false,
                );
                ui::label("Assignable controls are flashing. Fixed pads stay dark.");
                ui::button("Cancel", CANCEL_BUTTON_ID);
                ui::end_frame();
            }
            MappingEditor::Editing { index, draft } => {
                ui::begin_frame_styled_border(
                    EDITOR_FRAME_ID,
                    if index.is_some() {
                        "Edit mapping"
                    } else {
                        "New mapping"
                    },
                    8,
                    6,
                    0,
                    4,
                    235,
                    235,
                    240,
                    255,
                    1,
                );

                ui::begin_horizontal();
                ui::label_styled(&format!("Control: {}", draft.control.label()), 14, false);
                ui::button("Re-learn", RELEARN_BUTTON_ID);
                ui::end_horizontal();

                let kinds = ActionKind::for_control(draft.control.kind());
                let kind_labels: Vec<String> = kinds.iter().map(|k| k.label().to_string()).collect();
                let kind_idx = kinds
                    .iter()
                    .position(|k| *k == draft.action.kind())
                    .unwrap_or(0);
                ui::combo_box("Action", ACTION_COMBO_ID, &kind_labels, kind_idx as u8);

                let targets = draft.action.kind().targets(&dmx);
                if targets.is_empty() {
                    ui::label("No targets available for this action.");
                } else {
                    let names: Vec<String> = targets.iter().map(|(_, n)| n.clone()).collect();
                    let sel = targets
                        .iter()
                        .position(|(code, _)| *code == draft.action.target_code())
                        .unwrap_or(0);
                    ui::combo_box("Target", TARGET_COMBO_ID, &names, sel as u8);
                }

                if draft.control.kind() == ControlKind::Button {
                    let names: Vec<String> = COLORS.iter().map(|(n, _)| n.to_string()).collect();
                    let sel = COLORS
                        .iter()
                        .position(|(_, c)| *c == draft.color)
                        .unwrap_or(0);
                    ui::combo_box("LED colour", COLOR_COMBO_ID, &names, sel as u8);
                }

                ui::begin_horizontal();
                ui::button("Save", SAVE_BUTTON_ID);
                ui::button("Cancel", CANCEL_BUTTON_ID);
                if index.is_some() {
                    ui::button("Delete", DELETE_BUTTON_ID);
                }
                ui::end_horizontal();
                ui::end_frame();
            }
        }

        ui::separator();

        if self.mappings.is_empty() {
            ui::label("No mappings yet.");
        } else {
            ui::begin_frame(LIST_FRAME_ID);
            for (idx, m) in self.mappings.iter().enumerate().take(MAX_MAPPINGS) {
                ui::begin_horizontal();
                ui::label_styled(&format!("{:<9}", m.control.label()), 13, true);
                ui::label(&format!("->  {}", m.action.describe(&dmx)));
                ui::button("Edit", ROW_BASE_ID + 2 * idx as u8);
                ui::button("Delete", ROW_BASE_ID + 2 * idx as u8 + 1);
                ui::end_horizontal();
            }
            ui::end_frame();
        }

        if !self.mapping_log.is_empty() {
            ui::separator();
            for line in &self.mapping_log {
                ui::label(line);
            }
        }

        self.process_mappings_ui_events(events, plugin_id, &dmx);
    }

    fn process_mappings_ui_events(
        &mut self,
        events: &[ControlEventMessage],
        plugin_id: u8,
        dmx: &EngineState,
    ) {
        for event in events {
            let ControlEvent::PluginUi(ui_event, pid) = event.body() else {
                continue;
            };
            if pid != plugin_id {
                continue;
            }

            match ui_event {
                PluginUiEvent::Button { id } => self.handle_mapping_button(id),
                PluginUiEvent::ComboBox { id, selected } => {
                    self.handle_mapping_combo(id, selected as usize, dmx)
                }
                _ => {}
            }
        }
    }

    fn handle_mapping_button(&mut self, id: u8) {
        match id {
            NEW_MAPPING_BUTTON_ID => {
                if self.mappings.len() >= MAX_MAPPINGS {
                    self.push_mapping_log(format!("Mapping limit ({MAX_MAPPINGS}) reached"));
                } else {
                    self.mapping_editor = MappingEditor::Learning { editing: None };
                    self.push_mapping_log("Waiting for a control…".to_string());
                }
            }
            CANCEL_BUTTON_ID => {
                self.mapping_editor = MappingEditor::Idle;
            }
            RELEARN_BUTTON_ID => {
                if let MappingEditor::Editing { index, .. } = &self.mapping_editor {
                    self.mapping_editor = MappingEditor::Learning { editing: *index };
                }
            }
            SAVE_BUTTON_ID => {
                if let MappingEditor::Editing { index, draft } = self.mapping_editor.clone() {
                    let label = format!(
                        "{} -> {}",
                        draft.control.label(),
                        draft.action.describe(&bpf::get_dmx())
                    );
                    match index {
                        Some(i) if i < self.mappings.len() => self.mappings[i] = draft,
                        _ => self.mappings.push(draft),
                    }
                    self.sort_mappings();
                    self.save_mappings();
                    self.mapping_editor = MappingEditor::Idle;
                    self.push_mapping_log(format!("Saved {label}"));
                }
            }
            DELETE_BUTTON_ID => {
                if let MappingEditor::Editing {
                    index: Some(i), ..
                } = self.mapping_editor
                {
                    self.delete_mapping(i);
                    self.mapping_editor = MappingEditor::Idle;
                }
            }
            id if id >= ROW_BASE_ID => {
                let idx = ((id - ROW_BASE_ID) / 2) as usize;
                let is_delete = (id - ROW_BASE_ID) % 2 == 1;
                if idx >= self.mappings.len() {
                    return;
                }
                if is_delete {
                    self.delete_mapping(idx);
                    self.mapping_editor = MappingEditor::Idle;
                } else {
                    self.mapping_editor = MappingEditor::Editing {
                        index: Some(idx),
                        draft: self.mappings[idx].clone(),
                    };
                }
            }
            _ => {}
        }
    }

    fn delete_mapping(&mut self, idx: usize) {
        if idx < self.mappings.len() {
            let removed = self.mappings.remove(idx);
            self.save_mappings();
            self.push_mapping_log(format!("Deleted mapping for {}", removed.control.label()));
        }
    }

    fn handle_mapping_combo(&mut self, id: u8, selected: usize, dmx: &EngineState) {
        let MappingEditor::Editing { index, mut draft } = self.mapping_editor.clone() else {
            return;
        };

        match id {
            ACTION_COMBO_ID => {
                let kinds = ActionKind::for_control(draft.control.kind());
                if let Some(kind) = kinds.get(selected) {
                    if *kind != draft.action.kind() {
                        let first = kind.targets(dmx).first().map(|(c, _)| *c).unwrap_or(0);
                        draft.action = kind.with_target(first);
                        draft.color = kind.default_color();
                    }
                }
            }
            TARGET_COMBO_ID => {
                let kind = draft.action.kind();
                if let Some((code, _)) = kind.targets(dmx).get(selected) {
                    draft.action = kind.with_target(*code);
                }
            }
            COLOR_COMBO_ID => {
                if let Some((_, color)) = COLORS.get(selected) {
                    draft.color = *color;
                }
            }
            _ => return,
        }

        self.mapping_editor = MappingEditor::Editing { index, draft };
    }
}
