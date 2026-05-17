pub use crate::page::sub::AppSubPage;
use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};
use strum::EnumIter;

pub mod sub;

#[derive(Debug, Clone, Copy, PartialEq, EnumIter, Serialize, Deserialize, Encode, Decode)]
pub enum AppPage {
    Logs,
    System,
    Audio,
    FixturesSetup,
    View,
    ViewPerformance,
    FixturesPerformance,
    Animations,
    Visualizer,
}

impl AppPage {
    pub fn short(&self) -> &'static str {
        match self {
            AppPage::Logs => "Logs",
            AppPage::System => "Sys",
            AppPage::Audio => "Audio",
            AppPage::FixturesSetup => "F. Setup",
            AppPage::View => "Views",
            AppPage::ViewPerformance => "V. Perf",
            AppPage::FixturesPerformance => "F. Perf",
            AppPage::Animations => "Anim",
            AppPage::Visualizer => "Viz",
        }
    }

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
            ],
            AppPage::Audio => &[],
            AppPage::FixturesSetup => &[],
            AppPage::View => &[],
            AppPage::ViewPerformance => &[],
            AppPage::FixturesPerformance => &[],
            AppPage::Animations => &[],
            AppPage::Visualizer => &[],
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Encode, Decode, Clone)]
pub enum MainUiEvent {
    NavigatePage(AppPage),
    SetPluginUIOpen { plugin_id: u8, open: bool },
}
