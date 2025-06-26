use crate::blaulicht::TickInput;

use super::{clock::Time, println, state::State};

#[derive(Debug)]
pub enum FilterSensitivity {
    High,
    Mid,
    Low,
}

//
// Beat filter.
//

#[derive(Debug)]
pub struct BeatFilter {
    pub sensitivity: FilterSensitivity,
    open: bool,
    first_time_open: (bool, Time), // State and when it was changed.
}

impl BeatFilter {
    pub fn new(sensitivity: FilterSensitivity) -> Self {
        Self {
            sensitivity,
            open: false,
            first_time_open: (false, Time::now()),
        }
    }

    pub fn tick(&mut self, input: TickInput) {
        let open = match self.sensitivity {
            FilterSensitivity::High => {
                (input.bass_avg >= 70)
                    || (input.bass_avg_short == 200)
                    || (input.bass >= 70 && input.bass_avg <= 50)
                    || (input.bass_avg >= 70)
            }
            FilterSensitivity::Mid => {
                (input.bass_avg >= 80)
                    || (input.bass_avg_short == 255)
                    || (input.bass >= 80 && input.bass_avg <= 50)
                    || (input.bass_avg >= 80)
            }
            FilterSensitivity::Low => {
                (input.bass_avg >= 90)
                    || (input.bass_avg_short == 255)
                    || (input.bass >= 90 && input.bass_avg <= 60)
                    || (input.bass_avg >= 90)
            }
        };

        if open {
            if !self.open {
                // Cooldown for drop activations.
                if self.first_time_open.1.elapsed() > 1000 {
                    self.first_time_open.0 = true
                } else {
                    println!("COOLDOWN");
                }
            }

            self.first_time_open.1 = Time::now();
        }

        self.open = open;
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn is_open_first_time(&mut self) -> bool {
        let return_val = self.first_time_open.0;

        if return_val {
            self.first_time_open.0 = false;
        }

        return_val
    }
}

//
// Drop filter.
//

#[derive(Clone, Copy, Debug)]
pub enum DropState {
    None,
    Begin,
    Main,
}

impl DropState {
    // TODO: make this configurable.
    pub fn max_duration(&self) -> Time {
        match self {
            DropState::None => Time::new(0),
            DropState::Begin => Time::new(2000),
            DropState::Main => Time::new(0),
        }
    }
}

#[derive(Debug)]
pub struct DropFilter {
    // pub sensitivity: FilterSensitivity,
    pub drop_start_time: Option<Time>,
    pub state: DropState,
}

impl DropFilter {
    pub fn new() -> Self {
        Self {
            drop_start_time: None,
            state: DropState::None,
        }
    }

    pub fn beat_filter_in(&mut self, on_beat: bool) {
        match on_beat {
            true => self.tick_beat_true(),
            false => self.tick_beat_false(),
        }
    }

    fn tick_beat_true(&mut self) {
        match (self.drop_start_time, self.state) {
            (None, _) => {
                self.state = DropState::Begin;
                self.drop_start_time = Some(Time::now());
            }
            (Some(_), DropState::None) => unreachable!(),
            (Some(t), DropState::Begin) => {
                if t.elapsed() > self.state.max_duration() {
                    self.state = DropState::Main
                }
            }
            (Some(_), DropState::Main) => {}
        }
    }

    fn tick_beat_false(&mut self) {
        if self.drop_start_time.is_some() {
            self.drop_start_time = None;
            self.state = DropState::None;
        }
    }
}
