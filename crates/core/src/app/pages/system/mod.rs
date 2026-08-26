use crate::{app::ui::FileDialogOpenOrigin, state::NUM_DMX_UNIVERSES};
use egui_file_dialog::FileDialog;
use std::path::PathBuf;

pub mod artnet;
pub mod dmx;
pub mod health;
pub mod midi;
pub mod misc;
pub mod screens;
pub mod serial;
pub mod showfile;
pub mod speed;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemTab {
    General,
    Dmx,
    ArtNet,
    Plugins,
    Midi,
    Serial,
    Screens,
}

impl SystemTab {
    pub const ALL: [Self; 7] = [
        Self::General,
        Self::Dmx,
        Self::ArtNet,
        Self::Plugins,
        Self::Midi,
        Self::Serial,
        Self::Screens,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Dmx => "DMX",
            Self::ArtNet => "Art-Net",
            Self::Plugins => "Plugins",
            Self::Midi => "MIDI",
            Self::Serial => "Serial",
            Self::Screens => "Screens",
        }
    }
}

pub struct SystemUI {
    pub active_tab: SystemTab,
    open_file_dialog: Option<FileDialog>,
    pending_load_file: Option<PathBuf>,
    close_showfile_confirm_open: bool,
    file_dialog_open_origin: FileDialogOpenOrigin,
    reload_dialog_open: bool,
    confirm_shutdown_open: bool,
    pub debug_open: bool,
    artnet_dialog_open: bool,
    midi_dialog_open: bool,
    serial_dialog_open: bool,
    screens_dialog_open: bool,
    dmx_dialogs_open: [bool; NUM_DMX_UNIVERSES],
    new_artnet_address: String,
    new_artnet_port: String,
    artnet_input_error: Option<String>,
    plugin_dialog_open: bool,
}

impl Default for SystemUI {
    fn default() -> Self {
        Self {
            active_tab: SystemTab::General,
            open_file_dialog: None,
            pending_load_file: None,
            close_showfile_confirm_open: false,
            file_dialog_open_origin: FileDialogOpenOrigin::Load,
            reload_dialog_open: false,
            confirm_shutdown_open: false,
            debug_open: false,
            artnet_dialog_open: false,
            midi_dialog_open: false,
            serial_dialog_open: false,
            screens_dialog_open: false,
            dmx_dialogs_open: [false; NUM_DMX_UNIVERSES],
            new_artnet_address: String::new(),
            new_artnet_port: "6454".to_string(),
            artnet_input_error: None,
            plugin_dialog_open: false,
        }
    }
}
