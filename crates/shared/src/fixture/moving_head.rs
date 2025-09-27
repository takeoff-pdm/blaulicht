use crate::{RGBColor, fixture::state::FixtureState};

use super::Fixture;
use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};
use std::fmt::Display;
use strum::EnumIter;

#[derive(Serialize, Deserialize, Debug, Clone, EnumIter, Encode, Decode)]
pub enum MovingHead {
    MartinMac250E,
}

impl Display for MovingHead {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl MovingHead {
    pub fn footprint(&self) -> usize {
        match self {
            MovingHead::MartinMac250E => 18,
        }
    }

    pub fn write(&self, this: &Fixture, state: &FixtureState, dmx: &mut [u8]) {
        let color: RGBColor = state.color.into();

        match self {
            MovingHead::MartinMac250E => {
                // // shutter
                // if !value {
                //     dmx[start_addr + 0] = 0;
                // } else {
                //     dmx[start_addr + 0] = 50;
                // }
                //
                // dmx[start_addr + 1] = normal_bri;
                // dmx[start_addr + 2] = 0;

                // Strobe state.
                dmx[this.start_addr + 0] = state.strobe_speed as u8 * 255;

                // Alpha.
                dmx[this.start_addr + 1] = state.alpha;

                // Color.
                dmx[this.start_addr + 9] = color.r;
                dmx[this.start_addr + 10] = color.g;
                dmx[this.start_addr + 11] = color.b;

                // Position
                dmx[this.start_addr + 12] = state.orientation.pan;
                dmx[this.start_addr + 13] = 0;

                // tilt
                dmx[this.start_addr + 14] = state.orientation.tilt;
                dmx[this.start_addr + 15] = 0;
            }
        }
        // match self {
        //     MovingHead::Generic3ChanNoAlpha => todo!(),
        //     MovingHead::Generic4ChanWithAlpha => todo!(),
        // }
    }

    pub fn blackout(&self, this: &Fixture, state: &FixtureState, dmx: &mut [u8]) {
        // match self {
        //     MovingHead::Generic3ChanNoAlpha => todo!(),
        //     MovingHead::Generic4ChanWithAlpha => todo!(),
        // }
    }

    pub fn setup(&self, this: &Fixture, time: i32, state: &FixtureState, dmx: &mut [u8]) {
        match self {
            MovingHead::MartinMac250E => {
                match time {
                    v if v <= 5000 => {
                        // Enable lamp.
                        dmx[this.start_addr + 0] = 237;
                    }
                    v => {
                        dmx[this.start_addr + 0] = 20;
                    }
                }
            }
        }
        // TODO: just call write.
        // self.write(this, dmx);
        // match self {
        //     MovingHead::Generic3ChanNoAlpha => todo!(),
        //     MovingHead::Generic4ChanWithAlpha => todo!(),
        // }
    }
}
