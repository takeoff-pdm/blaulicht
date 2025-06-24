use serde::Deserialize;

use crate::user::clock::Time;

use super::Fixture;

#[derive(Deserialize, Debug)]
pub enum MovingHead {
    MartinMacAura,
}

impl MovingHead {
    pub fn write(&self, this: &Fixture, dmx: &mut [u8]) {
        match self {
            MovingHead::MartinMacAura => {
                // Strobe state.
                dmx[this.start_channel + 0] = this.strobe_state as u8 * 255;

                // Alpha.
                dmx[this.start_channel + 1] = this.alpha;

                // Color.
                dmx[this.start_channel + 9] = this.color.r;
                dmx[this.start_channel + 10] = this.color.g;
                dmx[this.start_channel + 11] = this.color.b;
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
