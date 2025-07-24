mod dimmer;
mod light;
mod moving_head;

use std::{borrow::Cow, collections::BTreeMap};

use blaulicht_shared::Color;
pub use dimmer::*;
pub use light::*;
pub use moving_head::*;
use serde::{Deserialize, Serialize};

use crate::dmx::clock::Time;

#[derive(Serialize, Deserialize, Debug, Default, Clone)]
pub struct FixtureOrientation {
    pan: u8,
    tilt: u8,
    rotation: u8,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FixtureGroup {
    // Assigns an ID to a fixture.
    pub fixtures: BTreeMap<u8, Fixture>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Fixture {
    pub name: Cow<'static, str>,
    pub type_: FixtureType,
    pub state: FixtureState,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FixtureState {
    pub start_addr: usize,
    // the many values a fixture could have.
    pub brightness: u8,
    pub color: Color,
    pub alpha: u8,
    pub orientation: FixtureOrientation,
    pub strobe_speed: u8,
    // ... todo
}

impl Default for FixtureState {
    fn default() -> Self {
        FixtureState {
            start_addr: 42,
            brightness: 0,
            color: Color::default(),
            alpha: 0,
            orientation: FixtureOrientation::default(),
            strobe_speed: 0,
        }
    }
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

    pub fn set_color(&mut self, color: (u8, u8, u8)) {
        self.state.color = color.into();
        // self.type_.write(self, dmx)
    }

    pub fn set_brightness(&mut self, bri: u8) {
        self.state.brightness = bri;
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

#[derive(Serialize, Deserialize, Debug, Clone)]
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
