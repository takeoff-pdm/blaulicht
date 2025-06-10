use logo::LogoMode;

use super::{
    beat::{BeatFilter, DropFilter, FilterSensitivity},
    clock::BeatClock,
    config::Config,
};

pub mod animation;
pub mod dimmer;
pub mod logo;
pub mod mood;
pub mod strobe;
pub mod video;

#[derive(Debug)]
pub struct State {
    pub config: Config,
    // pub fogger: bool,
    // pub fogger_int: u8,
    pub was_initial: bool,
    // pub init_time: u32,
    // pub last_beat_time: u32,
    // pub last_beat_time_mood: u32,

    // pub faders: Fader,

    // pub animation: Animation,
    // pub crossfader_input: bool,

    // pub logo_mode: LogoMode,

    // TODO: utilize multiple clocks instead.
    pub clock: BeatClock,
    pub beat_filter: BeatFilter,
    pub drop_filter: DropFilter,
}

impl Default for State {
    fn default() -> Self {
        Self {
            config: Config::default(),

            // fogger: false,
            // fogger_int: 127 / 2,
            // logo_mode: LogoMode::Normal,
            // crossfader_input: false,
            was_initial: false,
            // last_beat_time: 0,
            // last_beat_time_mood: 0,

            // init_time: 0,
            // faders: Fader {
            //     left_target: LeftFaderTarget::StrobeBrightness,
            //     right_target: RightFaderTarget::MoodBrightness,
            // },

            // animation: Animation {
            //     video: Video {
            //         // last_speed_update: 0,
            //         speed: 1.0,
            //         speed_bpm_sync: true,
            //         speed_bpm_sync_last_factor: 1.0,
            //         speed_sync_last_update: 0,
            //         brightness_strobe_synced: false,
            //         brightness: 100,
            //         file: VideoFile::Cheese,
            //         fry: 0,
            //     },
            //     strobe: Strobe {
            //         // strobe_is_currently_on: false,
            //         last_remaining_time_shown: 0,
            //         strobe_activate_time: None,
            //         strobe_deactivate_time: None,
            //         strobe_burst_state: false,
            //         white_value: false,
            //         white_value_enabled_time: 0,
            //         controls: StrobeControls {
            //             on_beat: true,
            //             strobe_enabled: false,
            //             strobe_auto_enable: true,
            //             brightness: 255,
            //             speed_multiplier: 1.0,
            //             strobe_drop_duration_secs: 5,
            //             strobe_animation: StrobeAnimation::Synced,
            //             time_on_millis: 50,
            //             time_off_millis: 50,
            //             pan: 128,
            //             tilt: 128,
            //             tilt_animation_enabled: false,
            //             tilt_animation_incr: true,
            //             tilt_animation_speed: 1.0,
            //             tilt_animation_last_tick: 0,
            //             // groups
            //             moving_head_group_enabled: true,
            //             stage_light_group_enabled: true,
            //             panel_group_enabled: true,
            //         },
            //     },
            //     mood: Mood {
            //         counter_range_index: 0,
            //         counter_in_range_value: 0,
            //         last_counter_update: 0,
            //         animation_speed: 1.0,
            //         animation_speed_beat: 1.0,
            //         hsv: 0,
            //         controls: MoodControls {
            //             on_beat: true,
            //             force: false,
            //             brightness: 255,
            //             animation: MoodAnimation::Synced,
            //             color_palette: MoodColorPalette::All,
            //             animation_on_beat: true,
            //         },
            //     },
            //     dim: Dimmer {
            //         stage_real_brightness: 255 / 2,
            //         controls: DimmerControls {
            //             brightness_stage: 255 / 2,
            //             brightness_other: 255 / 2,
            //         },
            //     },
            // },
            clock: BeatClock::new(),
            beat_filter: BeatFilter::new(FilterSensitivity::Mid),
            drop_filter: DropFilter::new(),
        }
    }
}

impl State {
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}
