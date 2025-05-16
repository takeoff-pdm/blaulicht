mod dimmer;
mod light;
mod moving_head;

pub use dimmer::*;
pub use light::*;
pub use moving_head::*;
use serde::Deserialize;

use crate::color::Color;

#[derive(Deserialize, Debug)]
pub struct Fixture {
    start_channel: usize,
    type_: FixtureType,
    color: Color,
    alpha: u8,
}

impl Fixture {
    pub fn new(start_channel: usize, type_: FixtureType) -> Self {
        Self {
            start_channel,
            type_,
            color: Color::default(),
            alpha: 0,
        }
    }

    pub fn write(&self, dmx: &mut [u8]) {
        self.type_.write(self, dmx)
    }

    pub fn set_color(&mut self, color: Color, dmx: &mut [u8]) {
        self.color = color;
        self.type_.write(self, dmx)
    }

    pub fn set_alpha(&mut self, alpha: u8, dmx: &mut [u8]) {
        self.alpha = alpha;
        self.type_.write(self, dmx)
    }

    pub fn blackout(&mut self, dmx: &mut [u8]) {
        self.type_.write(self, dmx);
    }
}

// Convert type and address tuple into Fixture.
impl From<(FixtureType, usize)> for Fixture {
    fn from(value: (FixtureType, usize)) -> Self {
        Self::new(value.1, value.0)
    }
}

//
// Types.
//

#[derive(Deserialize, Debug)]
pub enum FixtureType {
    MovingHead(MovingHead),
    Light(Light),
    Dimmer(Dimmer),
}

impl FixtureType {
    pub fn write(&self, this: &Fixture, dmx: &mut [u8]) {
        match self {
            FixtureType::MovingHead(moving_head) => moving_head.write(this, dmx),
            FixtureType::Light(light) => light.write(this, dmx),
            FixtureType::Dimmer(dimmer) => dimmer.write(this, dmx),
        }
    }

    pub fn blackout(&self, this: &Fixture, dmx: &mut [u8]) {
        match self {
            FixtureType::MovingHead(moving_head) => moving_head.blackout(this, dmx),
            FixtureType::Light(light) => light.blackout(this, dmx),
            FixtureType::Dimmer(dimmer) => dimmer.blackout(this, dmx),
        }
    }
}

impl From<Light> for FixtureType {
    fn from(value: Light) -> Self {
        Self::Light(value)
    }
}

impl From<MovingHead> for FixtureType {
    fn from(value: MovingHead) -> Self {
        Self::MovingHead(value)
    }
}

impl From<Dimmer> for FixtureType {
    fn from(value: Dimmer) -> Self {
        Self::Dimmer(value)
    }
}
