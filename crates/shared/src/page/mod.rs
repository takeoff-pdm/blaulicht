pub use crate::page::sub::AppSubPage;
use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};
use strum::EnumIter;

pub mod sub;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, EnumIter, Serialize, Deserialize, Encode, Decode,
)]
pub enum AppPage {
    Logs,
    System,
    Audio,
    /// Fixtures, groups and scenes setup. Named `FixturesSetup` before it got
    /// sub-pages; the alias keeps old showfile dock layouts, plugin state and
    /// blctl commands working.
    #[serde(alias = "FixturesSetup")]
    Setup,
    View,
    ViewPerformance,
    FixturesPerformance,
    Animations,
    Palettes,
    SceneGraph,
    Visualizer,
}

impl AppPage {
    pub fn short(&self) -> &'static str {
        match self {
            AppPage::Logs => "Logs",
            AppPage::System => "Sys",
            AppPage::Audio => "Audio",
            AppPage::Setup => "Setup",
            AppPage::View => "Views",
            AppPage::ViewPerformance => "V. Perf",
            AppPage::FixturesPerformance => "F. Perf",
            AppPage::Animations => "Anim",
            AppPage::Palettes => "Palettes",
            AppPage::SceneGraph => "Graph",
            AppPage::Visualizer => "Viz",
        }
    }

    /// Sub-pages (tabs) of this page in tab order; empty for a page without
    /// tabs. Every [`AppSubPage`] whose [`AppSubPage::page`] is `self` must be
    /// listed here exactly once (checked by a test in `page::sub`).
    pub fn list_subpages(&self) -> &'static [AppSubPage] {
        match self {
            AppPage::Logs => &[],
            AppPage::System => &[
                AppSubPage::System_System,
                AppSubPage::System_Dmx,
                AppSubPage::System_ArtNet,
                AppSubPage::System_Plugins,
                AppSubPage::System_Midi,
                AppSubPage::System_Serial,
                AppSubPage::System_Screens,
            ],
            AppPage::Audio => &[],
            AppPage::Setup => &[
                AppSubPage::Setup_Fixtures,
                AppSubPage::Setup_Groups,
                AppSubPage::Setup_Scenes,
            ],
            AppPage::View => &[],
            AppPage::ViewPerformance => &[],
            AppPage::FixturesPerformance => &[],
            AppPage::Animations => &[],
            AppPage::Palettes => &[],
            AppPage::SceneGraph => &[],
            AppPage::Visualizer => &[],
        }
    }

    /// The tab a page shows until another one is selected: the first listed.
    pub fn default_subpage(&self) -> Option<AppSubPage> {
        self.list_subpages().first().copied()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExternalScreenInfo {
    pub index: u32,
    pub width: f32,
    pub height: f32,
    pub x: Option<f32>,
    pub y: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_plugin_id: Option<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ArtNetReceiverInfo {
    pub address: String,
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_plugin_id: Option<u8>,
    pub handle: u32,
}

#[derive(Debug, Serialize, Deserialize, Encode, Decode, Clone)]
pub enum MainUiEvent {
    NavigatePage(AppPage),
    SetPluginUIOpen {
        plugin_id: u8,
        open: bool,
    },
    CreateExternalScreen {
        width: u32,
        height: u32,
    },
    RemoveExternalScreen {
        index: u32,
    },
    CreateOwnedExternalScreen {
        owner_plugin_id: u8,
        width: u32,
        height: u32,
    },
    RemoveOwnedExternalScreen {
        owner_plugin_id: u8,
    },
    /// Relative adjustment of the numberpad dialog that is currently open in
    /// the main UI (e.g. driven by a jog wheel / encoder on a controller).
    /// Positive `delta` increases the entered value, negative decreases it.
    /// Without an open numberpad, adjusts the last scalar fixture setter if
    /// touched within five seconds. Wheel activity renews that window.
    /// Appended to preserve the
    /// bincode indices of existing variants.
    NumberpadAdjust {
        delta: i32,
    },
    /// Show a sub-page (tab) and switch to the page that owns it
    /// ([`AppSubPage::page`]). The UI also emits it when a tab is clicked,
    /// then without switching the main page. Appended to preserve the bincode
    /// indices of existing variants.
    NavigateSubPage(AppSubPage),
}
