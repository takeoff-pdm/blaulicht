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
    view::{View, ViewSceneMasters},
    AnimationSpeedModifier, AppPage, ControlEvent, ControlEventMessage, EngineState,
    MainUiEvent, PluginUiEvent,
};
use map_range::MapRange;
use serde::{Deserialize, Deserializer, Serialize};

use crate::legacy::virtual_midi::{VirtualMidi, VIRTUAL_DEVICE_ID};
use crate::korg::{FADER_BYTES, SELECT_BUTTON_STARTER};
use crate::legacy::LegacyState;

// ---------------------------------------------------------------------------
// Widget ids (see the id table in mod.rs: 0 fans, 40 canvas, 41..44 tabs).
// ---------------------------------------------------------------------------
const NEW_MAPPING_BUTTON_ID: u8 = 45;
const NEW_MIX_MAPPING_BUTTON_ID: u8 = 56;
const NEW_KORG_MAPPING_BUTTON_ID: u8 = 57;
/// "Hold scene alpha" value slider.
const ALPHA_SLIDER_ID: u8 = 50;
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
/// Scene checkboxes of the editor (one per scene, in `dmx.scenes` order).
/// They share the row id range, so the list is hidden while editing.
const SCENE_CHECK_BASE_ID: u8 = 100;
pub const MAX_MAPPINGS: usize = 64;

// ---------------------------------------------------------------------------
// LED palette (APC mini mk2 velocities).
// ---------------------------------------------------------------------------
pub const APC_LED_STATUS: u8 = 0x96;
/// The SHIFT button. Held (or toggled on the twin) it turns a "Hold overlay"
/// press into a latched auto-hold; it is never assignable itself. It is held
/// permanently lit at `LED_WHITE` so the modifier is always findable.
pub const SHIFT_NOTE: u8 = 122;
/// Single-LED status byte for the side buttons (track / scene launch): the
/// mk2 documents velocity 0 off, 1 on, 2 blink on channel 1, but in practice
/// the buttons only light at `LED_WHITE` (3), which is what mapped ones use.
pub const APC_SINGLE_LED_STATUS: u8 = 0x90;
pub const SINGLE_LED_ON: u8 = 1;
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

/// Latched (auto-hold) pads flash bright/dim in their own colour.
const HOLD_FLASH_PERIOD: u32 = 500;
const HOLD_FLASH_ON: u32 = 250;

// ---------------------------------------------------------------------------
// Reserved pads: still driven by the hardcoded legacy arms in mod.rs.
// ---------------------------------------------------------------------------
const SCENES: [u8; 8] = [56, 48, 40, 32, 24, 16, 8, 0];
const PAGE_PADS: [u8; 8] = [63, 55, 47, 39, 31, 23, 15, 7];

/// Side buttons (track 100..108, scene launch 112..120) have a single-colour
/// LED with its own message format; the 8x8 grid pads are RGB.
pub fn is_single_led(note: u8) -> bool {
    note >= 100
}

fn led_status(note: u8) -> u8 {
    if is_single_led(note) {
        APC_SINGLE_LED_STATUS
    } else {
        APC_LED_STATUS
    }
}

pub fn is_reserved_pad(pad: u8) -> bool {
    SCENES.contains(&pad) || PAGE_PADS.contains(&pad)
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

/// Which controller a mapping belongs to. Learn mode captures from one device
/// at a time; actions are shared by all of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MappingDevice {
    Apc,
    MidiMix,
    Korg,
}

