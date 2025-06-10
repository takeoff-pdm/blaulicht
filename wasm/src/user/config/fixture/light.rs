use serde::Deserialize;

use crate::user::clock::Time;

use super::Fixture;

#[derive(Deserialize, Debug)]
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
    pub fn write(&self, this: &Fixture, dmx: &mut [u8]) {
        match self {
            Light::Generic3ChanNoAlpha => {
                dmx[this.start_channel + 0] = this.color.r;
                dmx[this.start_channel + 1] = this.color.g;
                dmx[this.start_channel + 2] = this.color.b;
            }
            Light::Generic4ChanWithAlpha => {
                dmx[this.start_channel + 0] = this.alpha;
                dmx[this.start_channel + 1] = this.color.r;
                dmx[this.start_channel + 2] = this.color.g;
                dmx[this.start_channel + 3] = this.color.b;
            }
            Light::LEDPartyTCLSpot => {
                dmx[this.start_channel + 0] = this.color.r;
                dmx[this.start_channel + 1] = this.color.g;
                dmx[this.start_channel + 2] = this.color.b;
                dmx[this.start_channel + 3] = this.alpha;
            }
        }
    }

    pub fn blackout(&self, this: &Fixture, dmx: &mut [u8]) {
        match self {
            Light::Generic3ChanNoAlpha => {
                dmx[this.start_channel + 0] = 0;
                dmx[this.start_channel + 1] = 0;
                dmx[this.start_channel + 2] = 0;
            }
            Light::Generic4ChanWithAlpha => {
                dmx[this.start_channel + 0] = 0;
                dmx[this.start_channel + 1] = this.color.r;
                dmx[this.start_channel + 2] = this.color.g;
                dmx[this.start_channel + 3] = this.color.b;
            }
            Light::LEDPartyTCLSpot => {
                dmx[this.start_channel + 0] = this.color.r;
                dmx[this.start_channel + 1] = this.color.g;
                dmx[this.start_channel + 2] = this.color.b;
                dmx[this.start_channel + 3] = 0;
            }
        }
    }

    pub fn setup(&self, this: &Fixture, time: Time, dmx: &mut [u8]) {
        // match self {
        //     MovingHead::Generic3ChanNoAlpha => todo!(),
        //     MovingHead::Generic4ChanWithAlpha => todo!(),
        // }
    }
}
