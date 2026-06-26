use std::fmt::Display;

use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};
use strum::EnumIter;

use crate::fixture::state::{FixtureState, ResolvedFixtureState};

use super::Fixture;

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, EnumIter)]
pub enum Dimmer {
    FogMachineSingle,
    DimmerSingle,
    DimmerWStrobe,
}

impl Display for Dimmer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl Dimmer {
    pub fn footprint(&self) -> usize {
        match self {
            Dimmer::FogMachineSingle => 1,
            Dimmer::DimmerSingle => 1,
            Dimmer::DimmerWStrobe => 2,
        }
    }

    pub fn write(&self, this: &Fixture, state: &ResolvedFixtureState, dmx: &mut [u8]) {
        match self {
            Dimmer::FogMachineSingle => fixture_channel!(dmx, this, 0) = state.alpha,
            Dimmer::DimmerSingle => fixture_channel!(dmx, this, 0) = state.alpha,
            Dimmer::DimmerWStrobe => {
                fixture_channel!(dmx, this, 0) = state.alpha;
                fixture_channel!(dmx, this, 1) = state.strobe_speed;
            }
        }
    }

    pub fn state_from_dmx(&self, this: &Fixture, dmx: &[u8]) -> FixtureState {
        let mut state = ResolvedFixtureState {
            alpha: fixture_channel!(dmx, this, 0),
            ..Default::default()
        };

        match self {
            Dimmer::FogMachineSingle | Dimmer::DimmerSingle => {}
            Dimmer::DimmerWStrobe => {
                state.strobe_speed = fixture_channel!(dmx, this, 1);
            }
        }

        state.into()
    }

    pub fn blackout(&self, _this: &Fixture, _state: &ResolvedFixtureState, _dmx: &mut [u8]) {}
    pub fn setup(&self, _this: &Fixture, _time: i32, _state: &ResolvedFixtureState, _dmx: &mut [u8]) {}
}
