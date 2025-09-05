mod dimmer;
mod light;
mod moving_head;

use crate::dmx::clock::Time;
use blaulicht_shared::{FixtureProperty, HSVColor, RGBColor};
pub use dimmer::*;
pub use light::*;
use map_range::MapRange;
pub use moving_head::*;
use serde::{Deserialize, Serialize};
use std::{borrow::Cow, collections::BTreeMap, u16};

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
    pub color: HSVColor,
    pub alpha: u8,
    pub orientation: FixtureOrientation,
    pub strobe_speed: u8,
    // ... todo
}

impl Default for FixtureState {
    fn default() -> Self {
        FixtureState {
            start_addr: 42,
            color: HSVColor::default(),
            alpha: 0,
            orientation: FixtureOrientation::default(),
            strobe_speed: 0,
        }
    }
}

pub enum MergeStrategy {
    Highest,
    Latest,
    Interpolate,
}

impl FixtureState {
    // Pulls in a changed property of another fixture state.
    // Uses the specified merge strategy.
    pub fn merge_from(
        &mut self,
        other: &FixtureState,
        property: FixtureProperty,
        strategy: MergeStrategy,
    ) {
        let self_value = self.get_value(property);
        let other_value = other.get_value(property);

        let new_value = match strategy {
            MergeStrategy::Highest => self_value.max(other_value),
            MergeStrategy::Latest => other_value,
            MergeStrategy::Interpolate => self_value.midpoint(other_value),
        };

        self.apply_value(new_value, property);
    }

    fn apply_value(&mut self, value: u16, property: FixtureProperty) {
        match property {
            FixtureProperty::Alpha => self.alpha = value as u8,
            FixtureProperty::ColorHue => {
                self.color.h = (value as f64).map_range(0.0..360.0, 0.0..1.0)
            }
            FixtureProperty::ColorSaturation => {
                self.color.s = (value as f64).map_range(0.0..255.0, 0.0..1.0)
            }
            FixtureProperty::ColorValue => {
                self.color.v = (value as f64).map_range(0.0..255.0, 0.0..1.0)
            }
            FixtureProperty::Tilt => self.orientation.tilt = value as u8,
            FixtureProperty::Pan => self.orientation.pan = value as u8,
            FixtureProperty::Rotation => self.orientation.rotation = value as u8,
        }
    }

    fn get_value(&self, property: FixtureProperty) -> u16 {
        match property {
            FixtureProperty::Alpha => self.alpha as u16,
            FixtureProperty::ColorHue => self.color.h.map_range(0.0..1.0, 0.0..360.0) as u16,
            FixtureProperty::ColorSaturation => self.color.s.map_range(0.0..1.0, 0.0..255.0) as u16,
            FixtureProperty::ColorValue => self.color.v.map_range(0.0..1.0, 0.0..255.0) as u16,
            FixtureProperty::Tilt => self.orientation.tilt as u16,
            FixtureProperty::Pan => self.orientation.pan as u16,
            FixtureProperty::Rotation => self.orientation.rotation as u16,
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
