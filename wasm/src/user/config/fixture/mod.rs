mod light;
mod moving_head;
mod dimmer;

use dimmer::Dimmer;
use light::Light;
use moving_head::MovingHead;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct Fixture {
    start_channel: usize,
    type_: FixtureType,
}

impl Fixture {
    pub fn new(start_channel: usize, type_: FixtureType) -> Self {
        Self {
            start_channel,
            type_,
        }
    }

    pub fn write(&self, dmx: &mut [u8]) {
        self.type_.write(dmx)
    }
}

#[derive(Deserialize)]
pub enum FixtureType {
    MovingHead(MovingHead),
    Light(Light),
    Dimmer(Dimmer),
}

impl FixtureType {
    pub fn write(&self, dmx: &mut [u8]) {
        match self {
            FixtureType::MovingHead(moving_head) => moving_head.write(dmx),
            FixtureType::Light(light) => light.write(dmx),
            FixtureType::Dimmer(dimmer) => dimmer.write(dmx),
        }
    }
}