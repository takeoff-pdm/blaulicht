mod dimmer;
mod light;
mod moving_head;

pub use dimmer::*;
pub use light::*;
pub use moving_head::*;
use serde::Deserialize;

use crate::{color::Color, user::clock::Time};

#[derive(Deserialize, Debug)]
pub struct Rotation {
    pub tilt: u8,
    pub pan: u8,
}

impl Default for Rotation {
    fn default() -> Self {
        Self { tilt: 0, pan: 0 }
    }
}

#[derive(Deserialize, Debug)]
pub struct Fixture {
    start_channel: usize,
    type_: FixtureType,
    color: Color,
    alpha: u8,
    rotation: Rotation,
    strobe_state: bool,
}

impl Fixture {
    pub fn new(start_channel: usize, type_: FixtureType) -> Self {
        Self {
            start_channel,
            type_,
            color: Color::default(),
            alpha: 0,
            rotation: Rotation::default(),
            strobe_state: false,
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

    pub fn set_tilt_pan(&mut self, tilt: u8, pan: u8, dmx: &mut [u8]) {
        self.rotation = Rotation { tilt, pan };
        self.type_.write(self, dmx)
    }

    pub fn blackout(&mut self, dmx: &mut [u8]) {
        self.type_.write(self, dmx);
    }

    pub fn set_burst(&mut self, state: bool, dmx: &mut [u8]) {
        self.strobe_state = state;
        self.type_.write(self, dmx);
    }

    pub fn setup(&mut self, time: Time, dmx: &mut [u8]) {
        self.type_.setup(self, time, dmx);
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

    pub fn setup(&self, this: &Fixture, time: Time, dmx: &mut [u8]) {
        match self {
            FixtureType::MovingHead(moving_head) => moving_head.setup(this, time, dmx),
            FixtureType::Light(light) => light.setup(this, time, dmx),
            FixtureType::Dimmer(dimmer) => dimmer.setup(this, time, dmx),
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
