use std::collections::HashMap;

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