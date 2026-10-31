//! View state owned by the native (desk) frontend: which plugin UIs are open
//! and where, pop-out and maximize state, and plugin tab selection. The engine
//! only sees the part it needs through the
//! [`FrontendUiMirror`](blaulicht_engine::frontend_ui::FrontendUiMirror).

use std::collections::{BTreeSet, HashMap, HashSet};

use blaulicht_engine::frontend_ui::DeskRequest;
use blaulicht_engine::state::ScreenId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PluginOpenState {
    pub screen_id: ScreenId,
    pub open: bool,
}

impl PluginOpenState {
    pub const CLOSED: Self = Self {
        screen_id: ScreenId::MAIN,
        open: false,
    };
}

/// Selected tab per `(plugin_id, tabs_id)`.
///
/// The selection is also reported to the plugin with
/// `PluginUiEvent::TabChanged`. With several frontends attached, each keeps
/// its own selection and the plugin's copy follows the last frontend that
/// changed it (last write wins).
pub(crate) type PluginTabSelections = HashMap<(u8, u8), u8>;

#[derive(Default)]
pub(crate) struct NativeViewState {
    /// Per plugin: whether its UI is open, and on which screen.
    pub plugin_visibility: HashMap<u8, PluginOpenState>,
    /// Plugins whose UI window is popped out into its own viewport.
    pub plugin_popped_out: HashSet<u8>,
    /// One-shot viewport maximize requests, consumed when the popped-out
    /// viewport is next shown.
    pub plugin_maximize_requested: HashSet<u8>,
    pub plugin_tabs_selected: PluginTabSelections,
}

impl NativeViewState {
    pub fn plugin_open_state(&self, plugin_id: u8) -> PluginOpenState {
        self.plugin_visibility
            .get(&plugin_id)
            .copied()
            .unwrap_or(PluginOpenState::CLOSED)
    }

    pub fn is_popped_out(&self, plugin_id: u8) -> bool {
        self.plugin_popped_out.contains(&plugin_id)
    }

    pub fn set_popped_out(&mut self, plugin_id: u8, popped_out: bool) {
        if popped_out {
            self.plugin_popped_out.insert(plugin_id);
        } else {
            self.plugin_popped_out.remove(&plugin_id);
        }
    }

    /// Open or close a plugin's UI, keeping the screen it was on. Works for a
    /// plugin that has no entry yet.
    pub fn set_plugin_open(&mut self, plugin_id: u8, open: bool) {
        self.plugin_visibility
            .entry(plugin_id)
            .or_insert(PluginOpenState::CLOSED)
            .open = open;
    }

    /// Open a plugin's UI popped out, and maximize its viewport the next time
    /// it is shown.
    pub fn request_maximize(&mut self, plugin_id: u8) {
        self.set_plugin_open(plugin_id, true);
        self.plugin_popped_out.insert(plugin_id);
        self.plugin_maximize_requested.insert(plugin_id);
    }

    /// Consume a pending maximize request.
    pub fn take_maximize_request(&mut self, plugin_id: u8) -> bool {
        self.plugin_maximize_requested.remove(&plugin_id)
    }

    pub fn apply_desk_request(&mut self, request: DeskRequest) {
        match request {
            DeskRequest::SetPluginUiOpen { plugin_id, open } => {
                tracing::debug!("Set plugin {plugin_id} UI visibility to {open}");
                self.set_plugin_open(plugin_id, open);
            }
            DeskRequest::MaximizePluginUi { plugin_id } => self.request_maximize(plugin_id),
        }
    }

    /// Plugins whose UI is open on any screen, for the engine's mirror.
    pub fn open_plugins(&self) -> BTreeSet<u8> {
        self.plugin_visibility
            .iter()
            .filter(|(_, state)| state.open)
            .map(|(plugin_id, _)| *plugin_id)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_open_request_opens_a_plugin_without_an_entry() {
        let mut view = NativeViewState::default();
        view.apply_desk_request(DeskRequest::SetPluginUiOpen {
            plugin_id: 9,
            open: true,
        });
        assert!(view.plugin_open_state(9).open);
        assert_eq!(view.open_plugins(), BTreeSet::from([9]));

        view.apply_desk_request(DeskRequest::SetPluginUiOpen {
            plugin_id: 9,
            open: false,
        });
        assert!(!view.plugin_open_state(9).open);
        assert!(view.open_plugins().is_empty());
    }

    #[test]
    fn set_open_keeps_the_screen() {
        let mut view = NativeViewState::default();
        view.plugin_visibility.insert(
            1,
            PluginOpenState {
                screen_id: ScreenId::external(0),
                open: false,
            },
        );
        view.set_plugin_open(1, true);
        assert_eq!(
            view.plugin_open_state(1),
            PluginOpenState {
                screen_id: ScreenId::external(0),
                open: true,
            }
        );
    }

    #[test]
    fn maximize_request_opens_popped_out_once() {
        let mut view = NativeViewState::default();
        view.apply_desk_request(DeskRequest::MaximizePluginUi { plugin_id: 3 });
        assert!(view.plugin_open_state(3).open);
        assert!(view.is_popped_out(3));
        assert!(view.take_maximize_request(3));
        assert!(!view.take_maximize_request(3));
        assert!(view.is_popped_out(3));
    }
}
