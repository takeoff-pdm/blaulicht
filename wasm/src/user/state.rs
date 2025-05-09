use std::{collections::HashMap, ops::Range};

#[derive(Debug)]
pub struct State {
    pub was_initial: bool,
    pub init_time: u32,
    pub last_beat_time: u32,

    pub faders: Fader,

    pub animation: Animation,
}

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

#[derive(Debug)]
pub struct Animation {
    pub strobe: Strobe,
    pub mood: Mood,
    pub video: Video,
}

#[derive(Debug, Clone, Copy)]
pub struct Video {
    pub last_speed_update: u32,
    pub speed: f32,

    pub speed_bpm_sync: bool,
    pub speed_bpm_sync_last_factor: f32,
    pub speed_sync_last_update: u32,

    pub brightness: u8,
    pub brightness_strobe_synced: bool,
    pub fry: u8,

    pub file: VideoFile,
}

#[derive(Debug, Clone, Copy)]
pub enum VideoFile {
    Cheese,
    Grr,
    Swim,
    Cyonic,
    Jacky,
    Loveletter,
    Platzhalter,
    Molly,
}

impl VideoFile {
    pub fn path(&self) -> &'static str {
        match self {
            VideoFile::Cheese => "cheese.webm",
            VideoFile::Grr => "grr.webm",
            VideoFile::Swim => "swim.webm",
            VideoFile::Cyonic => "cyonic.webm",
            VideoFile::Jacky => "jacky.webm",
            VideoFile::Loveletter => "loveletter.webm",
            VideoFile::Platzhalter => "platzhalter.webm",
            VideoFile::Molly => "molly.webm",
        }
    }

    pub fn speed(&self) -> f32 {
        match self {
            VideoFile::Cheese => 1.1,
            VideoFile::Grr => 4.0,
            VideoFile::Swim => 1.21,
            _ => 1.0,
        }
    }

    pub fn base_bpm(&self) -> usize {
        match self {
            VideoFile::Cheese => 120,
            VideoFile::Grr => 120,
            VideoFile::Swim => 60,
            _ => 60,
        }
    }
}

#[derive(Debug)]
pub struct Strobe {
    // strobe_is_currently_on: bool,
    pub last_remaining_time_shown: u32,
    pub strobe_activate_time: Option<u32>,
    pub strobe_deactivate_time: Option<u32>,
    pub controls: StrobeControls,
    // If currently white or black.
    pub strobe_burst_state: bool,
    pub white_value: bool,
    pub white_value_enabled_time: u32,
}

#[derive(Debug)]
pub struct Mood {
    // Loops from 0 to 360.
    pub counter: u16,
    pub controls: MoodControls,
}

#[derive(Debug, Clone)]
pub struct MoodControls {
    pub on_beat: bool,
    pub force: bool,
    pub brightness: u8,
    pub animation: MoodAnimation,
    pub color_palette: MoodColorPalette,
}

#[derive(Debug, PartialEq, Clone)]
pub enum MoodColorPalette {
    All,
    CyanMagenta,
    OrangeBlue,
}

impl MoodColorPalette {
    pub fn to_hsv_ranges(&self) -> Vec<Range<usize>> {
        match self {
            Self::All => vec![0..361],
            Self::CyanMagenta => vec![180..345],
            Self::OrangeBlue => vec![240..241, 40..41],
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

#[derive(Debug)]
pub struct StrobeControls {
    pub on_beat: bool,
    pub strobe_enabled: bool,
    pub strobe_auto_enable: bool,
    pub brightness: u8,
    pub speed_multiplier: f32,
    pub strobe_animation: StrobeAnimation,

    pub strobe_drop_duration_secs: u32,

    // Speed controls.
    pub time_on_millis: u32,
    pub time_off_millis: u32,
    pub pan: u8,
    pub tilt: u8,

    pub tilt_animation_enabled: bool,
    pub tilt_animation_incr: bool,
    pub tilt_animation_speed: f32,
    pub tilt_animation_last_tick: u32,
}

impl StrobeControls {
    pub fn default_time_on_off() -> u32 {
        200
    }
}

#[derive(Debug, PartialEq, Clone)]
pub enum StrobeAnimation {
    Synced,
    Alternating(AnimationAlternating),
}

impl Default for StrobeAnimation {
    fn default() -> Self {
        Self::Synced
    }
}

impl StrobeAnimation {
    pub fn alternating() -> Self {
        Self::Alternating(AnimationAlternating::default())
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct AnimationAlternating {
    // Maps a light index to whether its enabled and the time of its state change.
    // Activation / Deactivation time.
    pub times: HashMap<usize, StrobeAnimationAlternatingState>,
    pub current_index: usize,
    pub last_change_time: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StrobeAnimationAlternatingState {
    pub enabled: bool,
    pub change_time: u32,
}

impl StrobeAnimationAlternatingState {
    pub fn enabled(time: u32) -> Self {
        Self {
            enabled: true,
            change_time: time,
        }
    }

    pub fn disabled(time: u32) -> Self {
        Self {
            enabled: false,
            change_time: time,
        }
    }
}

impl Default for AnimationAlternating {
    fn default() -> Self {
        Self {
            times: Default::default(),
            current_index: Default::default(),
            last_change_time: 0,
        }
    }
}

impl Default for State {
    fn default() -> Self {
        Self {
            was_initial: false,
            last_beat_time: 0,

            init_time: 0,

            faders: Fader {
                left_target: LeftFaderTarget::StrobeBrightness,
                right_target: RightFaderTarget::MoodBrightness,
            },

            animation: Animation {
                video: Video {
                    last_speed_update: 0,
                    speed: 1.0,
                    speed_bpm_sync: false,
                    speed_bpm_sync_last_factor: 1.0,
                    speed_sync_last_update: 0,
                    brightness_strobe_synced: false,
                    brightness: 100,
                    file: VideoFile::Cheese,
                    fry: 0,
                },
                strobe: Strobe {
                    // strobe_is_currently_on: false,
                    last_remaining_time_shown: 0,
                    strobe_activate_time: None,
                    strobe_deactivate_time: None,
                    strobe_burst_state: false,
                    white_value: false,
                    white_value_enabled_time: 0,
                    controls: StrobeControls {
                        on_beat: true,
                        strobe_enabled: false,
                        strobe_auto_enable: true,
                        brightness: 255,
                        speed_multiplier: 1.0,
                        strobe_drop_duration_secs: 5,
                        strobe_animation: StrobeAnimation::Synced,
                        time_on_millis: 50,
                        time_off_millis: 50,
                        pan: 128,
                        tilt: 128,
                        tilt_animation_enabled: false,
                        tilt_animation_incr: true,
                        tilt_animation_speed: 1.0,
                        tilt_animation_last_tick: 0,
                    },
                },
                mood: Mood {
                    counter: 0,
                    controls: MoodControls {
                        on_beat: true,
                        force: false,
                        brightness: 255,
                        animation: MoodAnimation::Synced,
                        color_palette: MoodColorPalette::All,
                    },
                },
            },
        }
    }
}

impl State {
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}
