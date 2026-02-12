use std::{
    array,
    borrow::Cow,
    collections::{HashMap, VecDeque},
    net::{SocketAddr, ToSocketAddrs},
    sync::{Arc, Mutex, RwLock},
    time::Duration,
};

use blaulicht_audio_engine::{AudioSpectrogram, SignalCollectorParams};
use crossbeam_channel::{Receiver, Sender};
use serde::{Deserialize, Serialize};

use crate::{
    audio::defs::AudioThreadControlSignal,
    config::{Config, PluginConfig},
    dmx::EngineState,
    event::SystemEventBusConnectionInst,
    msg::{FromFrontend, SystemMessage},
    plugin::{midi::MidiError, serial::SerialError},
    ui_ops::WasmUiOp,
};

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

pub const NUM_DMX_UNIVERSES: usize = 2;

pub enum DmxHealthState {
    Healthy,
    Error(String),
}

pub struct DmxHealth {
    pub port: String,
    pub state: DmxHealthState,
}

impl Default for DmxHealth {
    fn default() -> Self {
        Self {
            port: "".to_string(),
            state: DmxHealthState::Error("Not Initialized".to_string()),
        }
    }
}

impl DmxHealth {
    pub fn is_healthy(&self) -> bool {
        match self.state {
            DmxHealthState::Healthy => true,
            DmxHealthState::Error(_) => false,
        }
    }

    pub fn healthy(port: String) -> Self {
        Self {
            state: DmxHealthState::Healthy,
            port,
        }
    }

    pub fn error(port: String, error: String) -> Self {
        Self {
            port,
            state: DmxHealthState::Error(error),
        }
    }
}

pub enum MidiHealthError {
    NotFound { device_name: String },
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum MidiDeviceState {
    Error(MidiError),
    Open(usize), // Number of handles on the device
}

#[derive(Default)]
pub struct MidiHealth {
    pub available_devices: Vec<String>,
    // pub errors: Vec<MidiHealthError>,
    pub devices: HashMap<String, MidiDeviceState>,
}

impl MidiHealth {
    pub fn is_healthy(&self) -> bool {
        !self
            .devices
            .iter()
            .any(|(_, state)| matches!(state, MidiDeviceState::Error(_)))
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum SerialDeviceState {
    Error(SerialError),
    Open(usize),
}

#[derive(Default, Clone)]
pub struct SerialHealth {
    pub devices: HashMap<String, SerialDeviceState>,
}

impl SerialHealth {
    pub fn is_healthy(&self) -> bool {
        !self
            .devices
            .iter()
            .any(|(_, state)| matches!(state, SerialDeviceState::Error(_)))
    }
}

pub struct AppHealthState {
    pub dmx_universes_healthy: [DmxHealth; NUM_DMX_UNIVERSES],
    pub artnet_health_state: bool,
    pub midi_health: MidiHealth,
    pub serial_health: SerialHealth,
}

#[derive(Clone)]
pub struct ArtNetReceiver {
    pub address: SocketAddr,
    pub enabled: bool,
}

impl ArtNetReceiver {
    pub fn new(address: SocketAddr) -> Self {
        Self {
            address,
            enabled: true,
        }
    }
}

#[derive(Default, Clone)]
pub struct ArtNetOutput {
    pub receivers: Vec<ArtNetReceiver>,
}

#[derive(PartialEq, Eq, Hash, Clone, Copy)]
pub struct ScreenId(usize);

impl ScreenId {
    pub const MAIN: Self = Self(0);

    pub fn external(index: usize) -> Self {
        Self(index + 1)
    }
}

#[derive(Clone, Copy)]
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

pub struct AppState {
    pub logs: Mutex<VecDeque<Cow<'static, str>>>,
    pub plugins: RwLock<HashMap<u8, PluginState>>,
    pub health_data: RwLock<AppHealthState>,
    pub dmx_engine: RwLock<EngineState>,
    pub audio: RwLock<AudioState>,
    pub dmx_universes: [RwLock<DmxBuffer>; NUM_DMX_UNIVERSES],
    pub artnet_output: RwLock<ArtNetOutput>,
    // pub audio_snapshot: RwLock<CollectedAudioSnapshot>,
    pub audio_params: RwLock<SignalCollectorParams>, // Boolean stores if something has
    // changed
    pub audio_spectrogram: RwLock<AudioSpectrogram>,
    pub mainloop_state: RwLock<AudioThreadControlSignal>,
    pub plugin_ui_ops: RwLock<HashMap<u8, Vec<WasmUiOp>>>,
    // Back buffer for plugin UI ops. Plugins write here; UI reads from `plugin_ui_ops`.
    pub plugin_ui_ops_back: RwLock<HashMap<u8, Vec<WasmUiOp>>>,
    // per-plugin UI window visibility (also per-screen.)
    // This is (plugin-id, screen_id) to visibility.
    pub plugin_ui_visibility: RwLock<HashMap<u8, PluginOpenState>>,
    pub plugin_ui_popped_out: RwLock<HashMap<u8, bool>>, // per-plugin UI window pop-out state
    pub plugin_ui_tabs_selected: RwLock<HashMap<(u8, u8), u8>>, // (plugin_id, tabs_id) -> tab_id
    pub plugin_state_storage: Arc<Mutex<HashMap<String, String>>>,
    pub plugin_state_storage_global: Arc<Mutex<HashMap<String, String>>>,
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
            plugin_ui_visibility.insert(i as u8, PluginOpenState::CLOSED);
            plugin_ui_popped_out.insert((i as u8), false);
        }

        Self {
            logs: Mutex::new(VecDeque::with_capacity(APP_LOG_LENGTH)),
            plugins: RwLock::new(plugins_map),
            health_data: RwLock::new(AppHealthState {
                dmx_universes_healthy: array::from_fn(|_| DmxHealth::default()),
                artnet_health_state: false,
                midi_health: MidiHealth::default(),
                serial_health: SerialHealth::default(),
            }),
            artnet_output: RwLock::new(ArtNetOutput::default()),
            dmx_engine: RwLock::new(EngineState::default()),
            dmx_universes: [RwLock::new(DmxBuffer::new()), RwLock::new(DmxBuffer::new())],
            audio: RwLock::new(AudioState::default()),
            // audio_snapshot: RwLock::new(CollectedAudioSnapshot::default()),
            audio_params: RwLock::new(SignalCollectorParams::default()),
            audio_spectrogram: RwLock::new(AudioSpectrogram::new(
                40,
                128,
                Duration::from_millis(20),
            )),
            mainloop_state: RwLock::new(AudioThreadControlSignal::ABORTED),
            plugin_ui_ops: RwLock::new(HashMap::new()),
            plugin_ui_ops_back: RwLock::new(HashMap::new()),
            plugin_ui_visibility: RwLock::new(plugin_ui_visibility),
            plugin_ui_popped_out: RwLock::new(plugin_ui_popped_out),
            plugin_ui_tabs_selected: RwLock::new(HashMap::new()),
            plugin_state_storage: Arc::new(Mutex::new(HashMap::new())),
            plugin_state_storage_global: Arc::new(Mutex::new(HashMap::new())),
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
