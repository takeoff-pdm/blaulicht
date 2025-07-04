use serde::{Deserialize, Serialize};

use crate::dmx::clock::Time;

use super::Fixture;


#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Dimmer {}

impl Dimmer {
    pub fn write(&self, this: &Fixture, dmx: &mut [u8]) {}
    pub fn blackout(&self, this: &Fixture, dmx: &mut [u8]) {}
    pub fn setup(&self, this: &Fixture, time: Time, dmx: &mut [u8]) {
        // match self {
        //     MovingHead::Generic3ChanNoAlpha => todo!(),
        //     MovingHead::Generic4ChanWithAlpha => todo!(),
        // }
    }
}