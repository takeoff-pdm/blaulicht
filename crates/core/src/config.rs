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
    sync::atomic::{AtomicU64, Ordering},
    sync::{Arc, Mutex, RwLockWriteGuard},
};
use tracing::debug;

use crate::stage::{StageScene, STAGE_SHOWFILE_VERSION};

const LEGACY_SHOWFILE_VERSION: u32 = 0;
static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(1);

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
    #[serde(default)]
    pub format_version: u32,
    pub engine: SaveEngineState,
    #[serde(default)]
    pub artnet: ShowfileArtNetState,
    #[serde(default)]
    pub plugin_state: HashMap<String, String>,
    #[serde(default)]
    pub stage: StageScene,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui: Option<ShowfileUiState>,
}

#[derive(Debug, Clone, Default)]
pub struct LoadedShowfileState {
    pub ui: Option<ShowfileUiState>,
    pub stage: StageScene,
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
    PageConfig {
        page: AppPage,
        #[serde(default)]
        render_mode: PageRenderMode,
    },
    PluginUi {
        plugin_id: u8,
        #[serde(default)]
        render_mode: PageRenderMode,
    },
    // Legacy showfiles stored core pages as bare enum values.
    Page(AppPage),
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PageRenderMode {
    #[default]
    Default,
    Dynamic,
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
) -> Option<LoadedShowfileState> {
    match read_showfile_logic(file.clone(), dmx, artnet_output, plugin_state_storage) {
        Ok(loaded_state) => {
            let _ = system_message_sender.send(SystemMessage::Log(
                format!("Loaded showfile from {file:?}"),
                LogLevel::Info,
            ));
            Some(loaded_state)
        }
        Err(e) => {
            let _ = system_message_sender.send(SystemMessage::Log(
                format!("Read showfile <{file:?}> ERR: {e}"),
                LogLevel::Err,
            ));
            None
        }
    }
}

fn read_showfile_logic(
    file: PathBuf,
    dmx: &mut RwLockWriteGuard<'_, dmx::EngineState>,
    artnet_output: &mut RwLockWriteGuard<'_, ArtNetOutput>,
    plugin_state_storage: &Arc<Mutex<HashMap<String, String>>>,
) -> anyhow::Result<LoadedShowfileState> {
    debug!("Attempting to read showfile from {file:?}...");

    let string = fs::read_to_string(&file)?;

    match serde_json::from_str::<CoreShowfile>(&string) {
        Ok(showfile) => {
            let legacy_coordinates = showfile.format_version == LEGACY_SHOWFILE_VERSION;
            let mut core_engine: EngineState = showfile
                .engine
                .try_into()
                .map_err(|e| anyhow!("{e}"))
                .with_context(|| "Failed to parse engine state")?;
            if legacy_coordinates {
                migrate_legacy_fixture_coordinates(&mut core_engine);
            }

            let mut storage = plugin_state_storage.lock().unwrap();
            *storage = showfile.plugin_state;

            // Preserve plugin-owned receivers across showfile load — they belong to the
            // plugin lifecycle, not the showfile.
            let preserved: Vec<ArtNetReceiver> = artnet_output
                .receivers
                .iter()
                .filter(|r| r.owner_plugin_id.is_some())
                .cloned()
                .collect();
            artnet_output.receivers =
                preserved
                    .into_iter()
                    .chain(
                        showfile.artnet.receivers.into_iter().map(|receiver| {
                            ArtNetReceiver::user(receiver.address, receiver.enabled)
                        }),
                    )
                    .collect();

            dmx.load_showfile(core_engine);

            let mut stage = showfile.stage;
            stage.sanitize();
            Ok(LoadedShowfileState {
                ui: showfile.ui,
                stage,
            })
        }
        Err(showfile_err) => match serde_json::from_str::<SaveEngineState>(&string) {
            Ok(deprecated) => {
                let mut core_format: EngineState = deprecated
                    .try_into()
                    .map_err(|e| anyhow!("{e}"))
                    .with_context(|| "Failed to parse deprecated engine state")?;
                migrate_legacy_fixture_coordinates(&mut core_format);

                {
                    let mut storage = plugin_state_storage.lock().unwrap();
                    storage.clear();
                }

                // Deprecated showfile format has no receivers — drop user-created ones
                // but keep plugin-owned receivers, which live outside the showfile.
                artnet_output
                    .receivers
                    .retain(|r| r.owner_plugin_id.is_some());
                dmx.load_showfile(core_format);

                Ok(LoadedShowfileState::default())
            }
            Err(_) => Err(anyhow!(showfile_err)),
        },
    }
}

pub fn read_showfile_ui_state(file: PathBuf) -> anyhow::Result<LoadedShowfileState> {
    let string = fs::read_to_string(&file)?;

    match serde_json::from_str::<CoreShowfile>(&string) {
        Ok(showfile) => {
            let mut stage = showfile.stage;
            stage.sanitize();
            Ok(LoadedShowfileState {
                ui: showfile.ui,
                stage,
            })
        }
        Err(showfile_err) => match serde_json::from_str::<SaveEngineState>(&string) {
            Ok(_) => Ok(LoadedShowfileState::default()),
            Err(_) => Err(anyhow!(showfile_err)),
        },
    }
}

fn migrate_legacy_fixture_coordinates(engine: &mut EngineState) {
    for fixture in engine
        .groups
        .values_mut()
        .flat_map(|group| group.fixtures.values_mut())
    {
        let legacy = fixture.pos.clone();
        fixture.pos.x = legacy.x * 0.1;
        fixture.pos.y = legacy.z * 0.1;
        fixture.pos.z = legacy.y * 0.1;
    }
}

pub const fn current_showfile_version() -> u32 {
    STAGE_SHOWFILE_VERSION
}

pub fn write_atomic(path: &Path, contents: &[u8]) -> anyhow::Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("showfile.json");
    let sequence = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let temporary = parent.join(format!(
        ".{file_name}.{}.{}.tmp",
        std::process::id(),
        sequence
    ));
    let result = (|| {
        let mut file = File::create(&temporary)?;
        file.write_all(contents)?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

pub fn close_showfile(
    dmx: &mut RwLockWriteGuard<'_, dmx::EngineState>,
    artnet_output: &mut RwLockWriteGuard<'_, ArtNetOutput>,
    plugin_state_storage: &Arc<Mutex<HashMap<String, String>>>,
) {
    debug!("Closing showfile...");

    let mut storage = plugin_state_storage.lock().unwrap();
    *storage = HashMap::new();

    // Drop user-created receivers; plugin-owned receivers live outside the showfile.
    artnet_output
        .receivers
        .retain(|r| r.owner_plugin_id.is_some());

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
            if let Some(parent) = path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
            {
                fs::create_dir_all(parent)?;
            }
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
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    let mut file = File::create(path)?;
    file.write_all(toml::to_string_pretty(&config).unwrap().as_bytes())
        .with_context(|| "Failed to write default config file (create new one)")?;
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use blaulicht_shared::fixture::{
        light::Light,
        state::{Fixture, FixtureGroup, Position},
        FixtureType,
    };
    use std::collections::BTreeMap;
    use tempdir::TempDir;

    #[test]
    fn legacy_fixture_coordinates_are_converted_to_world_meters() {
        let mut engine = EngineState::default();
        let mut fixture = Fixture::new(
            0,
            1,
            "Fixture".into(),
            FixtureType::from(Light::Generic3ChanNoAlpha),
        );
        fixture.pos = Position {
            x: 20.0,
            y: 30.0,
            z: 40.0,
        };
        engine.groups.insert(
            0,
            FixtureGroup {
                name: "Group".into(),
                fixtures: BTreeMap::from([(0, fixture)]),
            },
        );

        migrate_legacy_fixture_coordinates(&mut engine);

        let position = &engine.groups[&0].fixtures[&0].pos;
        assert_eq!([position.x, position.y, position.z], [2.0, 4.0, 3.0]);
    }

    #[test]
    fn old_core_showfiles_receive_an_empty_stage() {
        let current = CoreShowfile {
            format_version: current_showfile_version(),
            engine: SaveEngineState::default(),
            artnet: ShowfileArtNetState::default(),
            plugin_state: HashMap::new(),
            stage: StageScene::default(),
            ui: None,
        };
        let mut value = serde_json::to_value(current).unwrap();
        let object = value.as_object_mut().unwrap();
        object.remove("format_version");
        object.remove("stage");

        let loaded: CoreShowfile = serde_json::from_value(value).unwrap();

        assert_eq!(loaded.format_version, LEGACY_SHOWFILE_VERSION);
        assert_eq!(loaded.stage, StageScene::default());
    }

    #[test]
    fn atomic_write_replaces_contents_without_leaving_temporary_files() {
        let directory = TempDir::new("blaulicht-atomic-write").unwrap();
        let path = directory.path().join("show.json");
        fs::write(&path, b"old").unwrap();

        write_atomic(&path, b"new showfile").unwrap();

        assert_eq!(fs::read(&path).unwrap(), b"new showfile");
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    }
}
