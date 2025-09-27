use crate::{RGBColor, fixture::state::FixtureState};

use super::Fixture;
use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};
use std::fmt::Display;
use strum::EnumIter;

#[derive(Serialize, Deserialize, Debug, Clone, EnumIter, Encode, Decode)]
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
    //
    // 0: Red
    // 1: Green
    // 2: Blue
    // 4: UNKNOWN
    // 5: UNKNOWN
    // 6: Alpha
    //
    AdjMegaHexPar,
    //
    // 0: Red
    // 1: Green
    // 2: Blue
    // 4: UNKNOWN
    // 5: UNKNOWN
    // 6: UNKNOWN
    // 7: Alpha
    //
    LiteCraftMiniParAT10,
    //
    // 0: Alpha
    // 1: Strobe (0..30)
    // 2: Warm White
    // 2: Cold White
    //
    VaryTechVP1,
}

impl Display for Light {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl Light {
    pub fn footprint(&self) -> usize {
        match self {
            Light::Generic3ChanNoAlpha => 3,
            Light::Generic4ChanWithAlpha => 4,
            Light::LEDPartyTCLSpot => 6,
            Light::AdjMegaHexPar => 7,
            Light::LiteCraftMiniParAT10 => 8,
            Light::VaryTechVP1 => 4,
        }
    }

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
            Light::AdjMegaHexPar => {
                dmx[this.start_addr + 0] = color.r;
                dmx[this.start_addr + 1] = color.g;
                dmx[this.start_addr + 2] = color.b;
                dmx[this.start_addr + 6] = state.alpha;
            }
            Light::LiteCraftMiniParAT10 => {
                dmx[this.start_addr + 0] = color.r;
                dmx[this.start_addr + 1] = color.g;
                dmx[this.start_addr + 2] = color.b;
                dmx[this.start_addr + 7] = state.alpha;
            }
            Light::VaryTechVP1 => {
                dmx[this.start_addr + 0] = state.alpha;
                dmx[this.start_addr + 1] = 0; // strobe
                dmx[this.start_addr + 2] = 127; // warm white
                dmx[this.start_addr + 3] = 127; // cold white
            }
        }
    }

    pub fn blackout(&self, this: &Fixture, state: &FixtureState, dmx: &mut [u8]) {
        // let color: RGBColor = state.color.into();
        //
        // match self {
        //     Light::Generic3ChanNoAlpha => {
        //         dmx[this.start_addr + 0] = 0;
        //         dmx[this.start_addr + 1] = 0;
        //         dmx[this.start_addr + 2] = 0;
        //     }
        //     Light::Generic4ChanWithAlpha => {
        //         dmx[this.start_addr + 0] = 0;
        //         dmx[this.start_addr + 1] = color.r;
        //         dmx[this.start_addr + 2] = color.g;
        //         dmx[this.start_addr + 3] = color.b;
        //     }
        //     Light::LEDPartyTCLSpot => {
        //         dmx[this.start_addr + 0] = color.r;
        //         dmx[this.start_addr + 1] = color.g;
        //         dmx[this.start_addr + 2] = color.b;
        //         dmx[this.start_addr + 3] = 0;
        //     }
        // }
    }

    pub fn setup(&self, this: &Fixture, time: i32, state: &FixtureState, dmx: &mut [u8]) {
        // match self {
        //     MovingHead::Generic3ChanNoAlpha => todo!(),
        //     MovingHead::Generic4ChanWithAlpha => todo!(),
        // }
    }
}
