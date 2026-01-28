use crate::{app::ui::FileDialogOpenOrigin, state::NUM_DMX_UNIVERSES};
use egui_file::FileDialog;

pub mod artnet;
pub mod dmx;
pub mod health;
pub mod midi;
pub mod misc;
pub mod serial;
pub mod showfile;
pub mod speed;

pub struct SystemUI {
    open_file_dialog: Option<FileDialog>,
    file_dialog_open_origin: FileDialogOpenOrigin,
    reload_dialog_open: bool,
    confirm_shutdown_open: bool,
    pub debug_open: bool,
    artnet_dialog_open: bool,
    midi_dialog_open: bool,
    serial_dialog_open: bool,
    dmx_dialogs_open: [bool; NUM_DMX_UNIVERSES],
    new_artnet_address: String,
    new_artnet_port: String,
    artnet_input_error: Option<String>,
    plugin_dialog_open: bool,
}

impl Default for SystemUI {
    fn default() -> Self {
        Self {
            open_file_dialog: None,
            file_dialog_open_origin: FileDialogOpenOrigin::Load,
            reload_dialog_open: false,
            confirm_shutdown_open: false,
            debug_open: false,
            artnet_dialog_open: false,
            midi_dialog_open: false,
            serial_dialog_open: false,
            dmx_dialogs_open: [false; NUM_DMX_UNIVERSES],
            new_artnet_address: String::new(),
            new_artnet_port: String::new(),
            artnet_input_error: None,
            plugin_dialog_open: false,
        }
    }
}
