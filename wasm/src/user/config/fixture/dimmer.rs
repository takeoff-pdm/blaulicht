use serde::Deserialize;

use super::Fixture;


#[derive(Deserialize, Debug)]
pub enum Dimmer {}

impl Dimmer {
    pub fn write(&self, this: &Fixture, dmx: &mut [u8]) {}
    pub fn blackout(&self, this: &Fixture, dmx: &mut [u8]) {}
}