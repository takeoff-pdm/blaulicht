use serde::{Deserialize, Serialize};

use crate::dmx::{clock::Time, FixtureState};

use super::Fixture;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Light {
    //
    // 0: Red
    // 1: Green
    // 2: Blue
    //
    Generic3ChanNoAlpha,
    //
    // 0: Alpha
    // 1: Red
    // 2: Green
    // 3: Blue
    //
    Generic4ChanWithAlpha,
    //
    // 0: Red
    // 1: Green
    // 2: Blue
    // 4: Alpha
    // 5: Strobe
    //
    LEDPartyTCLSpot,
}

impl Light {
    pub fn write(&self, this: &Fixture, state: &FixtureState, dmx: &mut [u8]) {
        match self {
            Light::Generic3ChanNoAlpha => {
                let (r, g, b) = state.color.tup();
                let alpha = state.alpha;

                dmx[state.start_addr + 0] = (r as f32 / 255.0 * alpha as f32) as u8;
                dmx[state.start_addr + 1] = (g as f32 / 255.0 * alpha as f32) as u8;
                dmx[state.start_addr + 2] = (b as f32 / 255.0 * alpha as f32) as u8;
            }
            Light::Generic4ChanWithAlpha => {
                dmx[state.start_addr + 0] = state.alpha;
                dmx[state.start_addr + 1] = state.color.r;
                dmx[state.start_addr + 2] = state.color.g;
                dmx[state.start_addr + 3] = state.color.b;
            }
            Light::LEDPartyTCLSpot => {
                dmx[state.start_addr + 0] = state.color.r;
                dmx[state.start_addr + 1] = state.color.g;
                dmx[state.start_addr + 2] = state.color.b;
                dmx[state.start_addr + 3] = state.alpha;
            }
        }
    }

    pub fn blackout(&self, this: &Fixture, state: &FixtureState, dmx: &mut [u8]) {
        match self {
            Light::Generic3ChanNoAlpha => {
                dmx[state.start_addr + 0] = 0;
                dmx[state.start_addr + 1] = 0;
                dmx[state.start_addr + 2] = 0;
            }
            Light::Generic4ChanWithAlpha => {
                dmx[state.start_addr + 0] = 0;
                dmx[state.start_addr + 1] = state.color.r;
                dmx[state.start_addr + 2] = state.color.g;
                dmx[state.start_addr + 3] = state.color.b;
            }
            Light::LEDPartyTCLSpot => {
                dmx[state.start_addr + 0] = state.color.r;
                dmx[state.start_addr + 1] = state.color.g;
                dmx[state.start_addr + 2] = state.color.b;
                dmx[state.start_addr + 3] = 0;
            }
        }
    }

    pub fn setup(&self, this: &Fixture, time: Time, state: &FixtureState, dmx: &mut [u8]) {
        todo!("difficult")
        // match self {
        //     MovingHead::Generic3ChanNoAlpha => todo!(),
        //     MovingHead::Generic4ChanWithAlpha => todo!(),
        // }
    }
}
