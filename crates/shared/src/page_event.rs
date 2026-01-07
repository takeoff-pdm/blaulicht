use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};
use strum::EnumIter;

#[derive(Debug, Clone, PartialEq, EnumIter, Serialize, Deserialize, Encode, Decode)]
pub enum AppPage {
    Logs,
    System,
    Audio,
    FixturesSetup,
    View,
    ViewPerformance,
    FixturesPerformance,
    Animations,
}

impl AppPage {
    fn short(&self) -> &'static str {
        match self {
            AppPage::Logs => "Logs",
            AppPage::System => "Sys",
            AppPage::Audio => "Audio",
            AppPage::FixturesSetup => "F. Setup",
            AppPage::View => "Views",
            AppPage::ViewPerformance => "V. Perf",
            AppPage::FixturesPerformance => "F. Perf",
            AppPage::Animations => "Anim",
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Encode, Decode, Clone)]
pub enum MainUiEvent {
    NavigatePage(AppPage),
}
