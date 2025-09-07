use std::fmt::Display;

use blaulicht_shared::RGBColor;
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

impl Display for Light {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl Light {
    pub fn write(&self, this: &Fixture, state: &FixtureState, dmx: &mut [u8]) {
        let color: RGBColor = state.color.into();

        match self {
            Light::Generic3ChanNoAlpha => {
                let (r, g, b) = color.tup();
                let alpha = state.alpha;

                dmx[this.start_addr + 0] = (r as f32 / 255.0 * alpha as f32) as u8;
                dmx[this.start_addr + 1] = (g as f32 / 255.0 * alpha as f32) as u8;
                dmx[this.start_addr + 2] = (b as f32 / 255.0 * alpha as f32) as u8;
            }
            Light::Generic4ChanWithAlpha => {
                dmx[this.start_addr + 0] = state.alpha;
                dmx[this.start_addr + 1] = color.r;
                dmx[this.start_addr + 2] = color.g;
                dmx[this.start_addr + 3] = color.b;
            }
            Light::LEDPartyTCLSpot => {
                dmx[this.start_addr + 0] = color.r;
                dmx[this.start_addr + 1] = color.g;
                dmx[this.start_addr + 2] = color.b;
                dmx[this.start_addr + 3] = state.alpha;
            }
        }
    }

    pub fn blackout(&self, this: &Fixture, state: &FixtureState, dmx: &mut [u8]) {
        let color: RGBColor = state.color.into();

        match self {
            Light::Generic3ChanNoAlpha => {
                dmx[this.start_addr + 0] = 0;
                dmx[this.start_addr + 1] = 0;
                dmx[this.start_addr + 2] = 0;
            }
            Light::Generic4ChanWithAlpha => {
                dmx[this.start_addr + 0] = 0;
                dmx[this.start_addr + 1] = color.r;
                dmx[this.start_addr + 2] = color.g;
                dmx[this.start_addr + 3] = color.b;
            }
            Light::LEDPartyTCLSpot => {
                dmx[this.start_addr + 0] = color.r;
                dmx[this.start_addr + 1] = color.g;
                dmx[this.start_addr + 2] = color.b;
                dmx[this.start_addr + 3] = 0;
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
