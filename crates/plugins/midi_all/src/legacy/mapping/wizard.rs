//! Mapping Wizard: assigns a whole scene folder to one APC mini grid column.
//!
//! Every scene in the folder (direct children, sorted by name) becomes a
//! "Hold overlay scene" mapping on one pad of the column, top pad first, so
//! a pad holds its overlay while pressed and shift+press latches it. Only the
//! pads that get a scene are touched; other pads of the column keep their
//! mappings. The outer columns are reserved (scene / page pads), so columns
//! 2..=7 are offered.

use blaulicht_plugin_framework::{self as bpf, ui};
use blaulicht_shared::{
    folder::{sorted_folder_ids, FolderId, FolderKind},
    ControlEvent, ControlEventMessage, EngineState, PluginUiEvent,
};

use super::{Action, ActionKind, ControlId, Mapping, COLORS, MAX_MAPPINGS};
use crate::legacy::LegacyState;

// Widget ids 80..=89 (see the id table in mod.rs).
const COLUMN_COMBO_ID: u8 = 80;
const FOLDER_COMBO_ID: u8 = 81;
const COLOR_COMBO_ID: u8 = 82;
const APPLY_BUTTON_ID: u8 = 83;
const FRAME_ID: u8 = 84;

/// Grid columns (0-based) the wizard may fill; 0 and 7 are reserved.
const COLUMNS: std::ops::RangeInclusive<u8> = 1..=6;
const ROWS: usize = 8;

#[derive(Debug, Clone)]
pub struct WizardState {
    column: u8,
    folder: Option<FolderId>,
    color: u8,
}

impl Default for WizardState {
    fn default() -> Self {
        Self {
            column: *COLUMNS.start(),
            folder: None,
            color: ActionKind::HoldOverlay.default_color(),
        }
    }
}

/// Pad note of `row` (0 = top) in grid `column`. Note 0 is bottom-left.
fn pad(column: u8, row: usize) -> u8 {
    (ROWS - 1 - row) as u8 * 8 + column
}

/// Folder name with its parents, e.g. "Club / Strobes".
fn folder_path(dmx: &EngineState, id: FolderId) -> String {
    let mut parts = Vec::new();
    let mut cur = Some(id);
    while let Some(c) = cur {
        let Some(f) = dmx.folders.get(&c) else { break };
        parts.push(f.name.clone());
        cur = f.parent;
        if parts.len() > 16 {
            break; // broken parent chain
        }
    }
    parts.reverse();
    parts.join(" / ")
}

/// Scenes sitting directly in `folder`, sorted by name.
fn folder_scenes(dmx: &EngineState, folder: FolderId) -> Vec<(u8, String)> {
    let mut scenes: Vec<(u8, String)> = dmx
        .user_scenes()
        .filter(|(_, s)| s.folder == Some(folder))
        .map(|(id, s)| (*id, s.name.clone()))
        .collect();
    scenes.sort_by_cached_key(|(id, name)| (name.to_lowercase(), *id));
    scenes
}

impl LegacyState {
    pub fn render_mapping_wizard_ui(&mut self, events: &[ControlEventMessage], plugin_id: u8) {
        bpf::with_dmx(|dmx| self.render_mapping_wizard_inner(events, plugin_id, dmx))
    }

