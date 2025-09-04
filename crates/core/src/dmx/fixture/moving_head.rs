use serde::{Deserialize, Serialize};

use crate::dmx::{clock::Time, FixtureState};

use super::Fixture;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum MovingHead {
    MartinMacAura,
}

impl MovingHead {
    pub fn write(&self, this: &Fixture, state: &FixtureState, dmx: &mut [u8]) {
        match self {
            MovingHead::MartinMacAura => {
                // Strobe state.
                dmx[state.start_addr + 0] = state.strobe_speed as u8 * 255;

                // Alpha.
                dmx[state.start_addr + 1] = state.alpha;

                // Color.
                dmx[state.start_addr + 9] = state.color.r;
                dmx[state.start_addr + 10] = state.color.g;
                dmx[state.start_addr + 11] = state.color.b;
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

    pub fn setup(&self, this: &Fixture, time: Time, state: &FixtureState, dmx: &mut [u8]) {
        todo!("hard")
        // TODO: just call write.
        // self.write(this, dmx);
        // match self {
        //     MovingHead::Generic3ChanNoAlpha => todo!(),
        //     MovingHead::Generic4ChanWithAlpha => todo!(),
        // }
    }
}