impl MappingDevice {
    pub fn name(&self) -> &'static str {
        match self {
            MappingDevice::Apc => "APC mini",
            MappingDevice::MidiMix => "MIDI Mix",
            MappingDevice::Korg => "nanoKONTROL",
        }
    }

    /// Whether the device has an on-screen twin that can be clicked.
    fn has_twin(&self) -> bool {
        !matches!(self, MappingDevice::MidiMix)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ControlId {
    /// Any APC note: grid pads 0..64, track buttons 100..108, scene launch
    /// 112..120. Shift (122) is a modifier and cannot be mapped.
    ApcNote(u8),
    /// APC faders: CC 48..=55 channels, 56 master.
    ApcCc(u8),
    /// MIDI Mix buttons: mute `1+3c`, solo `2+3c`, rec arm `3+3c` per
    /// channel `c`, bank left 25, bank right 26, solo 27.
    MixNote(u8),
    /// MIDI Mix knobs/faders: CC 16..=31 (channels 1-4, three knobs then the
    /// fader each), 46..=61 (channels 5-8), 62 master.
    MixCc(u8),
    /// nanoKONTROL notes: select buttons 46..=53, "Set" 82.
    KorgNote(u8),
    /// nanoKONTROL CCs: faders (`FADER_BYTES`), knobs 13..=20 and the
    /// momentary CC buttons (127 on press, 0 on release).
    KorgCc(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlKind {
    Button,
    Fader,
}

/// nanoKONTROL CCs that behave as buttons (strip mute/solo/rec, transport and
/// marker cluster). Everything else on CC is an absolute fader/knob.
fn korg_cc_is_button(cc: u8) -> bool {
    matches!(cc, 21..=45 | 58 | 59 | 80 | 81 | 83 | 84)
}

fn korg_cc_is_absolute(cc: u8) -> bool {
    FADER_BYTES.contains(&cc) || (13..=20).contains(&cc)
}

/// MIDI Mix CC -> (channel 0..8, row 0..3 where 3 is the fader).
fn mix_cc_pos(cc: u8) -> Option<(u8, u8)> {
    match cc {
        16..=31 => Some(((cc - 16) / 4, (cc - 16) % 4)),
        46..=61 => Some(((cc - 46) / 4 + 4, (cc - 46) % 4)),
        _ => None,
    }
}

impl ControlId {
    pub fn device(&self) -> MappingDevice {
        match self {
            ControlId::ApcNote(_) | ControlId::ApcCc(_) => MappingDevice::Apc,
            ControlId::MixNote(_) | ControlId::MixCc(_) => MappingDevice::MidiMix,
            ControlId::KorgNote(_) | ControlId::KorgCc(_) => MappingDevice::Korg,
        }
    }

    pub fn kind(&self) -> ControlKind {
        match self {
            ControlId::ApcNote(_) | ControlId::MixNote(_) | ControlId::KorgNote(_) => {
                ControlKind::Button
            }
            ControlId::KorgCc(cc) if korg_cc_is_button(*cc) => ControlKind::Button,
            ControlId::ApcCc(_) | ControlId::MixCc(_) | ControlId::KorgCc(_) => ControlKind::Fader,
        }
    }

    pub fn label(&self) -> String {
        match *self {
            ControlId::ApcNote(n) if n < 64 => format!("Pad {n}"),
            ControlId::ApcNote(n) if (100..108).contains(&n) => format!("Track {}", n - 99),
            ControlId::ApcNote(n) if (112..120).contains(&n) => format!("Scene {}", n - 111),
            ControlId::ApcNote(SHIFT_NOTE) => "Shift".to_string(),
            ControlId::ApcNote(n) => format!("Note {n}"),
            ControlId::ApcCc(c) if (48..56).contains(&c) => format!("Fader {}", c - 47),
            ControlId::ApcCc(56) => "Master".to_string(),
            ControlId::ApcCc(c) => format!("CC {c}"),
            ControlId::MixNote(25) => "Mix bank L".to_string(),
            ControlId::MixNote(26) => "Mix bank R".to_string(),
            ControlId::MixNote(27) => "Mix solo".to_string(),
            ControlId::MixNote(n) if (1..=24).contains(&n) => {
                let (c, r) = ((n - 1) / 3 + 1, (n - 1) % 3);
                match r {
                    0 => format!("Mix mute {c}"),
                    1 => format!("Mix solo {c}"),
                    _ => format!("Mix rec {c}"),
                }
            }
            ControlId::MixNote(n) => format!("Mix note {n}"),
            ControlId::MixCc(62) => "Mix master".to_string(),
            ControlId::MixCc(c) => match mix_cc_pos(c) {
                Some((ch, 3)) => format!("Mix fader {}", ch + 1),
                Some((ch, r)) => format!("Mix knob {}.{}", ch + 1, r + 1),
                None => format!("Mix CC {c}"),
            },
            ControlId::KorgNote(n)
                if (SELECT_BUTTON_STARTER..SELECT_BUTTON_STARTER + 8).contains(&n) =>
            {
                format!("Korg select {}", n - SELECT_BUTTON_STARTER + 1)
            }
            ControlId::KorgNote(82) => "Korg set".to_string(),
            ControlId::KorgNote(n) => format!("Korg note {n}"),
            ControlId::KorgCc(c) if FADER_BYTES.contains(&c) => {
                format!("Korg fader {}", FADER_BYTES.iter().position(|b| *b == c).unwrap_or(0) + 1)
            }
            ControlId::KorgCc(c) if (13..=20).contains(&c) => format!("Korg knob {}", c - 12),
            ControlId::KorgCc(c) if (21..=28).contains(&c) => format!("Korg mute {}", c - 20),
            ControlId::KorgCc(c) if (29..=36).contains(&c) => format!("Korg solo {}", c - 28),
            ControlId::KorgCc(c) if (38..=45).contains(&c) => format!("Korg rec {}", c - 37),
            ControlId::KorgCc(c) => format!("Korg CC {c}"),
        }
    }

    /// Device, then group (pads, side buttons, faders), then number, for a
    /// stable list order.
    fn sort_key(&self) -> (u8, u8, u8) {
        match *self {
            ControlId::ApcNote(n) if n < 64 => (0, 0, n),
            ControlId::ApcNote(n) => (0, 1, n),
            ControlId::ApcCc(c) => (0, 2, c),
            ControlId::MixNote(n) => (1, 0, n),
            ControlId::MixCc(c) => (1, 1, c),
            ControlId::KorgNote(n) => (2, 0, n),
            ControlId::KorgCc(c) => (2, 1, c),
        }
    }

    /// APC LED note (the APC grid/side LEDs are the only RGB ones we drive).
    fn note(&self) -> Option<u8> {
        match self {
            ControlId::ApcNote(n) => Some(*n),
            _ => None,
        }
    }

    fn mix_note(&self) -> Option<u8> {
        match self {
            ControlId::MixNote(n) => Some(*n),
            _ => None,
        }
    }

    /// Whether the learn mode may capture this control.
    fn is_assignable(&self) -> bool {
        match self {
            ControlId::ApcNote(n) => !is_reserved_pad(*n) && *n != SHIFT_NOTE,
            _ => true,
        }
    }

    /// Every APC note the learn mode may offer.
    fn all_assignable_notes() -> impl Iterator<Item = u8> {
        (0..64u8)
            .filter(|n| !is_reserved_pad(*n))
            .chain(100..108)
            .chain(112..120)
    }

    /// MIDI Mix notes with an LED (mute, rec arm, bank L/R, solo).
    fn mix_led_notes() -> impl Iterator<Item = u8> {
        (1..=27u8).filter(|n| *n > 24 || (n - 1) % 3 != 1)
    }
}

/// Accepts the old single-scene form (`3`) as well as a list (`[3, 4]`).
fn one_or_many<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum OneOrMany {
        One(u8),
        Many(Vec<u8>),
    }
    Ok(match OneOrMany::deserialize(d)? {
        OneOrMany::One(v) => vec![v],
        OneOrMany::Many(v) => v,
    })
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Action {
    TriggerView(u8),
    /// View active while the button is held; the look present at press time is
    /// restored on release. Shift+press latches it until the button is tapped.
    HoldView(u8),
    FocusScene(u8),
    ToggleOverlay(u8),
    /// Overlay active while the button is held (press adds, release removes).
    /// Shift+press latches it (auto-hold) until the button is tapped again.
    HoldOverlay(u8),
    NavigatePage(AppPage),
    TogglePluginUi(u8),
    /// Fader -> master alpha of every listed scene.
    SceneMasterAlpha(#[serde(deserialize_with = "one_or_many")] Vec<u8>),
    /// Fader -> master speed of every listed scene.
    SceneMasterSpeed(#[serde(deserialize_with = "one_or_many")] Vec<u8>),
    /// Button: master alpha of every listed scene is `alpha` while held; the
    /// previous values are restored on release.
    HoldSceneAlpha { scenes: Vec<u8>, alpha: u8 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionKind {
    TriggerView,
    HoldView,
    FocusScene,
    ToggleOverlay,
    HoldOverlay,
    NavigatePage,
    TogglePluginUi,
    SceneMasterAlpha,
    SceneMasterSpeed,
    HoldSceneAlpha,
}

impl ActionKind {
    const BUTTON_KINDS: [ActionKind; 8] = [
        ActionKind::TriggerView,
        ActionKind::HoldView,
        ActionKind::FocusScene,
        ActionKind::ToggleOverlay,
        ActionKind::HoldOverlay,
        ActionKind::HoldSceneAlpha,
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
            ActionKind::HoldView => "Hold view (shift+press latches)",
            ActionKind::FocusScene => "Solo scene",
            ActionKind::ToggleOverlay => "Toggle overlay scene",
            ActionKind::HoldOverlay => "Hold overlay scene (shift+press latches)",
            ActionKind::NavigatePage => "Navigate to page",
            ActionKind::TogglePluginUi => "Toggle plugin window",
            ActionKind::SceneMasterAlpha => "Scene master alpha (scene list)",
            ActionKind::SceneMasterSpeed => "Scene master speed (scene list)",
            ActionKind::HoldSceneAlpha => "Hold scene alpha while pressed",
        }
    }

    fn default_color(&self) -> u8 {
        match self {
            ActionKind::TriggerView => 21,      // green
            ActionKind::HoldView => 25,         // mint
            ActionKind::FocusScene => 9,        // orange
            ActionKind::ToggleOverlay => 37,    // cyan
            ActionKind::HoldOverlay => 13,      // yellow
            ActionKind::HoldSceneAlpha => 5,    // red
            ActionKind::NavigatePage => 45,     // blue
            ActionKind::TogglePluginUi => 53,   // magenta
            ActionKind::SceneMasterAlpha | ActionKind::SceneMasterSpeed => 0,
        }
    }

    /// Target choices as `(code, name)`; `code` round-trips through `Action::with_target`.
    fn targets(&self, dmx: &EngineState) -> Vec<(u8, String)> {
        match self {
            ActionKind::TriggerView | ActionKind::HoldView => dmx
                .views
                .iter()
                .map(|(id, v)| (*id, format!("{} (#{id})", v.name)))
                .collect(),
            ActionKind::FocusScene
            | ActionKind::ToggleOverlay
            | ActionKind::HoldOverlay
            | ActionKind::SceneMasterAlpha
            | ActionKind::SceneMasterSpeed
            | ActionKind::HoldSceneAlpha => dmx
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
            ActionKind::HoldView => Action::HoldView(code),
            ActionKind::FocusScene => Action::FocusScene(code),
            ActionKind::ToggleOverlay => Action::ToggleOverlay(code),
            ActionKind::HoldOverlay => Action::HoldOverlay(code),
            ActionKind::NavigatePage => {
                Action::NavigatePage(PAGES[(code as usize).min(PAGES.len() - 1)])
            }
            ActionKind::TogglePluginUi => Action::TogglePluginUi(code),
            ActionKind::SceneMasterAlpha => Action::SceneMasterAlpha(vec![code]),
            ActionKind::SceneMasterSpeed => Action::SceneMasterSpeed(vec![code]),
            ActionKind::HoldSceneAlpha => Action::HoldSceneAlpha {
                scenes: vec![code],
                alpha: 0,
            },
        }
    }
}

impl Action {
    fn kind(&self) -> ActionKind {
        match self {
            Action::TriggerView(_) => ActionKind::TriggerView,
            Action::HoldView(_) => ActionKind::HoldView,
            Action::FocusScene(_) => ActionKind::FocusScene,
            Action::ToggleOverlay(_) => ActionKind::ToggleOverlay,
            Action::HoldOverlay(_) => ActionKind::HoldOverlay,
            Action::NavigatePage(_) => ActionKind::NavigatePage,
            Action::TogglePluginUi(_) => ActionKind::TogglePluginUi,
            Action::SceneMasterAlpha(_) => ActionKind::SceneMasterAlpha,
            Action::SceneMasterSpeed(_) => ActionKind::SceneMasterSpeed,
            Action::HoldSceneAlpha { .. } => ActionKind::HoldSceneAlpha,
        }
    }

    /// The scene list of list-targeted actions.
    fn scenes(&self) -> Option<&Vec<u8>> {
        match self {
            Action::SceneMasterAlpha(s) | Action::SceneMasterSpeed(s) => Some(s),
            Action::HoldSceneAlpha { scenes, .. } => Some(scenes),
            _ => None,
        }
    }

    fn scenes_mut(&mut self) -> Option<&mut Vec<u8>> {
        match self {
            Action::SceneMasterAlpha(s) | Action::SceneMasterSpeed(s) => Some(s),
            Action::HoldSceneAlpha { scenes, .. } => Some(scenes),
            _ => None,
        }
    }

    fn target_code(&self) -> u8 {
        match self {
            Action::TriggerView(t)
            | Action::HoldView(t)
            | Action::FocusScene(t)
            | Action::ToggleOverlay(t)
            | Action::HoldOverlay(t)
            | Action::TogglePluginUi(t) => *t,
            Action::NavigatePage(p) => PAGES.iter().position(|q| q == p).unwrap_or(0) as u8,
            _ => self.scenes().and_then(|s| s.first().copied()).unwrap_or(0),
        }
    }

    /// Human description for list rows, e.g. "View: Default View".
    fn describe(&self, dmx: &EngineState) -> String {
        let missing = |what: &str, id: u8| format!("{what}: #{id} (missing)");
        let scene_names = |ids: &[u8]| -> String {
            if ids.is_empty() {
                return "(no scenes)".to_string();
            }
            ids.iter()
                .map(|id| {
                    dmx.scenes
                        .get(id)
                        .map(|s| s.name.clone())
                        .unwrap_or_else(|| format!("#{id}?"))
                })
                .collect::<Vec<_>>()
                .join(", ")
        };
        match self {
            Action::TriggerView(id) => dmx
                .views
                .get(id)
                .map(|v| format!("View: {}", v.name))
                .unwrap_or_else(|| missing("View", *id)),
            Action::HoldView(id) => dmx
                .views
                .get(id)
                .map(|v| format!("Hold view: {}", v.name))
                .unwrap_or_else(|| missing("Hold view", *id)),
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
            Action::SceneMasterAlpha(ids) => format!("Master alpha: {}", scene_names(ids)),
            Action::SceneMasterSpeed(ids) => format!("Master speed: {}", scene_names(ids)),
            Action::HoldSceneAlpha { scenes, alpha } => {
                format!("Hold alpha {alpha}%: {}", scene_names(scenes))
            }
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
    /// Waiting for a control press on `device`. `editing` is the row being
    /// re-learned.
    Learning {
        editing: Option<usize>,
        device: MappingDevice,
    },
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
            // Shift became a modifier; a mapping on it from an older showfile
            // would swallow every shift press.
            let before = self.mappings.len();
            self.mappings
                .retain(|m| m.control != ControlId::ApcNote(SHIFT_NOTE));
            if self.mappings.len() != before {
                self.push_mapping_log("Dropped mapping on Shift (now a modifier)".to_string());
            }
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

/// What a press on a "Hold overlay" pad does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HoldPress {
    /// Pad was latched: release the auto-hold.
    Unlatch,
    /// Shift held: latch the overlay until the next tap.
    Latch,
    /// Momentary hold (hardware; the release removes the overlay).
    Hold,
    /// Twin click (press+release at once): toggle instead.
    Toggle,
}

fn hold_press_kind(latched: bool, shift_held: bool, synthetic: bool) -> HoldPress {
    if latched {
        HoldPress::Unlatch
    } else if shift_held {
        HoldPress::Latch
    } else if synthetic {
        HoldPress::Toggle
    } else {
        HoldPress::Hold
    }
}

/// A normalised control gesture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Input {
    Press,
    Release,
    /// Absolute fader/knob position 0..=127.
    Value(u8),
}

/// Maps a raw MIDI event of `device` onto a mappable control, or `None` for
/// anything the mapping system does not handle (left to the legacy arms).
fn normalise(device: MappingDevice, e: &MidiEvent) -> Option<(ControlId, Input)> {
    let (status, kind, value) = (e.status, e.kind, e.value);
    let note_on = status == 144 && value > 0;
    let note_off = status == 128 || (status == 144 && value == 0);
    match device {
        MappingDevice::Apc => match (status, kind) {
            (144 | 128, n) if note_on => Some((ControlId::ApcNote(n), Input::Press)),
            (144 | 128, n) if note_off => Some((ControlId::ApcNote(n), Input::Release)),
            (176, cc) if (48..=56).contains(&cc) => Some((ControlId::ApcCc(cc), Input::Value(value))),
            _ => None,
        },
        MappingDevice::MidiMix => match (status, kind) {
            (144 | 128, n) if (1..=27).contains(&n) && note_on => {
                Some((ControlId::MixNote(n), Input::Press))
            }
            (144 | 128, n) if (1..=27).contains(&n) && note_off => {
                Some((ControlId::MixNote(n), Input::Release))
            }
            (176, cc) if mix_cc_pos(cc).is_some() || cc == 62 => {
                Some((ControlId::MixCc(cc), Input::Value(value)))
            }
            _ => None,
        },
        MappingDevice::Korg => match (status, kind) {
            (144 | 128, n) if note_on => Some((ControlId::KorgNote(n), Input::Press)),
            (144 | 128, n) if note_off => Some((ControlId::KorgNote(n), Input::Release)),
            (176, cc) if korg_cc_is_button(cc) => Some((
                ControlId::KorgCc(cc),
                if value >= 64 { Input::Press } else { Input::Release },
            )),
            (176, cc) if korg_cc_is_absolute(cc) => {
                Some((ControlId::KorgCc(cc), Input::Value(value)))
            }
            _ => None,
        },
    }
}

/// Sets each `(scene, alpha)` pair in one transaction.
fn send_scene_alphas(pairs: Vec<(u8, u8)>) {
    send_events(
        pairs
            .into_iter()
            .map(|(scene, alpha)| ControlEvent::SetSceneMasterAlpha(scene, alpha))
            .collect(),
    );
}

/// Sends several events atomically (or the single one directly).
fn send_events(mut events: Vec<ControlEvent>) {
    match events.len() {
        0 => {}
        1 => bpf::send_event(events.pop().unwrap()),
        _ => bpf::send_event(ControlEvent::Transaction(events)),
    }
}

impl LegacyState {
    /// Handles one input event of `device`. Returns `true` when the event was
    /// consumed (captured by the learn mode or executed as a mapping) so the
    /// device's hardcoded arms are skipped.
    pub fn handle_mapped_input(&mut self, device: MappingDevice, e: &MidiEvent) -> bool {
        let synthetic = e.device == VIRTUAL_DEVICE_ID;

        if device == MappingDevice::Apc {
            match (e.status, e.kind, e.value) {
                // Shift is a modifier, never a mapping. A twin click delivers
                // press+release at once, so there it toggles instead of holding.
                (144, SHIFT_NOTE, v) if v > 0 => {
                    self.shift_held = if synthetic { !self.shift_held } else { true };
                    return true;
                }
                (128, SHIFT_NOTE, _) | (144, SHIFT_NOTE, 0) => {
                    if !synthetic {
                        self.shift_held = false;
                    }
                    return true;
                }
                _ => {}
            }
        }

        let Some((control, input)) = normalise(device, e) else {
            return false;
        };
        let learning_here =
            matches!(self.mapping_editor, MappingEditor::Learning { device: d, .. } if d == device);

        let value = match input {
            Input::Release => {
                // Swallow the release of a mapped/learnable control so legacy
                // release arms don't fire for it; everything else passes through.
                if let Some(idx) = self.mapping_index(control) {
                    // Twin clicks deliver press+release together; their hold
                    // actions toggled on press, so ignore the release. A
                    // latched (auto-hold) pad keeps its overlay past release.
                    if !synthetic
                        && !self.editor_owns(control)
                        && !self.latched_holds.contains(&control)
                    {
                        let action = self.mappings[idx].action.clone();
                        self.execute_release(control, &action);
                    }
                    return true;
                }
                return learning_here && control.is_assignable();
            }
            Input::Press => None,
            Input::Value(v) => Some(v),
        };

        match self.mapping_editor.clone() {
            MappingEditor::Learning { editing, .. } if learning_here => {
                if !control.is_assignable() {
                    self.push_mapping_log(format!(
                        "{} is fixed (scene/page column) — pick another",
                        control.label()
                    ));
                    return true;
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
                        Action::HoldOverlay(_) | Action::HoldView(_) => {
                            match hold_press_kind(
                                self.latched_holds.contains(&control),
                                self.shift_held,
                                synthetic,
                            ) {
                                HoldPress::Unlatch => {
                                    self.latched_holds.remove(&control);
                                    self.execute_release(control, &action);
                                }
                                HoldPress::Latch => {
                                    self.latched_holds.insert(control);
                                    self.execute(control, &action, value);
                                }
                                // A twin click cannot be held, so it toggles.
                                HoldPress::Toggle => match action {
                                    Action::HoldOverlay(scene) => self.toggle_overlay(scene),
                                    // `execute` restores the remembered look
                                    // when the view is already held.
                                    _ => self.execute(control, &action, value),
                                },
                                HoldPress::Hold => self.execute(control, &action, value),
                            }
                        }
                        _ => self.execute(control, &action, value),
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
        bpf::with_dmx(|dmx| kind.targets(dmx).first().map(|(code, _)| *code))
    }

    fn toggle_overlay(&self, scene: u8) {
        if bpf::with_dmx(|dmx| dmx.current_overlay_scenes.contains(&scene)) {
            bpf::send_event(ControlEvent::RemoveOverlayScene(scene));
        } else {
            bpf::send_event(ControlEvent::AddOverlayScene(scene));
        }
    }

    /// Sends the alphas remembered at press time back and forgets them.
    fn restore_held_alpha(&mut self, control: ControlId) {
        if let Some(prev) = self.held_alphas.remove(&control) {
            send_scene_alphas(prev);
        }
    }

    /// The look currently on stage, as a view: the overlay stack plus the
    /// master alpha / speed of every scene in it.
    fn current_look(dmx: &EngineState) -> View {
        let overlays = dmx.current_overlay_scenes.clone();
        let masters = overlays
            .iter()
            .filter_map(|id| {
                dmx.scenes.get(id).map(|s| {
                    (
                        *id,
                        ViewSceneMasters {
                            master_alpha: s.sink.master_alpha_fader,
                            master_speed: s.sink.master_speed,
                        },
                    )
                })
            })
            .collect();
        View {
            name: String::new(),
            overlays,
            masters,
        }
    }

    /// Puts the look remembered at press time back and forgets it.
    fn restore_held_view(&mut self, control: ControlId) {
        if let Some(prev) = self.held_views.remove(&control) {
            bpf::send_event(ControlEvent::Transaction(prev.apply_events()));
        }
    }

    /// Button release counterpart of `execute` (only hold actions care).
    fn execute_release(&mut self, control: ControlId, action: &Action) {
        match action {
            Action::HoldOverlay(scene) => {
                if bpf::with_dmx(|dmx| dmx.current_overlay_scenes.contains(scene)) {
                    bpf::send_event(ControlEvent::RemoveOverlayScene(*scene));
                }
            }
            Action::HoldSceneAlpha { .. } => self.restore_held_alpha(control),
            Action::HoldView(_) => self.restore_held_view(control),
            _ => {}
        }
    }

    /// Fires an action. `value` is the fader position (0..=127) for
    /// continuous actions; buttons pass `None`.
    pub fn execute(&mut self, control: ControlId, action: &Action, value: Option<u8>) {
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
            Action::HoldView(view_id) => match self.held_views.remove(&control) {
                // Second press without a release (twin click): restore.
                Some(prev) => bpf::send_event(ControlEvent::Transaction(prev.apply_events())),
                None => {
                    let dmx = bpf::get_dmx();
                    match dmx.views.get(view_id) {
                        Some(view) => {
                            let events = view.apply_events();
                            self.held_views.insert(control, Self::current_look(&dmx));
                            bpf::send_event(ControlEvent::Transaction(events));
                        }
                        None => self.push_mapping_log(format!("View #{view_id} no longer exists")),
                    }
                }
            },
            // Scene focus is only the edit target now, so a pad that used to
            // "focus" a scene makes it the whole look instead.
            Action::FocusScene(scene) => {
                bpf::send_event(ControlEvent::SetOverlays(vec![*scene]))
            }
            Action::ToggleOverlay(scene) => self.toggle_overlay(*scene),
            Action::HoldOverlay(scene) => {
                if !bpf::with_dmx(|dmx| dmx.current_overlay_scenes.contains(scene)) {
                    bpf::send_event(ControlEvent::AddOverlayScene(*scene));
                }
            }
            Action::HoldSceneAlpha { scenes, alpha } => match self.held_alphas.remove(&control) {
                // Second press without a release (twin click): restore.
                Some(prev) => send_scene_alphas(prev),
                None => {
                    let dmx = bpf::get_dmx();
                    let prev: Vec<(u8, u8)> = scenes
                        .iter()
                        .filter_map(|s| dmx.scenes.get(s).map(|sc| (*s, sc.sink.master_alpha_fader)))
                        .collect();
                    self.held_alphas.insert(control, prev);
                    send_scene_alphas(scenes.iter().map(|s| (*s, *alpha)).collect());
                }
            },
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
            Action::SceneMasterAlpha(scenes) => {
                let v = value.unwrap_or(127).min(127) as u16;
                let alpha = v.map_range(0..127, 0..100) as u8;
                send_events(
                    scenes
                        .iter()
                        .map(|s| ControlEvent::SetSceneMasterAlpha(*s, alpha))
                        .collect(),
                );
            }
            Action::SceneMasterSpeed(scenes) => {
                let v = value.unwrap_or(64).min(127) as u16;
                let max_index = AnimationSpeedModifier::ALL.len() as u16 - 1;
                let index = v.map_range(0..127, 0..max_index) as usize;
                send_events(
                    scenes
                        .iter()
                        .map(|s| {
                            ControlEvent::SetSceneMasterSpeed(
                                *s,
                                AnimationSpeedModifier::from_index(index),
                            )
                        })
                        .collect(),
                );
            }
        }
    }

    /// Whether the mapping's target is currently active (for bright LEDs).
    fn action_active(&self, m: &Mapping, dmx: &EngineState) -> bool {
        match &m.action {
            Action::TriggerView(id) => dmx
                .views
                .get(id)
                .is_some_and(|v| v.overlays == dmx.current_overlay_scenes),
            Action::HoldView(_) => self.held_views.contains_key(&m.control),
            Action::FocusScene(s) => dmx.current_overlay_scenes == [*s],
            Action::ToggleOverlay(s) | Action::HoldOverlay(s) => {
                dmx.current_overlay_scenes.contains(s)
            }
            Action::HoldSceneAlpha { .. } => self.held_alphas.contains_key(&m.control),
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

        let hold_phase_on = clock % HOLD_FLASH_PERIOD < HOLD_FLASH_ON;
        for m in &self.mappings {
            if let Some(note) = m.control.note() {
                let bright = if self.latched_holds.contains(&m.control) {
                    hold_phase_on
                } else {
                    self.action_active(m, dmx)
                };
                let color = if is_single_led(note) {
                    // The side buttons only light reliably at the placeholder
                    // velocity the learn blink uses, so they ignore `m.color`:
                    // white while active, the dim step while merely mapped.
                    if bright { LED_WHITE } else { SINGLE_LED_ON }
                } else if bright {
                    m.color
                } else {
                    dim(m.color)
                };
                desired.insert(note, color);
            }
        }

        match &self.mapping_editor {
            MappingEditor::Idle => {}
            MappingEditor::Learning {
                device: MappingDevice::Apc,
                editing,
            } => {
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
            MappingEditor::Learning { .. } => {}
            MappingEditor::Editing { draft, .. } => {
                if let Some(note) = draft.control.note() {
                    desired.insert(note, LED_WHITE);
                }
            }
        }

        // Shift is never mapped, but it is a modifier the operator has to find
        // in the dark: keep it lit at the placeholder velocity at all times.
        desired.insert(SHIFT_NOTE, LED_WHITE);

        desired
    }

    /// MIDI Mix button LEDs: single colour, note-on 127 = on, 0 = off.
    fn desired_mix_leds(&self, clock: u32, dmx: &EngineState) -> BTreeMap<u8, u8> {
        let mut desired = BTreeMap::new();
        let hold_phase_on = clock % HOLD_FLASH_PERIOD < HOLD_FLASH_ON;
        for m in &self.mappings {
            if let Some(note) = m.control.mix_note() {
                let on = if self.latched_holds.contains(&m.control) {
                    hold_phase_on
                } else {
                    self.action_active(m, dmx)
                };
                desired.insert(note, if on { 127 } else { 0 });
            }
        }
        match &self.mapping_editor {
            MappingEditor::Learning {
                device: MappingDevice::MidiMix,
                editing,
            } => {
                let phase_on = clock % LEARN_FLASH_PERIOD < LEARN_FLASH_ON;
                let relearn = editing.and_then(|i| self.mappings.get(i)?.control.mix_note());
                for note in ControlId::mix_led_notes() {
                    let mapped = self.mapping_index(ControlId::MixNote(note)).is_some();
                    if !mapped || Some(note) == relearn {
                        desired.insert(note, if phase_on { 127 } else { 0 });
                    }
                }
            }
            MappingEditor::Editing { draft, .. } => {
                if let Some(note) = draft.control.mix_note() {
                    desired.insert(note, 127);
                }
            }
            _ => {}
        }
        desired
    }

    /// Diff-syncs mapping LEDs to the MIDI Mix.
    pub fn sync_mix_leds(&mut self, conn: &VirtualMidi, clock: u32) {
        let desired = bpf::with_dmx(|dmx| self.desired_mix_leds(clock, dmx));
        for note in self.mix_lit_colors.keys() {
            if !desired.contains_key(note) {
                conn.send(0x90, *note, 0);
            }
        }
        for (note, v) in &desired {
            if self.mix_lit_colors.get(note) != Some(v) {
                conn.send(0x90, *note, *v);
            }
        }
        self.mix_lit_colors = desired;
    }

    /// Diff-syncs mapping LEDs to the device (and therefore the twin).
    pub fn sync_mapping_leds(&mut self, conn: &VirtualMidi, clock: u32) {
        // A latch only makes sense while its pad is still a hold mapping.
        let mappings = &self.mappings;
        self.latched_holds.retain(|control| {
            mappings
                .iter()
                .any(|m| {
                    m.control == *control
                        && matches!(m.action, Action::HoldOverlay(_) | Action::HoldView(_))
                })
        });

        let desired = bpf::with_dmx(|dmx| self.desired_leds(clock, dmx));

        for note in self.mapping_lit_colors.keys() {
            if !desired.contains_key(note) {
                conn.send(led_status(*note), *note, LED_OFF);
            }
        }
        for (note, color) in &desired {
            if self.mapping_lit_colors.get(note) != Some(color) {
                conn.send(led_status(*note), *note, *color);
            }
        }
        self.mapping_lit_colors = desired;
    }

    /// Whether the APC twin should show its learn highlight.
    pub fn is_learning(&self) -> bool {
        matches!(
            self.mapping_editor,
            MappingEditor::Learning {
                device: MappingDevice::Apc,
                ..
            }
        )
    }

    /// Status line for the twin while the editor is active.
    pub fn twin_status(&self) -> Option<(String, (u8, u8, u8))> {
        match &self.mapping_editor {
            MappingEditor::Idle if self.shift_held => Some((
                "SHIFT — press a hold pad to latch it".to_string(),
                (235, 235, 240),
            )),
            MappingEditor::Idle => None,
            MappingEditor::Learning {
                device: MappingDevice::Apc,
                ..
            } => Some((
                "LEARN — click a flashing control to map it".to_string(),
                (240, 170, 60),
            )),
            MappingEditor::Learning { device, .. } => Some((
                format!("LEARN — waiting for a control on the {}", device.name()),
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
        // Borrow the cached snapshot: this runs on every tick, so cloning the
        // whole engine state here was one of the most expensive things the
        // plugin did.
        bpf::with_dmx(|dmx| self.render_mappings_ui_inner(events, plugin_id, dmx))
    }

    fn render_mappings_ui_inner(
        &mut self,
        events: &[ControlEventMessage],
        plugin_id: u8,
        dmx: &EngineState,
    ) {

        match self.mapping_editor.clone() {
            MappingEditor::Idle => {
                ui::begin_horizontal();
                ui::label_styled("Mappings", 16, false);
                ui::button("+ APC mini", NEW_MAPPING_BUTTON_ID);
                ui::button("+ MIDI Mix", NEW_MIX_MAPPING_BUTTON_ID);
                ui::button("+ nanoKONTROL", NEW_KORG_MAPPING_BUTTON_ID);
                ui::end_horizontal();
                ui::label("Map any button, knob or fader of a controller to an action.");
            }
            MappingEditor::Learning { editing, device } => {
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
                    &if device.has_twin() {
                        format!("Press a control on the {}, or click it on the twin.", device.name())
                    } else {
                        format!("Press a control on the {}.", device.name())
                    },
                    14,
                    false,
                );
                if device == MappingDevice::Apc {
                    ui::label("Assignable controls are flashing. Fixed pads stay dark.");
                }
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

                let targets = draft.action.kind().targets(dmx);
                if targets.is_empty() {
                    ui::label("No targets available for this action.");
                } else if let Some(scenes) = draft.action.scenes() {
                    ui::label("Scenes:");
                    for (i, (code, name)) in targets.iter().enumerate() {
                        ui::checkbox(
                            name,
                            SCENE_CHECK_BASE_ID.saturating_add(i as u8),
                            scenes.contains(code),
                        );
                    }
                } else {
                    let names: Vec<String> = targets.iter().map(|(_, n)| n.clone()).collect();
                    let sel = targets
                        .iter()
                        .position(|(code, _)| *code == draft.action.target_code())
                        .unwrap_or(0);
                    ui::combo_box("Target", TARGET_COMBO_ID, &names, sel as u8);
                }

                if let Action::HoldSceneAlpha { alpha, .. } = &draft.action {
                    ui::slider("Alpha %", ALPHA_SLIDER_ID, 0, 100, *alpha);
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

        if matches!(self.mapping_editor, MappingEditor::Editing { .. }) {
            // The scene checkboxes reuse the row id range.
            ui::label("(mapping list hidden while editing)");
        } else if self.mappings.is_empty() {
            ui::label("No mappings yet.");
        } else {
            ui::begin_frame(LIST_FRAME_ID);
            for (idx, m) in self.mappings.iter().enumerate().take(MAX_MAPPINGS) {
                ui::begin_horizontal();
                ui::label_styled(&format!("{:<9}", m.control.label()), 13, true);
                ui::label(&format!("->  {}", m.action.describe(dmx)));
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

        self.process_mappings_ui_events(events, plugin_id, dmx);
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
                PluginUiEvent::Checkbox { id, checked } if id >= SCENE_CHECK_BASE_ID => {
                    self.handle_scene_checkbox((id - SCENE_CHECK_BASE_ID) as usize, checked, dmx)
                }
                PluginUiEvent::Slider { id, value } if id == ALPHA_SLIDER_ID => {
                    if let MappingEditor::Editing { index, mut draft } = self.mapping_editor.clone()
                    {
                        if let Action::HoldSceneAlpha { alpha, .. } = &mut draft.action {
                            *alpha = value.min(100);
                        }
                        self.mapping_editor = MappingEditor::Editing { index, draft };
                    }
                }
                _ => {}
            }
        }
    }

    fn start_learning(&mut self, device: MappingDevice) {
        if self.mappings.len() >= MAX_MAPPINGS {
            self.push_mapping_log(format!("Mapping limit ({MAX_MAPPINGS}) reached"));
        } else {
            self.mapping_editor = MappingEditor::Learning {
                editing: None,
                device,
            };
            self.push_mapping_log(format!("Waiting for a control on the {}…", device.name()));
        }
    }

    /// Toggles the `i`-th scene (in `dmx.scenes` order) in the draft's list.
    fn handle_scene_checkbox(&mut self, i: usize, checked: bool, dmx: &EngineState) {
        let MappingEditor::Editing { index, mut draft } = self.mapping_editor.clone() else {
            return;
        };
        let Some(scene) = dmx.scenes.keys().nth(i).copied() else {
            return;
        };
        if let Some(scenes) = draft.action.scenes_mut() {
            scenes.retain(|s| *s != scene);
            if checked {
                scenes.push(scene);
                scenes.sort_unstable();
            }
        }
        self.mapping_editor = MappingEditor::Editing { index, draft };
    }

    fn handle_mapping_button(&mut self, id: u8) {
        match id {
            NEW_MAPPING_BUTTON_ID => self.start_learning(MappingDevice::Apc),
            NEW_MIX_MAPPING_BUTTON_ID => self.start_learning(MappingDevice::MidiMix),
            NEW_KORG_MAPPING_BUTTON_ID => self.start_learning(MappingDevice::Korg),
            CANCEL_BUTTON_ID => {
                self.mapping_editor = MappingEditor::Idle;
            }
            RELEARN_BUTTON_ID => {
                if let MappingEditor::Editing { index, draft } = &self.mapping_editor {
                    self.mapping_editor = MappingEditor::Learning {
                        editing: *index,
                        device: draft.control.device(),
                    };
                }
            }
            SAVE_BUTTON_ID => {
                if let MappingEditor::Editing { index, draft } = self.mapping_editor.clone() {
                    let label = format!(
                        "{} -> {}",
                        draft.control.label(),
                        bpf::with_dmx(|dmx| draft.action.describe(dmx))
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
            id if id >= ROW_BASE_ID && !matches!(self.mapping_editor, MappingEditor::Editing { .. }) => {
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
