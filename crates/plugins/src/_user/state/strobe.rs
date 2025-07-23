use std::collections::HashMap;

use serde::Deserialize;

use super::animation::AnimationAlternating;

//
// Global strobe timing / trigger.
//

#[derive(Debug)]
pub struct StrobeState {
    pub last_remaining_time_shown: u32,
    pub strobe_activate_time: Option<u32>,
    pub strobe_deactivate_time: Option<u32>,
}

//
// Group state.
//

#[derive(Debug, Default, Deserialize)]
pub struct StrobeGroupState {
    #[serde(skip)]
    pub controls: StrobeControls,
    // If currently white or black.
    pub strobe_is_white_state: bool,
    pub white_value: bool,
    pub white_value_enabled_time: u32,
}

#[derive(Debug, Default)]
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
