use crate::app::components::Numberpad;
use blaulicht_engine::state::NUM_DMX_UNIVERSES;
use std::{collections::HashMap, net::SocketAddr, path::PathBuf};

pub mod artnet;
pub mod desk;
pub mod dmx;
#[cfg(feature = "file-dialog")]
pub mod file_dialog;
pub mod general;
pub mod health;
pub mod midi;
pub mod misc;
pub mod plugins;
pub mod remote;
pub mod screens;
pub mod serial;
pub mod showfile;
pub mod speed;

pub struct SystemUI {
    #[cfg(feature = "file-dialog")]
    open_file_dialog: Option<egui_file_dialog::FileDialog>,
    pending_load_file: Option<PathBuf>,
    close_showfile_confirm_open: bool,
    #[cfg(feature = "file-dialog")]
    file_dialog_open_origin: file_dialog::FileDialogOpenOrigin,
    reload_dialog_open: bool,
    confirm_shutdown_open: bool,
    pub debug_open: bool,
    midi_dialog_open: bool,
    serial_dialog_open: bool,
    screens_dialog_open: bool,
    remote_passcode_input: String,
    remote_passcode_shown: bool,
    dmx_dialogs_open: [bool; NUM_DMX_UNIVERSES],
    new_artnet_address: String,
    new_artnet_port: String,
    artnet_input_error: Option<String>,
    /// `[first, last]` universe numberpads per user-created receiver row.
    artnet_universe_pads: HashMap<SocketAddr, [Numberpad; 2]>,
    /// When the UI started, for the desktop About pane's uptime.
    started: std::time::Instant,
    /// The desktop Screens list's selected screen.
    selected_screen: usize,
    /// The desktop Plugins list's selected plugin.
    selected_plugin: Option<u8>,
    /// The port the remote server last listened on, for the Web UI
    /// address while it is off.
    remote_port: Option<u16>,
}

impl Default for SystemUI {
    fn default() -> Self {
        Self {
            #[cfg(feature = "file-dialog")]
            open_file_dialog: None,
            pending_load_file: None,
            close_showfile_confirm_open: false,
            #[cfg(feature = "file-dialog")]
            file_dialog_open_origin: file_dialog::FileDialogOpenOrigin::Load,
            reload_dialog_open: false,
            confirm_shutdown_open: false,
            debug_open: false,
            midi_dialog_open: false,
            serial_dialog_open: false,
            screens_dialog_open: false,
            remote_passcode_input: String::new(),
            remote_passcode_shown: false,
            dmx_dialogs_open: [false; NUM_DMX_UNIVERSES],
            new_artnet_address: String::new(),
            new_artnet_port: "6454".to_string(),
            artnet_input_error: None,
            artnet_universe_pads: HashMap::new(),
            started: std::time::Instant::now(),
            selected_screen: 0,
            selected_plugin: None,
            remote_port: None,
        }
    }
}
