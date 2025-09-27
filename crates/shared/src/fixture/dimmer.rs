use std::fmt::Display;

use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};
use strum::EnumIter;

use crate::fixture::state::FixtureState;

use super::Fixture;

#[derive(Serialize, Deserialize, Debug, Clone, Encode, Decode, EnumIter)]
pub enum Dimmer {
    FogMachineSingle,
    DimmerSingle,
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
        }
    }
    pub fn write(&self, this: &Fixture, state: &FixtureState, dmx: &mut [u8]) {
        match self {
            Dimmer::FogMachineSingle => dmx[this.start_addr + 0] = state.alpha,
            Dimmer::DimmerSingle => dmx[this.start_addr + 0] = state.alpha,
        }
    }
    pub fn blackout(&self, this: &Fixture, state: &FixtureState, dmx: &mut [u8]) {}
    pub fn setup(&self, this: &Fixture, time: i32, state: &FixtureState, dmx: &mut [u8]) {}
}
