use serde::Deserialize;


#[derive(Deserialize)]
pub enum MovingHead {}

impl MovingHead {
    pub fn write(&self, dmx: &mut [u8]) {}
}