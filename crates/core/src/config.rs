use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::RwLockWriteGuard,
};

use anyhow::{anyhow, Context, Result};
use audioviz::spectrum::config::StreamConfig;
use log::{debug, error};
use serde::{Deserialize, Serialize};

use crate::dmx;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Config {
    pub port: u16,
    pub default_audio_device: Option<String>,
    pub stream: StreamConfig,
    pub plugins: Vec<PluginConfig>,
    pub last_open_showfile: Option<PathBuf>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PluginConfig {
    pub file_path: String,
    pub enabled: bool,
    pub enable_watcher: bool,
}

pub fn read_showfile(
    file: PathBuf,
    dmx: &mut RwLockWriteGuard<'_, dmx::EngineState>,
) -> anyhow::Result<()> {
    debug!("Attempting to read showfile from {file:?}...");

    let mut f = File::open(&file)?;
    let metadata = fs::metadata(&file)?;
    let mut buffer = vec![0; metadata.len() as usize];
    f.read(&mut buffer)?;

    match postcard::from_bytes(&buffer) {
        Ok(de) => {
            dmx.load_showfile(de);
            Ok(())
        }
        Err(e) => Err(anyhow!(e)),
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            port: 1234,
            default_audio_device: None,
            stream: StreamConfig {
                // TODO: also experiment with fft resolution
                // gravity: None, // OR: Some(100)
                gravity: Some(100.0),
                ..Default::default()
            },
            plugins: vec![PluginConfig {
                file_path: "./plugins/hello_world.wasm".to_string(),
                enabled: false,
                enable_watcher: false,
            }],
            last_open_showfile: None,
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
