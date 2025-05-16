use std::{collections::HashMap, ops::Range};

use super::config::Config;

#[derive(Debug)]
pub struct State {
    pub config: Config,
    pub fogger: bool,
    pub fogger_int: u8,

    pub was_initial: bool,
    pub init_time: u32,
    pub last_beat_time: u32,
    pub last_beat_time_mood: u32,

    pub faders: Fader,

    pub animation: Animation,

    pub crossfader_input: bool,

    pub logo_mode: LogoMode,
}

#[derive(Debug, PartialEq)]
pub enum LogoMode {
    Drop,
    Breakdown,
    Normal,
}

impl LogoMode {
    pub fn bytes(&self) -> &[u8] {
        match self {
            LogoMode::Drop => b"D1",
            LogoMode::Breakdown => b"D0",
            LogoMode::Normal => b"D2",
        }
    }
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
    pub dim: Dimmer,
}

#[derive(Debug, Clone, Copy)]
pub struct Video {
    // pub last_speed_update: u32,
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
    Hydra,
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
            VideoFile::Hydra => "HYDRA",
        }
    }

    pub fn speed(&self) -> f32 {
        match self {
            VideoFile::Cheese => 1.1,
            VideoFile::Grr => 4.0,
            VideoFile::Swim => 1.21,
            VideoFile::Hydra => 1.0,
            _ => 1.0,
        }
    }

    pub fn base_bpm(&self) -> usize {
        match self {
            VideoFile::Cheese => 120,
            VideoFile::Grr => 120,
            VideoFile::Swim => 60,
            VideoFile::Hydra => 60,
            _ => 60,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Dimmer {
    pub stage_real_brightness: u8,
    pub controls: DimmerControls,
}

#[derive(Debug, Clone, Copy)]
pub struct DimmerControls {
    pub brightness_stage: u8,
    pub brightness_other: u8,
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

    pub moving_head_group_enabled: bool,
    pub stage_light_group_enabled: bool,
    pub panel_group_enabled: bool,
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
            config: Config::default(),

            fogger: false,
            fogger_int: 127 / 2,
            logo_mode: LogoMode::Normal,
            crossfader_input: false,

            was_initial: false,
            last_beat_time: 0,
            last_beat_time_mood: 0,

            init_time: 0,

            faders: Fader {
                left_target: LeftFaderTarget::StrobeBrightness,
                right_target: RightFaderTarget::MoodBrightness,
            },

            animation: Animation {
                video: Video {
                    // last_speed_update: 0,
                    speed: 1.0,
                    speed_bpm_sync: true,
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
                        // groups
                        moving_head_group_enabled: true,
                        stage_light_group_enabled: true,
                        panel_group_enabled: true,
                    },
                },
                mood: Mood {
                    counter_range_index: 0,
                    counter_in_range_value: 0,
                    last_counter_update: 0,
                    animation_speed: 1.0,
                    animation_speed_beat: 1.0,
                    hsv: 0,
                    controls: MoodControls {
                        on_beat: true,
                        force: false,
                        brightness: 255,
                        animation: MoodAnimation::Synced,
                        color_palette: MoodColorPalette::All,
                        animation_on_beat: true,
                    },
                },
                dim: Dimmer {
                    stage_real_brightness: 255 / 2,
                    controls: DimmerControls {
                        brightness_stage: 255 / 2,
                        brightness_other: 255 / 2,
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
