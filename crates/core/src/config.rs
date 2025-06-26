use std::{
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use audioviz::spectrum::config::StreamConfig;
use log::debug;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Config {
    pub port: u16,
    pub default_audio_device: Option<String>,
    pub stream: StreamConfig,
    pub plugins: Vec<PluginConfig>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PluginConfig {
    pub file_path: String,
    pub enabled: bool,
    pub enable_watcher: bool,
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
