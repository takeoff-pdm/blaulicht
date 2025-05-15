use serde::Deserialize;


#[derive(Deserialize)]
pub enum Dimmer {}

impl Dimmer {
    pub fn write(&self, dmx: &mut [u8]) {}
}