mod dimmer;
mod light;
mod moving_head;

use crate::dmx::clock::Time;
use blaulicht_shared::Color;
pub use dimmer::*;
pub use light::*;
pub use moving_head::*;
use serde::{Deserialize, Serialize};
use std::{borrow::Cow, collections::BTreeMap};

#[derive(Serialize, Deserialize, Debug, Default, Clone)]
pub struct FixtureOrientation {
    pub pan: u8,
    pub tilt: u8,
    pub rotation: u8,
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
    pub pos: Position,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct Position {
    pub x: usize,
    pub y: usize,
}

impl From<(usize, usize)> for Position {
    fn from(value: (usize, usize)) -> Self {
        Self {
            x: value.0,
            y: value.1,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FixtureState {
    pub start_addr: usize,
    // the many values a fixture could have.
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
            // state: FixtureState {
            //     start_addr, // TODO: add check?
            //     color: Color::default(),
            //     alpha: 0,
            //     strobe_speed: 0,
            //     orientation: FixtureOrientation::default(),
            // },
            pos: Position::default(),
        }
    }

    pub fn write(&self, state: &FixtureState, dmx: &mut [u8]) {
        self.type_.write(self, state, dmx)
    }

    // pub fn set_color(&mut self, color: (u8, u8, u8)) {
    //     self.state.color = color.into();
    //     // self.type_.write(self, dmx)
    // }
    //
    // pub fn set_alpha(&mut self, alpha: u8) {
    //     self.state.alpha = alpha;
    //     // self.type_.write(self, dmx)
    // }

    //
    // Begin rotation.
    //

    // pub fn set_tilt(&mut self, tilt: u8) {
    //     self.state.orientation.tilt = tilt;
    // }
    //
    // pub fn set_pan(&mut self, pan: u8) {
    //     self.state.orientation.pan = pan;
    // }
    //
    // pub fn set_rotation(&mut self, rotation: u8) {
    //     self.state.orientation.rotation = rotation;
    // }

    //
    // End rotation.
    //

    pub fn setup(&mut self, time: Time, state: &FixtureState, dmx: &mut [u8]) {
        self.type_.setup(self, time, state, dmx);
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
    pub fn write(&self, this: &Fixture, state: &FixtureState, dmx: &mut [u8]) {
        match self {
            FixtureType::MovingHead(moving_head) => moving_head.write(this, state, dmx),
            FixtureType::Light(light) => light.write(this, state, dmx),
            FixtureType::Dimmer(dimmer) => dimmer.write(this, state, dmx),
        }
    }

    pub fn blackout(&self, this: &Fixture, state: &FixtureState, dmx: &mut [u8]) {
        match self {
            FixtureType::MovingHead(moving_head) => moving_head.blackout(this, state, dmx),
            FixtureType::Light(light) => light.blackout(this, state, dmx),
            FixtureType::Dimmer(dimmer) => dimmer.blackout(this, state, dmx),
        }
    }

    pub fn setup(&self, this: &Fixture, time: Time, state: &FixtureState, dmx: &mut [u8]) {
        match self {
            FixtureType::MovingHead(moving_head) => moving_head.setup(this, time, state, dmx),
            FixtureType::Light(light) => light.setup(this, time, state, dmx),
            FixtureType::Dimmer(dimmer) => dimmer.setup(this, time, state, dmx),
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
