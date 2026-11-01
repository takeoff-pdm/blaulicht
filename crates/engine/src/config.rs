use crate::{
    dmx,
    state::{ArtNetOutput, ArtNetReceiver},
};
use anyhow::{anyhow, Context, Result};
use blaulicht_audio_engine::BeatDetection;
use blaulicht_shared::{AppPage, EngineState, SaveEngineState, ShowfileArtNetState};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    sync::{Arc, Mutex},
};
use tracing::debug;

use crate::stage::{StageScene, STAGE_SHOWFILE_VERSION};

const LEGACY_SHOWFILE_VERSION: u32 = 0;

/// First showfile version in which the render base is the ephemeral `BLANK`
/// scene rather than `current_scene_focus`. Anything older needs its stored
/// focus folded into the overlay stack on load.
const BLANK_BASE_SHOWFILE_VERSION: u32 = 4;
static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(1);

#[cfg(feature = "audio")]
use audioviz::spectrum::config::StreamConfig;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Config {
    /// The port remote clients connect to (`crate::server`), on every
    /// network interface.
    pub port: u16,
    /// Whether the engine accepts remote clients at all.
    #[serde(default = "default_server_enabled")]
    pub server_enabled: bool,
    /// The web frontend's bundle (`crates/web/build.sh`), served on `port`
    /// to plain HTTP requests. Relative to this file's folder. Unset: `web/`
    /// next to the binary, or one folder up (`target/web` in a checkout).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub web_ui_dir: Option<PathBuf>,
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
    /// Beat and tempo detector: "legacy" (spectrum only), "tracker" (PCM
    /// tracker, spectral-flux onsets) or "model" (PCM tracker, learned beat
    /// model).
    #[serde(default)]
    pub beat_detection: BeatDetection,
    pub plugins: Vec<PluginConfig>,
    pub last_open_showfile: Option<PathBuf>,
    #[serde(default)]
    pub plugin_state: HashMap<String, String>,
}

