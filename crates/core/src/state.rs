use std::{
    borrow::Cow,
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex, RwLock},
};

use blaulicht_shared::{CollectedAudioSnapshot};
use crossbeam_channel::{Receiver, Sender};
use serde::{Deserialize, Serialize};

use crate::{
    audio::defs::AudioThreadControlSignal, config::{Config, PluginConfig}, dmx::EngineState, event::{SystemEventBusConnection, SystemEventBusConnectionInst}, msg::{FromFrontend, Signal, SystemMessage, UnifiedMessage}, plugin::Plugin
};
use crate::ui_ops::WasmUiOp;

#[derive(Clone)]
pub struct AppStateWrapper {
    pub from_frontend_sender: crossbeam_channel::Sender<FromFrontend>,

    pub system_message_receiver: Receiver<SystemMessage>,
    pub system_message_sender: Sender<SystemMessage>,

    pub config: Arc<Mutex<Config>>,
    pub config_path: String,
    pub event_bus_connection: SystemEventBusConnectionInst,

    pub state: Arc<AppState>,
}

const APP_LOG_LENGTH: usize = 1000;

#[derive(Serialize, Deserialize)]
pub struct AudioState {
    pub device_name: Option<String>,
}

impl AudioState {
    pub fn default() -> Self {
        Self { device_name: None }
    }
}

/// Rolling buffer of recent spectra for a live spectrogram.
pub struct AudioSpectrogram {
    /// Most-recent-last columns; each column is `bin_count` tall with u8 intensities 0..=255.
    /// Contains bins. A bin is just a averaged part of the frequency space.
    pub columns: VecDeque<Vec<u8>>,
    /// Maximum number of time columns to keep.
    pub max_columns: usize,
    /// Number of frequency bins per column.
    pub bin_count: usize,
}

impl AudioSpectrogram {
    pub fn new(max_columns: usize, bin_count: usize) -> Self {
        Self {
            columns: VecDeque::with_capacity(max_columns),
            max_columns,
            bin_count,
        }
    }

    pub fn push_column(&mut self, mut col: Vec<u8>) {
        // Ensure correct height; pad or truncate as needed.
        if col.len() != self.bin_count {
            col.resize(self.bin_count, 0);
        }
        self.columns.push_back(col);
        if self.columns.len() > self.max_columns {
            self.columns.pop_front();
        }
    }
}

pub struct AppState {
    pub logs: Mutex<VecDeque<Cow<'static, str>>>,
    pub plugins: RwLock<HashMap<u8, PluginState>>,
    pub dmx_engine: RwLock<EngineState>,
    pub audio: RwLock<AudioState>,
    pub dmx_universes: [RwLock<DmxBuffer>; 2],
    pub audio_snapshot: RwLock<CollectedAudioSnapshot>,
    pub audio_spectrogram: RwLock<AudioSpectrogram>,
    pub mainloop_state: RwLock<AudioThreadControlSignal>,
    pub plugin_ui_ops: RwLock<HashMap<u8, Vec<WasmUiOp>>>,
    // Back buffer for plugin UI ops. Plugins write here; UI reads from `plugin_ui_ops`.
    pub plugin_ui_ops_back: RwLock<HashMap<u8, Vec<WasmUiOp>>>,
    pub plugin_ui_visibility: RwLock<HashMap<u8, bool>>, // per-plugin UI window visibility
    pub plugin_ui_popped_out: RwLock<HashMap<u8, bool>>, // per-plugin UI window pop-out state
    pub plugin_ui_tabs_selected: RwLock<HashMap<(u8, u8), u8>>, // (plugin_id, tabs_id) -> tab_id
    pub plugin_state_storage: Arc<Mutex<HashMap<String, String>>>,
}

pub struct DmxBuffer {
    pub dmx_buffer: [u8; 513],
}

impl DmxBuffer {
    pub fn new() -> Self {
        Self {
            dmx_buffer: [0; 513],
        }
    }
}

