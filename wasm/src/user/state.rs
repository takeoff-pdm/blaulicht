#[derive(Debug)]
pub struct State {
    pub was_initial: bool,
    pub last_beat_time: u32,
    pub animation: Animation,
}

#[derive(Debug)]
pub struct Animation {
    pub strobe: Strobe,
    pub mood: Mood,
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
}

#[derive(Debug)]
pub struct Mood {
    // Loops from 0 to 360.
    pub counter: u16,
    pub controls: MoodControls,
}

#[derive(Debug)]
pub struct MoodControls {
    pub on_beat: bool,
    pub force: bool,
    pub brightness: u8,
}

#[derive(Debug)]
pub struct StrobeControls {
    pub on_beat: bool,
    pub strobe_enabled: bool,
    pub strobe_auto_enable: bool,
}

impl Default for State {
    fn default() -> Self {
        Self {
            was_initial: false,
            last_beat_time: 0,

            animation: Animation {
                strobe: Strobe {
                    // strobe_is_currently_on: false,
                    last_remaining_time_shown: 0,
                    strobe_activate_time: None,
                    strobe_deactivate_time: None,
                    strobe_burst_state: false,
                    controls: StrobeControls {
                        on_beat: true,
                        strobe_enabled: false,
                        strobe_auto_enable: true,
                    },
                },
                mood: Mood {
                    counter: 0,
                    controls: MoodControls {
                        on_beat: true,
                        force: false,
                        brightness: 255,
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
