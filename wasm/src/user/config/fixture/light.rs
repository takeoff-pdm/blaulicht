use serde::Deserialize;


#[derive(Deserialize)]
pub enum Light {}

impl Light {
    pub fn write(&self, dmx: &mut [u8]) {}
}