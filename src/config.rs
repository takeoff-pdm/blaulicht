use std::{
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use log::debug;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct Config {
    pub port: u16,
    pub extra_serial_paths: Vec<PathBuf>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            port: 1234,
            extra_serial_paths: vec!["/dev/pts/0".into()],
        }
    }
}

pub fn config_path() -> Result<PathBuf> {
    Ok("~/blualicht.toml".into())
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
