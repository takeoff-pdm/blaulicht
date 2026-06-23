use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};
use strum::EnumIter;

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
    SceneGraph,
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
            AppPage::SceneGraph => "Graph",
            AppPage::Visualizer => "Viz",
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Encode, Decode, Clone)]
pub enum MainUiEvent {
    NavigatePage(AppPage),
    SetPluginUIOpen { plugin_id: u8, open: bool },
}
