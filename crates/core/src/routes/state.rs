use std::{
    borrow::Cow,
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex, RwLock},
};

use crossbeam_channel::Receiver;
use crossbeam_queue::ArrayQueue;
use heapless::spsc;
use serde::{Deserialize, Serialize};

use crate::{
    config::{Config, PluginConfig}, dmx::EngineState, event::{SystemEventBusConnection, SystemEventBusConnectionInst}, msg::{FromFrontend, Signal, SystemMessage, UnifiedMessage}, plugin::Plugin
};

pub struct AppStateWrapper {
    pub from_frontend_sender: crossbeam_channel::Sender<FromFrontend>,

    pub system_message_receiver: Receiver<SystemMessage>,
    pub signal_receiver: Receiver<Signal>,

    // pub to_frontend_consumers:
    //     Arc<Mutex<HashMap<String, crossbeam_channel::Sender<UnifiedMessage>>>>,
    pub config: Arc<Mutex<Config>>,
    pub config_path: String,
    // System event bus connection.
    pub event_bus_connection: SystemEventBusConnectionInst,

    // Real state.
    pub state: Arc<AppState>,
}

const APP_LOG_LENGTH: usize = 1000;

#[derive(Serialize, Deserialize)]
pub struct AudioState {
    pub device_name: Option<String>,
}

impl AudioState {
    pub fn default() -> Self {
        Self {
            device_name: None,
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct AppState {
    pub logs: Mutex<VecDeque<Cow<'static, str>>>,
    pub plugins: RwLock<HashMap<u8, PluginState>>,
    pub dmx_engine: RwLock<EngineState>,
    pub audio: RwLock<AudioState>,
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

        Self {
            logs: Mutex::new(VecDeque::with_capacity(APP_LOG_LENGTH)),
            plugins: RwLock::new(plugins_map),
            dmx_engine: RwLock::new(EngineState::default()),
            audio: RwLock::new(AudioState::default())
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
    path: Cow<'static, str>,
    flags: PluginFlags,
    logs: VecDeque<Cow<'static, str>>,
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

    pub fn is_active(&self) -> bool {
        self.flags.enabled && !self.flags.has_error
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
