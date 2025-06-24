use std::ops::Range;

use super::animation::AnimationAlternating;

#[derive(Debug)]
pub struct Mood {
    // Loops from 0 to 360.
    pub counter_range_index: usize,
    pub counter_in_range_value: isize,
    pub last_counter_update: u32,
    pub hsv: isize,
    pub animation_speed: f32,
    pub animation_speed_beat: f32,
    pub controls: MoodControls,
}

#[derive(Debug, Clone)]
pub struct MoodControls {
    pub on_beat: bool,
    pub force: bool,
    pub brightness: u8,
    pub animation: MoodAnimation,
    pub color_palette: MoodColorPalette,
    pub animation_on_beat: bool,
}

#[derive(Debug, PartialEq, Clone)]
pub enum MoodColorPalette {
    All,
    CyanMagenta,
    OrangeBlue,
    White,
}

impl MoodColorPalette {
    pub fn to_hsv_ranges(&self) -> Vec<Range<isize>> {
        match self {
            Self::All => vec![0..361],
            // TODO: fix this kind of animation.
            Self::CyanMagenta => vec![180..180, 345..345],
            Self::OrangeBlue => vec![240..241, 40..41],
            Self::White => vec![-1..-1],
        }
    }
}

#[derive(Debug, PartialEq, Clone)]
pub enum MoodAnimation {
    Synced,
    Alternating(AnimationAlternating),
    LeftRightSnake(AnimationAlternating),
}

impl MoodAnimation {
    pub fn alternating() -> Self {
        Self::Alternating(AnimationAlternating::default())
    }

    pub fn snake() -> Self {
        Self::LeftRightSnake(AnimationAlternating::default())
    }
}