    fn render_mapping_wizard_inner(
        &mut self,
        events: &[ControlEventMessage],
        plugin_id: u8,
        dmx: &EngineState,
    ) {
        let folders = sorted_folder_ids(&dmx.folders, FolderKind::Scenes);
        if self.wizard.folder.map_or(true, |f| !folders.contains(&f)) {
            self.wizard.folder = folders.first().copied();
        }

        ui::label_styled("Mapping Wizard", 16, false);
        ui::label(
            "Puts a scene folder on one APC mini column: each pad holds its scene as an \
             overlay while pressed, shift+press latches it.",
        );

        let columns: Vec<String> = COLUMNS.map(|c| format!("Column {}", c + 1)).collect();
        ui::combo_box(
            "Column",
            COLUMN_COMBO_ID,
            &columns,
            self.wizard.column - COLUMNS.start(),
        );

        if folders.is_empty() {
            ui::label("No scene folders yet. Create one on the Scenes page.");
        } else {
            let names: Vec<String> = folders.iter().map(|f| folder_path(dmx, *f)).collect();
            let sel = self
                .wizard
                .folder
                .and_then(|f| folders.iter().position(|x| *x == f))
                .unwrap_or(0);
            ui::combo_box("Scene folder", FOLDER_COMBO_ID, &names, sel as u8);
        }

        let colors: Vec<String> = COLORS.iter().map(|(n, _)| n.to_string()).collect();
        let csel = COLORS
            .iter()
            .position(|(_, c)| *c == self.wizard.color)
            .unwrap_or(0);
        ui::combo_box("LED colour", COLOR_COMBO_ID, &colors, csel as u8);

        let plan = self.wizard_plan(dmx);
        ui::begin_frame(FRAME_ID);
        match &plan {
            None => ui::label("Pick a scene folder."),
            Some((scenes, _)) if scenes.is_empty() => {
                ui::label("This folder has no scenes.");
            }
            Some((scenes, skipped)) => {
                for (row, (scene, name)) in scenes.iter().enumerate() {
                    let control = ControlId::ApcNote(pad(self.wizard.column, row));
                    let replaces = self
                        .mapping_index(control)
                        .map(|i| &self.mappings[i])
                        .filter(|m| m.action != Action::HoldOverlay(*scene))
                        .map(|m| format!("   (replaces: {})", m.action.describe(dmx)))
                        .unwrap_or_default();
                    ui::label(&format!("Row {}  ->  {name}{replaces}", row + 1));
                }
                if *skipped > 0 {
                    ui::label(&format!(
                        "{skipped} more scene(s) don't fit into the 8 pads and are skipped."
                    ));
                }
            }
        }
        ui::end_frame();

        let can_apply = matches!(&plan, Some((s, _)) if !s.is_empty());
        ui::button_styled("Apply to column", APPLY_BUTTON_ID, can_apply);

        if !self.mapping_log.is_empty() {
            ui::separator();
            for line in &self.mapping_log {
                ui::label(line);
            }
        }

        for event in events {
            let ControlEvent::PluginUi(ui_event, pid) = event.body() else {
                continue;
            };
            if pid != plugin_id {
                continue;
            }
            match ui_event {
                PluginUiEvent::ComboBox { id: COLUMN_COMBO_ID, selected } => {
                    self.wizard.column =
                        (COLUMNS.start() + selected).min(*COLUMNS.end());
                }
                PluginUiEvent::ComboBox { id: FOLDER_COMBO_ID, selected } => {
                    if let Some(f) = folders.get(selected as usize) {
                        self.wizard.folder = Some(*f);
                    }
                }
                PluginUiEvent::ComboBox { id: COLOR_COMBO_ID, selected } => {
                    if let Some((_, c)) = COLORS.get(selected as usize) {
                        self.wizard.color = *c;
                    }
                }
                PluginUiEvent::Button { id: APPLY_BUTTON_ID } => self.apply_wizard(dmx),
                _ => {}
            }
        }
    }

    /// The scenes that go onto the column (top first) and how many don't fit.
    fn wizard_plan(&self, dmx: &EngineState) -> Option<(Vec<(u8, String)>, usize)> {
        let mut scenes = folder_scenes(dmx, self.wizard.folder?);
        let skipped = scenes.len().saturating_sub(ROWS);
        scenes.truncate(ROWS);
        Some((scenes, skipped))
    }

    fn apply_wizard(&mut self, dmx: &EngineState) {
        let Some((scenes, _)) = self.wizard_plan(dmx) else {
            return;
        };
        let column = self.wizard.column;
        let new = scenes
            .iter()
            .enumerate()
            .filter(|(row, _)| self.mapping_index(ControlId::ApcNote(pad(column, *row))).is_none())
            .count();
        if self.mappings.len() + new > MAX_MAPPINGS {
            self.push_mapping_log(format!(
                "Not applied: would exceed the mapping limit ({MAX_MAPPINGS})"
            ));
            return;
        }

        for (row, (scene, _)) in scenes.iter().enumerate() {
            let mapping = Mapping {
                control: ControlId::ApcNote(pad(column, row)),
                action: Action::HoldOverlay(*scene),
                color: self.wizard.color,
            };
            match self.mapping_index(mapping.control) {
                Some(i) => self.mappings[i] = mapping,
                None => self.mappings.push(mapping),
            }
        }
        self.sort_mappings();
        self.save_mappings();
        let folder = self
            .wizard
            .folder
            .map(|f| folder_path(dmx, f))
            .unwrap_or_default();
        self.push_mapping_log(format!(
            "Mapped {} scene(s) of \"{folder}\" to column {}",
            scenes.len(),
            column + 1
        ));
    }
}
