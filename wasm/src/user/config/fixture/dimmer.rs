use serde::Deserialize;

use crate::user::clock::Time;

use super::Fixture;


#[derive(Deserialize, Debug)]
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