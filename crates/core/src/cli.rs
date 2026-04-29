use clap::Parser;
use std::path::PathBuf;

fn parse_size_tuple(s: &str) -> Result<(usize, usize), String> {
    let parts: Vec<&str> = s.split(',').collect();
    if parts.len() != 2 {
        return Err("Expected x,y".into());
    }
    let w = parts[0].parse().map_err(|_| "Bad width")?;
    let h = parts[1].parse().map_err(|_| "Bad height")?;
    Ok((w, h))
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct CliArgs {
    /// Whether the UI shall include borders / decorations.
    #[arg(short, long, default_value_t = false)]
    pub window_decorations: bool,

    /// Whether the UI shall be launched in desktop mode.
    #[arg(short, long, default_value_t = false)]
    pub desktop_mode: bool,

    /// Specify external screens to add with dimensions (x,y). Can be used multiple times.
    #[arg(short, long, value_parser = parse_size_tuple)]
    pub external_screens: Vec<(usize, usize)>,

    /// Config file location.
    #[arg(short, long)]
    pub config_file: Option<PathBuf>,

    /// Default directory for showfiles.
    #[arg(long, value_name = "DIR")]
    pub showfile_home: Option<PathBuf>,
}
