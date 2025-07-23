use std::{collections::HashMap, ops::Range};

use super::config::Config;


#[derive(Debug)]
pub struct Fader {
    pub left_target: LeftFaderTarget,
    pub right_target: RightFaderTarget,
}

#[derive(Debug, PartialEq)]
pub enum LeftFaderTarget {
    StrobeBrightness,
    MovingHeadPan,
}

impl LeftFaderTarget {
    pub fn is_alt(&self) -> bool {
        *self != Self::StrobeBrightness
    }
}

#[derive(Debug, PartialEq)]
pub enum RightFaderTarget {
    MoodBrightness,
    MovingHeadTilt,
}

impl RightFaderTarget {
    pub fn is_alt(&self) -> bool {
        *self != Self::MoodBrightness
    }
}

// #[derive(Debug)]
// pub struct Animation {
//     pub strobe: Strobe,
//     pub mood: Mood,
//     pub video: Video,
//     pub dim: Dimmer,
// }
