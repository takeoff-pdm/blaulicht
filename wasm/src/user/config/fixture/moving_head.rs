use serde::Deserialize;

use super::Fixture;


#[derive(Deserialize, Debug)]
pub enum MovingHead {
}

impl MovingHead {
    pub fn write(&self, this: &Fixture, dmx: &mut [u8]) {
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
}