fn default_server_enabled() -> bool {
    true
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

/// The desk frontend's part of the showfile (its main screen and external
/// screen layouts). The engine stores it, saves it and hands it back on load
/// (`SystemMessage::ShowLoaded`), but never interprets it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShowfileUiState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub main_screen_desktop_mode: Option<ShowfileExternalScreen>,
    #[serde(default)]
    pub external_screens: Vec<ShowfileExternalScreen>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ShowfileDockSplitAxis {
    Horizontal,
    Vertical,
}

/// A showfile read from disk and parsed, not yet in the engine
/// ([`install_showfile`]). Parsing takes no lock, so the engine keeps running
/// while a file is read.
pub struct ParsedShowfile {
    engine: EngineState,
    /// Older showfiles stored the render base as the focused scene.
    migrate_legacy_base_scene: bool,
    artnet: ShowfileArtNetState,
    plugin_state: HashMap<String, String>,
    stage: StageScene,
    /// The desk frontend's section, if the file has one.
    pub ui: Option<ShowfileUiState>,
}

impl ParsedShowfile {
    /// A new, empty show (closing a showfile).
    pub fn empty() -> Self {
        Self {
            engine: EngineState::default(),
            migrate_legacy_base_scene: false,
            artnet: ShowfileArtNetState::default(),
            plugin_state: HashMap::new(),
            stage: StageScene::default(),
            ui: None,
        }
    }
}

/// Reads and parses the showfile at `file`: the current format, or the
/// deprecated bare engine state (which has no receivers, plugin state, stage
/// or UI section).
pub fn parse_showfile(file: &Path) -> anyhow::Result<ParsedShowfile> {
    debug!("Attempting to read showfile from {file:?}...");

    let string = fs::read_to_string(file)?;

    match serde_json::from_str::<CoreShowfile>(&string) {
        Ok(showfile) => {
            let legacy_coordinates = showfile.format_version == LEGACY_SHOWFILE_VERSION;
            let legacy_base_scene = showfile.format_version < BLANK_BASE_SHOWFILE_VERSION;
            let mut engine: EngineState = showfile
                .engine
                .try_into()
                .map_err(|e| anyhow!("{e}"))
                .with_context(|| "Failed to parse engine state")?;
            if legacy_coordinates {
                migrate_legacy_fixture_coordinates(&mut engine);
            }
            Ok(ParsedShowfile {
                engine,
                migrate_legacy_base_scene: legacy_base_scene,
                artnet: showfile.artnet,
                plugin_state: showfile.plugin_state,
                stage: showfile.stage,
                ui: showfile.ui,
            })
        }
        Err(showfile_err) => match serde_json::from_str::<SaveEngineState>(&string) {
            Ok(deprecated) => {
                let mut engine: EngineState = deprecated
                    .try_into()
                    .map_err(|e| anyhow!("{e}"))
                    .with_context(|| "Failed to parse deprecated engine state")?;
                migrate_legacy_fixture_coordinates(&mut engine);
                Ok(ParsedShowfile {
                    engine,
                    migrate_legacy_base_scene: true,
                    ..ParsedShowfile::empty()
                })
            }
            Err(_) => Err(anyhow!(showfile_err)),
        },
    }
}

/// Puts a parsed show into the engine: show state, stage (with `file` as the
/// open showfile, `None` for a new show), user Art-Net receivers and the
/// per-show plugin state. Plugin-owned receivers stay: they belong to the
/// plugin lifecycle, not the showfile. Returns the frontend section.
pub fn install_showfile(
    showfile: ParsedShowfile,
    file: Option<PathBuf>,
    dmx: &mut dmx::EngineState,
    artnet_output: &mut ArtNetOutput,
    plugin_state_storage: &Mutex<HashMap<String, String>>,
) -> Option<ShowfileUiState> {
    *plugin_state_storage
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = showfile.plugin_state;

    artnet_output
        .receivers
        .retain(|receiver| receiver.owner_plugin_id.is_some());
    artnet_output
        .receivers
        .extend(showfile.artnet.receivers.into_iter().map(|receiver| {
            let mut entry = ArtNetReceiver::user(receiver.address, receiver.enabled);
            if let Some((first, last)) = receiver.universes {
                entry.set_universe_range(first, last);
            }
            entry
        }));

    dmx.load_showfile(showfile.engine, showfile.migrate_legacy_base_scene);
    dmx.load_stage(showfile.stage, file);
    showfile.ui
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

/// File name of a showfile for display, falling back to the whole path when it has none.
pub fn showfile_display_name(path: &Path) -> String {
    path.file_name()
        .unwrap_or(path.as_os_str())
        .to_string_lossy()
        .into_owned()
}

/// Absolute form of `path` for logging; keeps `path` unchanged if it can't be resolved.
pub fn absolute_showfile_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        return path.to_path_buf();
    }
    std::env::current_dir()
        .map(|cwd| cwd.join(path))
        .unwrap_or_else(|_| path.to_path_buf())
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

impl Default for Config {
    fn default() -> Self {
        Self {
            port: 1234,
            server_enabled: default_server_enabled(),
            web_ui_dir: None,
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
            beat_detection: BeatDetection::default(),
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

/// The engine's config and the file it is written to. Only engine-side code
/// writes it: the supervisor (`default_audio_device`) and the persistence
/// service (`last_open_showfile`, global plugin state). Its lock is a leaf:
/// never take it while holding another lock (above all the `dmx_engine`
/// lock), and take no other lock while holding it.
#[derive(Clone)]
pub struct SharedConfig {
    pub config: Arc<Mutex<Config>>,
    pub path: PathBuf,
}

impl SharedConfig {
    pub fn new(config: Config, path: PathBuf) -> Self {
        Self {
            config: Arc::new(Mutex::new(config)),
            path,
        }
    }

    /// Changes the config with `edit` and writes it to the file.
    pub fn update(&self, edit: impl FnOnce(&mut Config)) -> Result<()> {
        let mut config = self
            .config
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        edit(&mut config);
        write_config(self.path.clone(), config.clone()).map(|_| ())
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
                folder: None,
            },
        );

        migrate_legacy_fixture_coordinates(&mut engine);

        let position = &engine.groups[&0].fixtures[&0].pos;
        assert_eq!([position.x, position.y, position.z], [2.0, 4.0, 3.0]);
    }

    #[test]
    fn beat_detection_defaults_to_tracker_and_parses_every_mode() {
        let without = toml::to_string(&Config::default())
            .unwrap()
            .lines()
            .filter(|line| !line.starts_with("beat_detection"))
            .collect::<Vec<_>>()
            .join("\n");
        let parsed: Config = toml::from_str(&without).unwrap();
        assert_eq!(parsed.beat_detection, BeatDetection::Tracker);

        for (value, mode) in [
            ("legacy", BeatDetection::Legacy),
            ("tracker", BeatDetection::Tracker),
            ("model", BeatDetection::Model),
        ] {
            let parsed: Config =
                toml::from_str(&format!("beat_detection = \"{value}\"\n{without}")).unwrap();
            assert_eq!(parsed.beat_detection, mode);
        }
        assert!(
            toml::from_str::<Config>(&format!("beat_detection = \"modle\"\n{without}")).is_err()
        );
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
    fn loading_a_showfile_restores_receiver_universe_ranges() {
        let directory = TempDir::new("blaulicht-artnet-range").unwrap();
        let path = directory.path().join("show.json");
        let showfile = CoreShowfile {
            format_version: current_showfile_version(),
            engine: SaveEngineState::default(),
            artnet: ShowfileArtNetState {
                receivers: vec![
                    blaulicht_shared::ShowfileArtNetReceiver {
                        address: "10.0.0.1:6454".parse().unwrap(),
                        enabled: true,
                        universes: Some((5, 3)),
                    },
                    blaulicht_shared::ShowfileArtNetReceiver {
                        address: "10.0.0.2:6454".parse().unwrap(),
                        enabled: false,
                        universes: None,
                    },
                ],
            },
            plugin_state: HashMap::new(),
            stage: StageScene::default(),
            ui: None,
        };
        fs::write(&path, serde_json::to_vec(&showfile).unwrap()).unwrap();

        let mut dmx = dmx::EngineState::default();
        let mut artnet = ArtNetOutput::default();
        let plugin_state = Mutex::new(HashMap::new());
        install_showfile(
            parse_showfile(&path).unwrap(),
            Some(path),
            &mut dmx,
            &mut artnet,
            &plugin_state,
        );

        let receivers = &artnet.receivers;
        assert_eq!(receivers.len(), 2);
        // Inverted bounds are normalised on load.
        assert_eq!(
            (receivers[0].first_universe, receivers[0].last_universe),
            (3, 5)
        );
        assert!(!receivers[0].sends_universe(2));
        assert!(receivers[0].sends_universe(5));
        // Pre-range showfiles keep sending every universe.
        assert!(receivers[1].sends_all_universes());
    }

    #[test]
    fn a_shared_config_update_is_written_to_its_file() {
        let directory = TempDir::new("blaulicht-shared-config").unwrap();
        let path = directory.path().join("config.toml");
        let shared = SharedConfig::new(Config::default(), path.clone());
        let desk_view = Arc::clone(&shared.config);
        shared
            .update(|config| config.default_audio_device = Some("Dummy: Noise".to_string()))
            .unwrap();
        assert_eq!(
            desk_view.lock().unwrap().default_audio_device.as_deref(),
            Some("Dummy: Noise")
        );
        let written: Config = toml::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(
            written.default_audio_device.as_deref(),
            Some("Dummy: Noise")
        );
    }

    #[test]
    fn a_loaded_showfile_hands_its_stage_to_the_engine() {
        let directory = TempDir::new("blaulicht-stage-load").unwrap();
        let path = directory.path().join("show.json");
        let mut stage = StageScene::default();
        stage.add_object(crate::stage::StageObject::preset(
            crate::stage::StageObjectKind::Box,
            1,
        ));
        let showfile = CoreShowfile {
            format_version: current_showfile_version(),
            engine: SaveEngineState::default(),
            artnet: ShowfileArtNetState::default(),
            plugin_state: HashMap::new(),
            stage: stage.clone(),
            ui: None,
        };
        fs::write(&path, serde_json::to_vec(&showfile).unwrap()).unwrap();

        let mut dmx = dmx::EngineState::default();
        let mut artnet = ArtNetOutput::default();
        let plugin_state = Mutex::new(HashMap::new());
        install_showfile(
            parse_showfile(&path).unwrap(),
            Some(path.clone()),
            &mut dmx,
            &mut artnet,
            &plugin_state,
        );
        assert_eq!(dmx.stage().objects, stage.objects);
        assert_eq!(dmx.showfile(), Some(path.as_path()));

        // Closing installs an empty show without a showfile.
        install_showfile(
            ParsedShowfile::empty(),
            None,
            &mut dmx,
            &mut artnet,
            &plugin_state,
        );
        assert!(dmx.stage().objects.is_empty());
        assert_eq!(dmx.showfile(), None);
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

    #[test]
    fn dock_tabs_saved_as_fixtures_setup_load_as_setup() {
        // Showfiles from before ticket 006 store the Setup page under its old
        // name, both as a bare value and in a page config.
        let legacy: ShowfileDockTab = serde_json::from_str(r#""FixturesSetup""#).unwrap();
        assert_eq!(legacy, ShowfileDockTab::Page(AppPage::Setup));

        let config: ShowfileDockTab =
            serde_json::from_str(r#"{"page":"FixturesSetup","render_mode":"Dynamic"}"#).unwrap();
        assert_eq!(
            config,
            ShowfileDockTab::PageConfig {
                page: AppPage::Setup,
                render_mode: PageRenderMode::Dynamic,
            }
        );
    }

    #[test]
    fn dock_tabs_saved_as_fixtures_performance_load_as_programmer() {
        // Showfiles from before ticket 011 store the Programmer page under its
        // old name.
        let legacy: ShowfileDockTab = serde_json::from_str(r#""FixturesPerformance""#).unwrap();
        assert_eq!(legacy, ShowfileDockTab::Page(AppPage::Programmer));
    }

    #[test]
    fn showfile_display_name_is_the_file_name() {
        assert_eq!(
            showfile_display_name(Path::new("/home/user/shows/club.json")),
            "club.json"
        );
        assert_eq!(showfile_display_name(Path::new("club.json")), "club.json");
        assert_eq!(showfile_display_name(Path::new("/")), "/");
    }

    #[test]
    fn absolute_showfile_path_resolves_relative_paths() {
        let relative = Path::new("shows/club.json");
        let absolute = absolute_showfile_path(relative);
        assert!(absolute.is_absolute());
        assert!(absolute.ends_with(relative));
        let already = Path::new("/srv/club.json");
        assert_eq!(absolute_showfile_path(already), already);
    }
}
