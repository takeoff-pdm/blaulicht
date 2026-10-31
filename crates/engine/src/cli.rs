//! Command line flags of the engine, shared by every binary that runs it
//! (`#[command(flatten)]` them into the binary's own arguments).

use clap::Args;
use std::path::PathBuf;

#[derive(Args, Debug, Clone)]
pub struct EngineArgs {
    /// Config file location.
    #[arg(short, long)]
    pub config_file: Option<PathBuf>,
}

impl EngineArgs {
    /// The config file, `./config.toml` by default.
    pub fn config_file(&self) -> PathBuf {
        self.config_file
            .clone()
            .unwrap_or_else(|| PathBuf::from("./config.toml"))
    }
}
