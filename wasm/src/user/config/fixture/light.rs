use serde::Deserialize;

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
}

impl Light {
    pub fn write(&self, this: &Fixture, dmx: &mut [u8]) {
        match self {
            Light::Generic3ChanNoAlpha => {
                dmx[0] = this.color.r;
                dmx[1] = this.color.g;
                dmx[2] = this.color.b;
            }
            Light::Generic4ChanWithAlpha => {
                dmx[0] = this.alpha;
                dmx[1] = this.color.r;
                dmx[2] = this.color.g;
                dmx[3] = this.color.b;
            }
        }
    }

    pub fn blackout(&self, this: &Fixture, dmx: &mut [u8]) {
        match self {
            Light::Generic3ChanNoAlpha => {
                dmx[0] = 0;
                dmx[1] = 0;
                dmx[2] = 0;
            }
            Light::Generic4ChanWithAlpha => {
                dmx[0] = 0;
                dmx[1] = this.color.r;
                dmx[2] = this.color.g;
                dmx[3] = this.color.b;
            }
        }
    }
}
