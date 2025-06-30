mod dimmer;
mod light;
mod moving_head;

use std::{borrow::Cow, collections::HashMap};

pub use dimmer::*;
pub use light::*;
pub use moving_head::*;
use serde::{Deserialize, Serialize};

use crate::dmx::{clock::Time, color::Color};

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct FixtureOrientation {
    pan: u8,
    tilt: u8,
    rotation: u8,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct FixtureGroup {
    // Assigns an ID to a fixture.
    pub fixtures: HashMap<u8, Fixture>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Fixture {
    name: Cow<'static, str>,
    type_: FixtureType,
    state: FixtureState,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct FixtureState {
    start_addr: usize,
    // the many values a fixture could have.
    brightness: u8,
    color: Color,
    alpha: u8,
    orientation: FixtureOrientation,
    strobe_speed: u8,
    // ... todo
}

impl Fixture {
    pub fn new(start_addr: usize, name: Cow<'static, str>, type_: FixtureType) -> Self {
        Self {
            name,
            type_,
            state: FixtureState {
                start_addr, // TODO: add check?
                brightness: 0,
                color: Color::default(),
                alpha: 0,
                strobe_speed: 0,
                orientation: FixtureOrientation::default(),
            },
        }
    }

    pub fn write(&self, dmx: &mut [u8]) {
        self.type_.write(self, dmx)
    }

    pub fn set_color(&mut self, color: Color) {
        self.state.color = color;
        // self.type_.write(self, dmx)
    }

    pub fn set_alpha(&mut self, alpha: u8) {
        self.state.alpha = alpha;
        // self.type_.write(self, dmx)
    }

    //
    // Begin rotation.
    //

    pub fn set_tilt(&mut self, tilt: u8) {
        self.state.orientation.tilt = tilt;
    }

    pub fn set_pan(&mut self, pan: u8) {
        self.state.orientation.pan = pan;
    }

    pub fn set_rotation(&mut self, rotation: u8) {
        self.state.orientation.rotation = rotation;
    }

    //
    // End rotation.
    //

    pub fn setup(&mut self, time: Time, dmx: &mut [u8]) {
        self.type_.setup(self, time, dmx);
    }
}

//
// Types.
//

#[derive(Serialize, Deserialize, Debug)]
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
