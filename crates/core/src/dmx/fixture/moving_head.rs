use serde::{Deserialize, Serialize};

use crate::dmx::clock::Time;

use super::Fixture;

#[derive(Serialize, Deserialize, Debug)]
pub enum MovingHead {
    MartinMacAura,
}

impl MovingHead {
    pub fn write(&self, this: &Fixture, dmx: &mut [u8]) {
        match self {
            MovingHead::MartinMacAura => {
                // Strobe state.
                dmx[this.state.start_addr + 0] = this.state.strobe_speed as u8 * 255;

                // Alpha.
                dmx[this.state.start_addr + 1] = this.state.alpha;

                // Color.
                dmx[this.state.start_addr + 9] = this.state.color.r;
                dmx[this.state.start_addr + 10] = this.state.color.g;
                dmx[this.state.start_addr + 11] = this.state.color.b;
            }
        }
        // match self {
        //     MovingHead::Generic3ChanNoAlpha => todo!(),
        //     MovingHead::Generic4ChanWithAlpha => todo!(),
        // }
    }

    pub fn blackout(&self, this: &Fixture, dmx: &mut [u8]) {
        // match self {
        //     MovingHead::Generic3ChanNoAlpha => todo!(),
        //     MovingHead::Generic4ChanWithAlpha => todo!(),
        // }
    }

    pub fn setup(&self, this: &Fixture, time: Time, dmx: &mut [u8]) {
        // TODO: just call write.
        self.write(this, dmx);
        // match self {
        //     MovingHead::Generic3ChanNoAlpha => todo!(),
        //     MovingHead::Generic4ChanWithAlpha => todo!(),
        // }
    }
}
