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
            AppPage::SceneGraph => &[],
            AppPage::Visualizer => &[],
        }
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
}
