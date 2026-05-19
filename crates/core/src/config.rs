use crate::{
    dmx,
    msg::SystemMessage,
    state::{ArtNetOutput, ArtNetReceiver},
};
use anyhow::{anyhow, Context, Result};
use blaulicht_shared::{AppPage, EngineState, LogLevel, SaveEngineState, ShowfileArtNetState};
use crossbeam_channel::Sender;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, RwLockWriteGuard},
};
use tracing::debug;

#[cfg(feature = "audio")]
use audioviz::spectrum::config::StreamConfig;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Config {
    pub port: u16,
    pub dmx_out_devices: [String; 2],
    pub default_audio_device: Option<String>,

    pub run_setup_on_reload: bool,

    #[cfg(feature = "audio")]
    pub stream: StreamConfig,

    #[cfg(not(feature = "audio"))]
    stream: serde_json::Value,

    #[serde(default = "default_spectrogram_window_seconds")]
    pub spectrogram_window_seconds: u64,
    #[serde(default = "default_spectrogram_refresh_hz")]
    pub spectrogram_refresh_hz: u32,
    pub plugins: Vec<PluginConfig>,
    pub last_open_showfile: Option<PathBuf>,
    #[serde(default)]
    pub plugin_state: HashMap<String, String>,
}

fn default_spectrogram_window_seconds() -> u64 {
    120
}

