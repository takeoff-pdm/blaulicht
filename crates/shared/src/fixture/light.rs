use crate::{RGBColor, fixture::state::FixtureState};

use super::Fixture;
use bincode::{Decode, Encode};
use map_range::MapRange;
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
    //
    // 0: Master Dimmer
    // 1: Strobe
    // 2: Macro
    // 3: Macro Speed
    // 4: Red
    // 5: Green
    // 6: Blue
    // 7: White
    //
    LightMaxxVegaSilentPar2Quad,
    //
    // 0: Red
    // 1: Green
    // 2: Blue
    // 3: Dimmer
    // 4: Strobe (value 011-255)
    LEDPar64RGBSpot5Chan,
    //
    // 0: Red
    // 1: Green
    // 2: Blue
    // 3: White
    //
    CameoQSpot40RGBW_4Chan,
    //
    // 0: Hue
    // 1: Rotation
    // 2: Strobe
    // 3: Alpha
    //
    LightMaxxTripleDerbyHP,
    //
    // 0: UV Dimmer
    // 1: UV Dimmer
    // 2: Strobe (0-250)
    // 3: Matrix (maps from hue)
    // 5: Laser (maps from hue)
    // 4: Speed
    // 6: Laser Strobe (06-250)
    // 7: Laser Rotation (maps from tilt)
    // 8: White SMDS
    // 9: Speed
    EuroLiteLEDMultiFX_10Chan,
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
            Light::LightMaxxVegaSilentPar2Quad => 8,
            Light::LEDPar64RGBSpot5Chan => 5,
            Light::CameoQSpot40RGBW_4Chan => 4,
            Light::LightMaxxTripleDerbyHP => 4,
            Light::EuroLiteLEDMultiFX_10Chan => 10,
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
                dmx[this.start_addr + 1] = state.strobe_speed; // strobe
                dmx[this.start_addr + 2] = 127; // warm white
                dmx[this.start_addr + 3] = 127; // cold white
            }
            Light::LightMaxxVegaSilentPar2Quad => {
                dmx[this.start_addr + 0] = state.alpha;
                dmx[this.start_addr + 1] = state.strobe_speed;
                dmx[this.start_addr + 2] = 0;
                dmx[this.start_addr + 3] = 0; // MACRO
                dmx[this.start_addr + 4] = color.r;
                dmx[this.start_addr + 5] = color.g;
                dmx[this.start_addr + 6] = color.b;
                // WHITE override.
                dmx[this.start_addr + 7] = if color.r == 255 && color.g == 255 && color.b == 255 {
                    255
                } else {
                    0
                };
            }
            Light::LEDPar64RGBSpot5Chan => {
                dmx[this.start_addr + 0] = color.r;
                dmx[this.start_addr + 1] = color.g;
                dmx[this.start_addr + 2] = color.b;
                dmx[this.start_addr + 3] = state.alpha;
                dmx[this.start_addr + 4] = match state.strobe_speed {
                    0 => 0,
                    v => v.map_range(0..255, 11..255),
                };
            }
            Light::CameoQSpot40RGBW_4Chan => {
                let (r, g, b) = color.tup();
                let alpha = state.alpha;

                dmx[this.start_addr + 0] = (r as f32 / 255.0 * alpha as f32) as u8;
                dmx[this.start_addr + 1] = (g as f32 / 255.0 * alpha as f32) as u8;
                dmx[this.start_addr + 2] = (b as f32 / 255.0 * alpha as f32) as u8;
                // WHITE override.
                dmx[this.start_addr + 3] = if color.r == 255 && color.g == 255 && color.b == 255 {
                    255
                } else {
                    0
                };
            }
            Light::LightMaxxTripleDerbyHP => {
                //
                // 0: Hue
                // 1: Rotation
                // 2: Strobe
                // 3: Alpha
                //
                dmx[this.start_addr + 0] = (state.color.h.map_range(0.0..360.0, 0.0..255.0)) as u8;
                dmx[this.start_addr + 1] = state.orientation.pan;
                dmx[this.start_addr + 2] = state.strobe_speed;
                dmx[this.start_addr + 3] = state.alpha;
            }
            Light::EuroLiteLEDMultiFX_10Chan => {
                let hue_u8 = (state.color.h.map_range(0.0..360.0, 0.0..255.0)) as u8;
                dmx[this.start_addr + 0] = state.alpha;
                dmx[this.start_addr + 1] = state.alpha;
                dmx[this.start_addr + 2] = state.strobe_speed;
                dmx[this.start_addr + 3] = hue_u8;
                dmx[this.start_addr + 4] = hue_u8;
                dmx[this.start_addr + 5] = state.orientation.pan;
                dmx[this.start_addr + 6] = state.strobe_speed;
                dmx[this.start_addr + 7] = state.orientation.tilt;
                dmx[this.start_addr + 8] = state.focus;
                dmx[this.start_addr + 9] = state.orientation.pan;
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