impl AppState {
    pub fn new(plugins: &[PluginConfig]) -> Self {
        let plugins_map = plugins
            .iter()
            .enumerate()
            .map(|(i, v)| {
                (
                    i as u8,
                    PluginState::new(v.file_path.clone().into(), v.enabled),
                )
            })
            .collect();

        let mut plugin_ui_visibility = HashMap::new();
        let mut plugin_ui_popped_out = HashMap::new();
        for (i, _) in plugins.iter().enumerate() {
            plugin_ui_visibility.insert(i as u8, false);
            plugin_ui_popped_out.insert(i as u8, false);
        }

        Self {
            logs: Mutex::new(VecDeque::with_capacity(APP_LOG_LENGTH)),
            plugins: RwLock::new(plugins_map),
            dmx_engine: RwLock::new(EngineState::default()),
            dmx_universes: [RwLock::new(DmxBuffer::new()), RwLock::new(DmxBuffer::new())],
            audio: RwLock::new(AudioState::default()),
            audio_snapshot: RwLock::new(CollectedAudioSnapshot::default()),
            // Default: ~6.6 seconds history at 60 FPS if filled every frame; actual fill rate ~20 Hz.
            audio_spectrogram: RwLock::new(AudioSpectrogram::new(400, 128)),
            mainloop_state: RwLock::new(AudioThreadControlSignal::ABORTED),
            plugin_ui_ops: RwLock::new(HashMap::new()),
            plugin_ui_ops_back: RwLock::new(HashMap::new()),
            plugin_ui_visibility: RwLock::new(plugin_ui_visibility),
            plugin_ui_popped_out: RwLock::new(plugin_ui_popped_out),
            plugin_ui_tabs_selected: RwLock::new(HashMap::new()),
            plugin_state_storage: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn log(&self, msg: Cow<'static, str>) {
        // TODO: is this the right place to log to stdout?
        log::info!("{msg}");
        let mut logs = self.logs.lock().unwrap();
        logs.push_back(msg);
        if logs.len() == logs.capacity() {
            // If the buffer is full, remove 1/4 of its first contents.
            for _ in 0..(logs.capacity() / 4) {
                logs.pop_front();
            }
        }
    }

    pub fn log_plugin(&self, plugin_key: u8, msg: Cow<'static, str>) {
        let mut plugins = self.plugins.write().unwrap();
        let plugin = plugins.get_mut(&plugin_key).unwrap();
        plugin.log(msg);
    }
}

const PLUGIN_LOG_LENGTH: usize = 1000;
#[derive(Deserialize, Serialize)]
pub struct PluginState {
    pub path: Cow<'static, str>,
    pub flags: PluginFlags,
    pub logs: VecDeque<Cow<'static, str>>,
}

#[derive(Deserialize, Serialize)]
pub struct PluginFlags {
    enabled: bool,
    has_error: bool,
}

impl PluginFlags {
    pub fn from_enabled(enabled: bool) -> Self {
        Self {
            enabled,
            has_error: false,
        }
    }
}

impl PluginState {
    pub fn new(path: Cow<'static, str>, enabled: bool) -> Self {
        Self {
            path,
            flags: PluginFlags::from_enabled(enabled),
            logs: VecDeque::with_capacity(PLUGIN_LOG_LENGTH),
        }
    }

    pub fn set_errored(&mut self, v: bool) {
        self.flags.has_error = v;
    }

    pub fn set_enabled(&mut self, v: bool) {
        self.flags.enabled = v;
    }

    pub fn has_errored(&self) -> bool {
        self.flags.has_error
    }

    pub fn is_enabled(&self) -> bool {
        self.flags.enabled
    }

    pub fn log(&mut self, msg: Cow<'static, str>) {
        log::debug!("[WASM] [{}] {msg}", self.path);
        self.logs.push_back(msg);
        if self.logs.len() == self.logs.capacity() {
            // If the buffer is full, remove 1/4 of its first contents.
            for _ in 0..(self.logs.capacity() / 4) {
                self.logs.pop_front();
            }
        }
    }
}
