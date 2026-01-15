use std::fmt::Display;

use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};
use strum::EnumIter;

use crate::fixture::state::FixtureState;

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

    pub fn write(&self, this: &Fixture, state: &FixtureState, dmx: &mut [u8]) {
        match self {
            Dimmer::FogMachineSingle => dmx[this.start_addr + 0] = state.alpha,
            Dimmer::DimmerSingle => dmx[this.start_addr + 0] = state.alpha,
            Dimmer::DimmerWStrobe => {
                dmx[this.start_addr + 0] = state.alpha;
                dmx[this.start_addr + 1] = state.strobe_speed;
            }
        }
    }

    pub fn state_from_dmx(&self, this: &Fixture, dmx: &[u8]) -> FixtureState {
        let mut state = FixtureState {
            alpha: dmx[this.start_addr + 0],
            ..Default::default()
        };

        match self {
            Dimmer::FogMachineSingle | Dimmer::DimmerSingle => {}
            Dimmer::DimmerWStrobe => {
                state.strobe_speed = dmx[this.start_addr + 1];
            }
        }

        state
    }

    pub fn blackout(&self, this: &Fixture, state: &FixtureState, dmx: &mut [u8]) {}
    pub fn setup(&self, this: &Fixture, time: i32, state: &FixtureState, dmx: &mut [u8]) {}
}
