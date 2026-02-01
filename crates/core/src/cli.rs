use std::path::PathBuf;

use clap::Parser;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct CliArgs {
    /// Whether the UI shall include borders / decorations.
    #[arg(short, long, default_value_t = false)]
    pub window_decorations: bool,

    /// Config file location.
    #[arg(short, long)]
    pub config_file: Option<PathBuf>,
}
