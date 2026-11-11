//! Whether the plugin talks to real hardware or only pretends to.
//!
//! Covers everything outside the engine this plugin drives: the MIDI devices
//! and the `sudo fans` command. It describes the machine rather than the show,
//! so it lives in global plugin state (`config.toml`), not the showfile.

use blaulicht_plugin_framework as bpf;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HardwareMode {
    #[default]
    Real,
    Mock,
}

#[derive(Default, Serialize, Deserialize)]
struct PersistedConfig {
    #[serde(default)]
    hardware: HardwareMode,
}

pub fn load_mode() -> HardwareMode {
    bpf::load_plugin_state(bpf::PluginStateLocation::Global)
        .and_then(|json| serde_json::from_str::<PersistedConfig>(&json).ok())
        .unwrap_or_default()
        .hardware
}

pub fn save_mode(hardware: HardwareMode) {
    match serde_json::to_string(&PersistedConfig { hardware }) {
        Ok(json) => bpf::save_plugin_state(bpf::PluginStateLocation::Global, &json),
        Err(e) => bpf::bl_log(
            &format!("Failed to serialize hardware config: {e}"),
            blaulicht_shared::LogLevel::Err,
        ),
    }
}
