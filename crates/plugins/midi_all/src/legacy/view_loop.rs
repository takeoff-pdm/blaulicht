//! View loop ("auto mode"): a minimal stand-in for the scene graph. Cycles
//! through a list of views, triggering the next one every `seconds`, exactly
//! like a "Trigger view" mapping would. A mapped button on any of the mapping
//! devices (APC mini, MIDI Mix, nanoKONTROL; the "Toggle view loop"
//! action) turns it on and off.
//!
//! The list and the duration are saved in the showfile next to the mappings.
//! Auto mode itself always starts off.

use blaulicht_plugin_framework::{self as bpf, ui};
use blaulicht_shared::{ControlEvent, ControlEventMessage, EngineState, PluginUiEvent};
use serde::{Deserialize, Serialize};

// Widget ids: free of the ids in mod.rs / mapping.rs. Remove buttons sit above
// the mapping rows (100 + 2 * MAX_MAPPINGS), whose handler ignores them.
const RUNNING_SWITCH_ID: u8 = 70;
const SECONDS_SLIDER_ID: u8 = 71;
const ADD_COMBO_ID: u8 = 72;
const ADD_BUTTON_ID: u8 = 73;
const NEXT_BUTTON_ID: u8 = 74;
const REMOVE_BASE_ID: u8 = 230;
const MAX_STEPS: usize = (u8::MAX - REMOVE_BASE_ID) as usize + 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ViewLoopConfig {
    /// View ids, shown in this order and then from the start again.
    pub views: Vec<u8>,
    pub seconds: u8,
}

impl Default for ViewLoopConfig {
    fn default() -> Self {
        Self {
            views: Vec::new(),
            seconds: 10,
        }
    }
}

#[derive(Default)]
pub struct ViewLoop {
    pub config: ViewLoopConfig,
    pub running: bool,
    /// Index into `config.views` of the view shown last.
    index: usize,
    /// Clock (ms) at which that view was triggered; `None` = trigger now.
    started_at: Option<u32>,
    add_choice: u8,
}

impl ViewLoop {
    pub fn toggle(&mut self) {
        self.set_running(!self.running);
    }

    fn set_running(&mut self, on: bool) {
        self.running = on;
        self.started_at = None;
    }

    /// Triggers the next view once the current one has had its time.
    pub fn tick(&mut self, clock: u32) {
        let len = self.config.views.len();
        if !self.running || len == 0 {
            self.started_at = None;
            return;
        }
        let next = match self.started_at {
            None => self.index.min(len - 1),
            Some(t) if clock.wrapping_sub(t) >= self.config.seconds.max(1) as u32 * 1000 => {
                (self.index + 1) % len
            }
            Some(_) => return,
        };
        self.show(next, clock);
    }

    /// Triggers the view at `index`, skipping views that no longer exist.
    fn show(&mut self, index: usize, clock: u32) {
        let len = self.config.views.len();
        let found = bpf::with_dmx(|dmx| {
            (0..len).map(|k| (index + k) % len).find_map(|i| {
                let view = dmx.views.get(&self.config.views[i])?;
                Some((i, view.apply_events()))
            })
        });
        if let Some((i, events)) = found {
            bpf::send_event(ControlEvent::Transaction(events));
            self.index = i;
        }
        // Also when nothing was found, so a loop of deleted views retries once
        // per interval instead of every tick.
        self.started_at = Some(clock);
    }

    /// Draws the tab and applies its UI events. Returns true when the saved
    /// config changed.
    pub fn render(
        &mut self,
        events: &[ControlEventMessage],
        plugin_id: u8,
        clock: u32,
        dmx: &EngineState,
    ) -> bool {
        let views: Vec<(u8, String)> = dmx
            .views
            .iter()
            .map(|(id, v)| (*id, format!("{} (#{id})", v.name)))
            .collect();
        self.add_choice = self.add_choice.min(views.len().saturating_sub(1) as u8);
        let name_of = |id: u8| {
            views
                .iter()
                .find(|(vid, _)| *vid == id)
                .map(|(_, name)| name.clone())
                .unwrap_or_else(|| format!("#{id} (missing)"))
        };
        let showing = self.running && self.started_at.is_some();

        ui::label("Triggers the views below one after another. Map a button to");
        ui::label("\"Toggle view loop\" in the Mappings tab to switch it from the APC mini,");
        ui::label("the MIDI Mix or the nanoKONTROL.");
        ui::begin_horizontal();
        ui::switch("Auto mode", RUNNING_SWITCH_ID, self.running);
        ui::button_styled("Next", NEXT_BUTTON_ID, showing);
        ui::end_horizontal();
        ui::slider("Seconds per view", SECONDS_SLIDER_ID, 1, 120, self.config.seconds);

        if let (true, Some(started)) = (showing, self.started_at) {
            let total = self.config.seconds.max(1) as u32 * 1000;
            let left = total.saturating_sub(clock.wrapping_sub(started));
            ui::label(&format!(
                "Showing {}, next in {:.1} s",
                name_of(self.config.views[self.index]),
                left as f32 / 1000.0
            ));
        }

        ui::separator();
        if self.config.views.is_empty() {
            ui::label("No views in the loop yet.");
        }
        for (i, id) in self.config.views.iter().enumerate() {
            let marker = if showing && i == self.index { "▶" } else { "  " };
            ui::begin_horizontal();
            ui::button("✕", REMOVE_BASE_ID + i as u8);
            ui::label(&format!("{marker} {}. {}", i + 1, name_of(*id)));
            ui::end_horizontal();
        }

        ui::separator();
        let names: Vec<String> = views.iter().map(|(_, n)| n.clone()).collect();
        ui::begin_horizontal();
        ui::combo_box("View", ADD_COMBO_ID, &names, self.add_choice);
        ui::button_styled(
            "Add",
            ADD_BUTTON_ID,
            !names.is_empty() && self.config.views.len() < MAX_STEPS,
        );
        ui::end_horizontal();

        let mut changed = false;
        for event in events {
            let ControlEvent::PluginUi(ui_event, pid) = event.body() else {
                continue;
            };
            if pid != plugin_id {
                continue;
            }
            match ui_event {
                PluginUiEvent::Switch { id: RUNNING_SWITCH_ID, value } => self.set_running(value),
                PluginUiEvent::Slider { id: SECONDS_SLIDER_ID, value } => {
                    self.config.seconds = value.max(1);
                    changed = true;
                }
                PluginUiEvent::ComboBox { id: ADD_COMBO_ID, selected } => {
                    self.add_choice = selected;
                }
                PluginUiEvent::Button { id: ADD_BUTTON_ID } => {
                    if let Some((id, _)) = views.get(self.add_choice as usize) {
                        if self.config.views.len() < MAX_STEPS {
                            self.config.views.push(*id);
                            changed = true;
                        }
                    }
                }
                PluginUiEvent::Button { id: NEXT_BUTTON_ID } => {
                    if self.running && !self.config.views.is_empty() {
                        self.show((self.index + 1) % self.config.views.len(), clock);
                    }
                }
                PluginUiEvent::Button { id } if id >= REMOVE_BASE_ID => {
                    let i = (id - REMOVE_BASE_ID) as usize;
                    if i < self.config.views.len() {
                        self.config.views.remove(i);
                        if i < self.index {
                            self.index -= 1;
                        }
                        self.index = self.index.min(self.config.views.len().saturating_sub(1));
                        changed = true;
                    }
                }
                _ => {}
            }
        }
        changed
    }
}
