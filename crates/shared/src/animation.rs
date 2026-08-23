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
        self.timer = self.timer.saturating_add(1);
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
    pub iteration_count: u32,
    pub reversed: bool,
}

impl ActiveAnimation {
    pub fn reset(&mut self) {
        self.reset_timers(SyncMode::Synced);
    }

    pub fn set_timers(&mut self, animation_sync: SyncMode) {
        self.enabled = true;
        self.reset_timers(animation_sync);
    }

    /// Resets timer timestamps and assigns the initial phase for each fixture.
    /// Unlike [`Self::set_timers`], this preserves whether the animation is enabled.
    pub fn reset_timers(&mut self, animation_sync: SyncMode) {
        let amount = self.fixture_timers.len();

        // TIMING mode: spread or sync the timing.
        for (counter, timer_state) in self.fixture_timers.values_mut().enumerate() {
            timer_state.last_tick_time = 0;
            // Beat-pinned animations defer the actual reset to the next beat
            // event (the animation tick clears this immediately for all
            // others).
            timer_state.needs_reset_on_beat = true;
            timer_state.timer = match animation_sync {
                SyncMode::Synced => 0,
                SyncMode::StretchedEven if amount > 0 => {
                    ((360.0 / amount as f32) * counter as f32) as u64
                }
                SyncMode::StretchedEven => 0,
                SyncMode::StretchedHalfHalf => (180 * (counter % 2)) as u64,
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reset_timers_assigns_phases_without_enabling_animation() {
        let mut animation = ActiveAnimation::new(&[(1, 1), (1, 2), (1, 3)], AnimationSpec::empty());

        animation.reset_timers(SyncMode::StretchedEven);

        assert!(!animation.enabled);
        assert_eq!(animation.fixture_timers[&(1, 1)].timer, 0);
        assert_eq!(animation.fixture_timers[&(1, 2)].timer, 120);
        assert_eq!(animation.fixture_timers[&(1, 3)].timer, 240);
    }

    #[test]
    fn set_timers_enables_and_resets_animation() {
        let mut animation = ActiveAnimation::new(&[(1, 1)], AnimationSpec::empty());
        animation.fixture_timers.get_mut(&(1, 1)).unwrap().timer = 42;

        animation.set_timers(SyncMode::Synced);

        assert!(animation.enabled);
        assert_eq!(animation.fixture_timers[&(1, 1)].timer, 0);
        assert_eq!(animation.fixture_timers[&(1, 1)].last_tick_time, 0);
        assert!(animation.fixture_timers[&(1, 1)].needs_reset_on_beat);
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
            iteration_count: 0,
            reversed: false,
        }
    }
}
