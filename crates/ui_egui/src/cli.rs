//! Command line flags of the egui desk (`#[command(flatten)]` them into the
//! binary's arguments, next to the engine's `EngineArgs`).

use clap::Args;
use std::path::PathBuf;

use crate::layout::UiLayout;

fn parse_size_tuple(s: &str) -> Result<(usize, usize), String> {
    let parts: Vec<&str> = s.split(',').collect();
    if parts.len() != 2 {
        return Err("Expected x,y".into());
    }
    let w = parts[0].parse().map_err(|_| "Bad width")?;
    let h = parts[1].parse().map_err(|_| "Bad height")?;
    Ok((w, h))
}

#[derive(Args, Debug, Clone)]
pub struct UiArgs {
    /// Whether the UI shall include borders / decorations.
    #[arg(short, long, default_value_t = false)]
    pub window_decorations: bool,

    /// UI layout: `mobile` (the 800x480 touch layout, default) or `desktop`
    /// (a resizable window with a navigation sidebar). Wins over `-d`.
    #[arg(long, value_name = "LAYOUT")]
    pub layout: Option<UiLayout>,

    /// Same as `--layout desktop` (kept for scripts; `--desktop` works too).
    #[arg(short, long, visible_alias = "desktop", default_value_t = false)]
    pub desktop_mode: bool,

    /// Specify external screens to add with dimensions (x,y). Can be used multiple times.
    #[arg(short, long, value_parser = parse_size_tuple)]
    pub external_screens: Vec<(usize, usize)>,

    /// Default directory for showfiles.
    #[arg(long, value_name = "DIR")]
    pub showfile_home: Option<PathBuf>,
}

impl UiArgs {
    /// The layout to start in: `--layout`, else desktop with `-d`, else
    /// mobile.
    pub fn layout(&self) -> UiLayout {
        self.layout.unwrap_or(if self.desktop_mode {
            UiLayout::Desktop
        } else {
            UiLayout::Mobile
        })
    }
}

#[cfg(test)]
mod tests {
    use super::UiArgs;
    use crate::layout::UiLayout;
    use clap::Parser;

    #[derive(Parser)]
    struct Cli {
        #[command(flatten)]
        ui: UiArgs,
    }

    fn layout(args: &[&str]) -> UiLayout {
        let argv = std::iter::once("blaulicht-core").chain(args.iter().copied());
        Cli::try_parse_from(argv).unwrap().ui.layout()
    }

    #[test]
    fn layout_flags() {
        assert_eq!(layout(&[]), UiLayout::Mobile);
        assert_eq!(layout(&["--layout", "desktop"]), UiLayout::Desktop);
        assert_eq!(layout(&["--layout=mobile"]), UiLayout::Mobile);
        assert_eq!(layout(&["-d"]), UiLayout::Desktop);
        assert_eq!(layout(&["--desktop-mode"]), UiLayout::Desktop);
        assert_eq!(layout(&["--desktop"]), UiLayout::Desktop);
        // An explicit layout wins over the old flag.
        assert_eq!(layout(&["-d", "--layout", "mobile"]), UiLayout::Mobile);
        assert!(Cli::try_parse_from(["blaulicht-core", "--layout", "tv"]).is_err());
    }
}
