use std::{
    cmp,
    collections::VecDeque,
    fmt::Display,
    iter::Sum,
    ops::{Add, Div, Sub},
};

use crate::blaulicht::TickInput;
use crate::println;

use super::GLOBAL_TIME;

type TIME_INNER = i32;

#[derive(PartialEq, Eq, Debug, Default, Clone, Copy)]
pub struct Time(TIME_INNER);

impl From<TIME_INNER> for Time {
    fn from(value: TIME_INNER) -> Self {
        Self(value)
    }
}

impl Sub<Time> for Time {
    type Output = Time;

    fn sub(self, rhs: Time) -> Self::Output {
        Self(self.0 - rhs.0)
    }
}

impl Add<Time> for Time {
    type Output = Time;

    fn add(self, rhs: Time) -> Self::Output {
        Self(self.0 + rhs.0)
    }
}

impl Div<Time> for Time {
    type Output = Time;

    fn div(self, rhs: Time) -> Self::Output {
        Self(self.inner() / rhs.inner())
    }
}

impl<'t> Sum<&'t Time> for Time {
    fn sum<I: Iterator<Item = &'t Time>>(iter: I) -> Self {
        Self::new(iter.map(|t| t.inner()).sum())
    }
}

impl PartialOrd<Time> for Time {
    fn partial_cmp(&self, other: &Time) -> Option<std::cmp::Ordering> {
        Some(self.0.cmp(&other.0))
    }
}

impl PartialEq<TIME_INNER> for Time {
    fn eq(&self, other: &TIME_INNER) -> bool {
        self.0 == *other
    }
}

impl PartialOrd<TIME_INNER> for Time {
    fn partial_cmp(&self, other: &TIME_INNER) -> Option<std::cmp::Ordering> {
        Some(self.0.cmp(&other))
    }
}

impl Display for Time {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (unit, val) = match self {
            v if v.0 > 60 * 1000 => ("m", self.0 / (60 * 1000)),
            v if v.0 > 1000 => ("s", self.0 / 1000),
            _ => ("ms", self.0),
        };

        write!(f, "{val}{unit}")
    }
}

impl Time {
    pub const fn new(value: TIME_INNER) -> Self {
        Self(value)
    }

    pub fn now() -> Self {
        unsafe { GLOBAL_TIME }
    }

    pub fn elapsed(&self) -> Self {
        Self::now() - *self
    }

    pub fn inner(&self) -> TIME_INNER {
        self.0
    }
}

#[derive(Debug)]
pub struct BeatClock {
    pub last_beat_tick_time: Time,
    pub target_time_between_ticks: Time,
    // Is set to true
    current_activation: Option<BeatClockActivation>,
    avg_drift: VecDeque<Time>,
}

#[derive(Debug)]
pub struct BeatClockActivation {
    overdue_delta: Time,
    avg_drift: Time,
}

const CLOCK_DRIFT_HISTORY_LEN: usize = 10;

impl BeatClock {
    pub fn new() -> Self {
        Self {
            last_beat_tick_time: Time::default(),
            target_time_between_ticks: Time::default(),
            current_activation: None,
            avg_drift: VecDeque::with_capacity(CLOCK_DRIFT_HISTORY_LEN),
        }
    }

    fn internal_speed_update(&mut self, input: TickInput) {
        if input.bpm == 0 {
            self.avg_drift.clear();
            return;
        }

        let target_time_between_ticks_avg = (input.time_between_beats_millis as TIME_INNER
            + self.target_time_between_ticks.inner())
            / 2;
        self.target_time_between_ticks = Time::new(target_time_between_ticks_avg);
    }

    pub fn tick(&mut self, input: TickInput) {
        self.internal_speed_update(input);

        //
        // Activation logic.
        //

        let mut elapsed_delta = self.last_beat_tick_time.elapsed();

        if (self.current_activation.is_some()) || (self.target_time_between_ticks == 0) {
            return;
        }

        // Attempt to fix clock drift.
        // Apply overdue_delta on the elapsed_delta to compensate drift.
        let mut avg_drift = if !self.avg_drift.is_empty() {
            (self.avg_drift.iter().sum::<Time>().inner() / self.avg_drift.len() as TIME_INNER)
                .into()
        } else {
            Time::new(0)
        };

        // Ignore huge drift.
        if avg_drift > 1000 {
            avg_drift = Time::new(0);
        }

        let new_elapsed_delta = if avg_drift > 20 {
            elapsed_delta - avg_drift
        } else {
            elapsed_delta + avg_drift
        };

        elapsed_delta = new_elapsed_delta;

        if elapsed_delta < self.target_time_between_ticks {
            return;
        }

        let overdue_delta = elapsed_delta - self.target_time_between_ticks;
        self.last_beat_tick_time = Time::now();

        // Update avg. drift.
        if self.avg_drift.len() >= CLOCK_DRIFT_HISTORY_LEN {
            self.avg_drift.pop_front();
        }
        self.avg_drift.push_back(overdue_delta);

        // Set current activation.
        self.current_activation = Some(BeatClockActivation {
            overdue_delta,
            avg_drift,
        });
    }

    //
    // Also returns a bool whether the callback was activated.
    //
    pub fn activate(&mut self, callback: fn(activation: BeatClockActivation)) -> bool {
        let activation = self.current_activation.take();
        if let Some(a) = activation {
            callback(a);
            true
        } else {
            false
        }
    }
}
