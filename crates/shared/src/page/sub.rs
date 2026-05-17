use core::fmt::Display;

use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};
use strum::EnumIter;

#[derive(Debug, Clone, Copy, PartialEq, EnumIter, Serialize, Deserialize, Encode, Decode)]
pub enum AppSubPage {
    // System Page
    System_System,
    System_Dmx,
    System_ArtNet,
    System_Midi,
    System_Serial,
    System_Plugins,
}

impl Display for AppSubPage {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let str = match self {
            AppSubPage::System_System => "System",
            AppSubPage::System_Dmx => "Dmx",
            AppSubPage::System_ArtNet => "Artnet",
            AppSubPage::System_Midi => "Midi",
            AppSubPage::System_Serial => "Serial",
            AppSubPage::System_Plugins => "Plugins",
        };

        write!(f, "{str}")
    }
}
