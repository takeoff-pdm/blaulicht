// Animations.
//

use std::{collections::BTreeMap, fmt::Display};

use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};
use strum::EnumIter;

use crate::{AnimationSpec, AnimationSpeedModifier};

#[derive(Serialize, Deserialize, Debug, Clone, Default, Encode, Decode)]
pub struct AnimationTimerState {
    pub last_tick_time: u64,
    pub timer: u64, // Counts up continously
    pub needs_reset_on_beat: bool,
}

impl AnimationTimerState {
    // Advances the timer variable
    pub fn tick(&mut self, now: u64) {
        self.timer += 1;
        self.last_tick_time = now;
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Encode, Decode)]
pub struct ActiveAnimation {
    // pub animation_id: u8,
    // 1/16 | 1/8 | 1/4 | 1/2 | 1 | 2 | 4 | 8
    pub speed_factor: AnimationSpeedModifier,
    // TODO: override parameters
    pub enabled: bool,
    // pub selection: EngineSelection,
    pub fixture_timers: BTreeMap<(u8, u8), AnimationTimerState>,
    // CLONE of the spec.
    pub spec_cloned: AnimationSpec,
}

impl ActiveAnimation {
    pub fn reset(&mut self) {
        for t in self.fixture_timers.iter_mut() {
            *t.1 = AnimationTimerState::default()
        }
    }

    pub fn set_timers(&mut self, animation_sync: SyncMode) {
        self.enabled = true;

        // TIMING mode: spread or sync the timing.
        let amount = self.fixture_timers.len();
        match animation_sync {
            SyncMode::Synced => {
                for (counter, (_, timer_state)) in self.fixture_timers.iter_mut().enumerate() {
                    timer_state.last_tick_time = 0;
                    timer_state.timer = 0;
                }
            }
            SyncMode::StretchedEven => {
                for (counter, (_, timer_state)) in self.fixture_timers.iter_mut().enumerate() {
                    timer_state.last_tick_time = 0;
                    timer_state.timer = ((360.0 / amount as f32) * counter as f32) as u64;

                    println!("timer: {}", timer_state.timer);
                }
            }
            SyncMode::StretchedHalfHalf => {
                for (counter, (_, timer_state)) in self.fixture_timers.iter_mut().enumerate() {
                    timer_state.last_tick_time = 0;
                    timer_state.timer = (180 * (counter % 2)) as u64;
                }
            }
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, Encode, Decode, PartialEq, Eq, EnumIter)]
pub enum SyncMode {
    Synced,
    StretchedEven,
    StretchedHalfHalf,
}

impl Default for SyncMode {
    fn default() -> Self {
        Self::Synced
    }
}

impl Display for SyncMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                SyncMode::Synced => "Synced",
                SyncMode::StretchedEven => "SyncedEven",
                SyncMode::StretchedHalfHalf => "SyncedHalfHalf",
            }
        )
    }
}

impl ActiveAnimation {
    pub fn new(fixtures: &[(u8, u8)], spec_cloned: AnimationSpec) -> Self {
        let mut fixture_timers = BTreeMap::new();

        for key in fixtures {
            fixture_timers.insert(*key, AnimationTimerState::default());
        }

        Self {
            speed_factor: AnimationSpeedModifier::_1,
            enabled: false,
            fixture_timers,
            spec_cloned,
        }
    }
}