fn default_spectrogram_refresh_hz() -> u32 {
    60
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PluginConfig {
    pub file_path: String,
    pub enabled: bool,
    pub enable_watcher: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreShowfile {
    pub engine: SaveEngineState,
    #[serde(default)]
    pub artnet: ShowfileArtNetState,
    #[serde(default)]
    pub plugin_state: HashMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui: Option<ShowfileUiState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShowfileUiState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub main_screen_desktop_mode: Option<ShowfileExternalScreen>,
    #[serde(default)]
    pub external_screens: Vec<ShowfileExternalScreen>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShowfileExternalScreen {
    pub width: f32,
    pub height: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_plugin_id: Option<u8>,
    pub layout: ShowfileDockNode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ShowfileDockNode {
    Leaf {
        tabs: Vec<ShowfileDockTab>,
        active: usize,
    },
    Split {
        axis: ShowfileDockSplitAxis,
        fraction: f32,
        first: Box<ShowfileDockNode>,
        second: Box<ShowfileDockNode>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ShowfileDockTab {
    Page(AppPage),
    PluginUi { plugin_id: u8 },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ShowfileDockSplitAxis {
    Horizontal,
    Vertical,
}

pub fn read_showfile(
    file: PathBuf,
    dmx: &mut RwLockWriteGuard<'_, dmx::EngineState>,
    artnet_output: &mut RwLockWriteGuard<'_, ArtNetOutput>,
    plugin_state_storage: &Arc<Mutex<HashMap<String, String>>>,
    system_message_sender: Sender<SystemMessage>,
) -> Option<ShowfileUiState> {
    match read_showfile_logic(file.clone(), dmx, artnet_output, plugin_state_storage) {
        Ok(ui_state) => {
            system_message_sender
                .send(SystemMessage::Log(
                    format!("Loaded showfile from {file:?}"),
                    LogLevel::Info,
                ))
                .unwrap();
            ui_state
        }
        Err(e) => {
            system_message_sender
                .send(SystemMessage::Log(
                    format!("Read showfile <{file:?}> ERR: {e}"),
                    LogLevel::Err,
                ))
                .unwrap();
            None
        }
    }
}

fn read_showfile_logic(
    file: PathBuf,
    dmx: &mut RwLockWriteGuard<'_, dmx::EngineState>,
    artnet_output: &mut RwLockWriteGuard<'_, ArtNetOutput>,
    plugin_state_storage: &Arc<Mutex<HashMap<String, String>>>,
) -> anyhow::Result<Option<ShowfileUiState>> {
    debug!("Attempting to read showfile from {file:?}...");

    let string = fs::read_to_string(&file)?;

    match serde_json::from_str::<CoreShowfile>(&string) {
        Ok(showfile) => {
            let core_engine: EngineState = showfile
                .engine
                .try_into()
                .map_err(|e| anyhow!("{e}"))
                .with_context(|| "Failed to parse engine state")?;

            let mut storage = plugin_state_storage.lock().unwrap();
            *storage = showfile.plugin_state;

            artnet_output.receivers = showfile
                .artnet
                .receivers
                .into_iter()
                .map(|receiver| ArtNetReceiver {
                    address: receiver.address,
                    enabled: receiver.enabled,
                })
                .collect();

            dmx.load_showfile(core_engine);

            Ok(showfile.ui)
        }
        Err(showfile_err) => match serde_json::from_str::<SaveEngineState>(&string) {
            Ok(deprecated) => {
                let core_format: EngineState = deprecated
                    .try_into()
                    .map_err(|e| anyhow!("{e}"))
                    .with_context(|| "Failed to parse deprecated engine state")?;

                {
                    let mut storage = plugin_state_storage.lock().unwrap();
                    storage.clear();
                }

                artnet_output.receivers.clear();
                dmx.load_showfile(core_format);

                Ok(None)
            }
            Err(_) => Err(anyhow!(showfile_err)),
        },
    }
}

pub fn read_showfile_ui_state(file: PathBuf) -> anyhow::Result<Option<ShowfileUiState>> {
    let string = fs::read_to_string(&file)?;

    match serde_json::from_str::<CoreShowfile>(&string) {
        Ok(showfile) => Ok(showfile.ui),
        Err(showfile_err) => match serde_json::from_str::<SaveEngineState>(&string) {
            Ok(_) => Ok(None),
            Err(_) => Err(anyhow!(showfile_err)),
        },
    }
}

pub fn close_showfile(
    dmx: &mut RwLockWriteGuard<'_, dmx::EngineState>,
    artnet_output: &mut RwLockWriteGuard<'_, ArtNetOutput>,
    plugin_state_storage: &Arc<Mutex<HashMap<String, String>>>,
) {
    debug!("Closing showfile...");

    let mut storage = plugin_state_storage.lock().unwrap();
    *storage = HashMap::new();

    artnet_output.receivers.clear();

    dmx.load_showfile(EngineState::default());
}

impl Default for Config {
    fn default() -> Self {
        Self {
            port: 1234,
            dmx_out_devices: ["/dev/dmx_out0".to_string(), "/dev/dmx_out1".to_string()],
            default_audio_device: None,

            #[cfg(not(feature = "audio"))]
            stream: serde_json::Value::Null,

            #[cfg(feature = "audio")]
            stream: StreamConfig {
                // TODO: also experiment with fft resolution
                // gravity: None, // OR: Some(100)
                gravity: Some(100.0),
                ..StreamConfig::default()
            },

            run_setup_on_reload: false,

            spectrogram_window_seconds: default_spectrogram_window_seconds(),
            spectrogram_refresh_hz: default_spectrogram_refresh_hz(),
            plugins: vec![PluginConfig {
                file_path: "./plugins/hello_world.wasm".to_string(),
                enabled: false,
                enable_watcher: false,
            }],
            last_open_showfile: None,
            plugin_state: HashMap::new(),
        }
    }
}

pub fn config_path() -> Result<PathBuf> {
    Ok("~/blaulicht.toml".into())
}

pub fn read_config(file_path: PathBuf) -> Result<Option<Config>> {
    // Either read or create a configuration file based on it's current existence
    let path = Path::new(&file_path);
    match &path.exists() {
        true => {
            // The file exists, it can be read
            debug!(
                "Found existing config file at {}",
                file_path.to_string_lossy()
            );
            let content = fs::read_to_string(path)?;
            let config = toml::from_str(&content)?;
            Ok(Some(config))
        }
        false => {
            // The file does not exist, therefore create a new one
            fs::create_dir_all(path.parent().unwrap())?;
            let mut file = File::create(path)?;
            file.write_all(
                toml::to_string_pretty(&Config::default())
                    .unwrap()
                    .as_bytes(),
            )
            .with_context(|| "Failed to write default config file (create new one)")?;
            Ok(None)
        }
    }
}

pub fn write_config(file_path: PathBuf, config: Config) -> Result<Option<Config>> {
    // Either read or create a configuration file based on it's current existence
    let path = Path::new(&file_path);
    fs::create_dir_all(path.parent().unwrap())?;
    let mut file = File::create(path)?;
    file.write_all(toml::to_string_pretty(&config).unwrap().as_bytes())
        .with_context(|| "Failed to write default config file (create new one)")?;
    Ok(None)
}
