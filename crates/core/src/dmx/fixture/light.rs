use serde::{Deserialize, Serialize};

use crate::dmx::clock::Time;

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
    pub fn write(&self, this: &Fixture, dmx: &mut [u8]) {
        match self {
            Light::Generic3ChanNoAlpha => {
                dmx[this.state.start_addr + 0] = this.state.color.r;
                dmx[this.state.start_addr + 1] = this.state.color.g;
                dmx[this.state.start_addr + 2] = this.state.color.b;
            }
            Light::Generic4ChanWithAlpha => {
                dmx[this.state.start_addr + 0] = this.state.alpha;
                dmx[this.state.start_addr + 1] = this.state.color.r;
                dmx[this.state.start_addr + 2] = this.state.color.g;
                dmx[this.state.start_addr + 3] = this.state.color.b;
            }
            Light::LEDPartyTCLSpot => {
                dmx[this.state.start_addr + 0] = this.state.color.r;
                dmx[this.state.start_addr + 1] = this.state.color.g;
                dmx[this.state.start_addr + 2] = this.state.color.b;
                dmx[this.state.start_addr + 3] = this.state.alpha;
            }
        }
    }

    pub fn blackout(&self, this: &Fixture, dmx: &mut [u8]) {
        match self {
            Light::Generic3ChanNoAlpha => {
                dmx[this.state.start_addr + 0] = 0;
                dmx[this.state.start_addr + 1] = 0;
                dmx[this.state.start_addr + 2] = 0;
            }
            Light::Generic4ChanWithAlpha => {
                dmx[this.state.start_addr + 0] = 0;
                dmx[this.state.start_addr + 1] = this.state.color.r;
                dmx[this.state.start_addr + 2] = this.state.color.g;
                dmx[this.state.start_addr + 3] = this.state.color.b;
            }
            Light::LEDPartyTCLSpot => {
                dmx[this.state.start_addr + 0] = this.state.color.r;
                dmx[this.state.start_addr + 1] = this.state.color.g;
                dmx[this.state.start_addr + 2] = this.state.color.b;
                dmx[this.state.start_addr + 3] = 0;
            }
        }
    }

    pub fn setup(&self, this: &Fixture, time: Time, dmx: &mut [u8]) {
        todo!("difficult")
        // match self {
        //     MovingHead::Generic3ChanNoAlpha => todo!(),
        //     MovingHead::Generic4ChanWithAlpha => todo!(),
        // }
    }
}
