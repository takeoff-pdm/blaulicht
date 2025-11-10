use bincode::{Decode, Encode};
use map_range::MapRange;
use serde::{Deserialize, Serialize};
use std::{borrow::Cow, collections::BTreeMap};

use crate::{ControlEvent, FixtureProperty, HSVColor, RGBColor, fixture::FixtureType};

#[derive(Serialize, Deserialize, Debug, Clone, Encode, Decode)]
pub enum MergeStrategy {
    Highest,
    Latest,
    Interpolate,
}

#[derive(Serialize, Deserialize, Debug, Default, Clone, Encode, Decode)]
pub struct FixtureOrientation {
    pub pan: u8,
    pub tilt: u8,
    // pub rotation: u8,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, Encode, Decode)]
pub struct FixtureGroup {
    // Assigns an ID to a fixture.
    pub name: String,
    pub fixtures: BTreeMap<u8, Fixture>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Encode, Decode)]
pub struct Fixture {
    pub name: String,
    pub type_: FixtureType,
    pub pos: Position,
    // DMX start address + universe number.
    pub start_addr: usize,
    pub universe_no: usize,
}

impl Fixture {
    pub fn new(universe_no: usize, start_addr: usize, name: String, type_: FixtureType) -> Self {
        Self {
            name,
            type_,
            pos: Position::default(),
            start_addr,
            universe_no,
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

    pub fn setup(&self, time: i32, state: &FixtureState, dmx: &mut [u8]) {
        self.type_.setup(self, time, state, dmx);
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, Encode, Decode)]
pub struct Position {
    pub x: usize,
    pub y: usize,
    pub z: usize,
}

impl From<(usize, usize)> for Position {
    fn from(value: (usize, usize)) -> Self {
        Self {
            x: value.0,
            y: value.1,
            z: 0,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Encode, Decode)]
pub struct FixtureState {
    // the many values a fixture could have.
    pub color: HSVColor,
    pub alpha: u8,
    pub orientation: FixtureOrientation,
    pub strobe_speed: u8,
    pub focus: u8,
    // ... todo
}

impl Default for FixtureState {
    fn default() -> Self {
        FixtureState {
            color: HSVColor::default(),
            alpha: 0,
            orientation: FixtureOrientation::default(),
            strobe_speed: 0,
            focus: 0,
        }
    }
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

    pub fn apply_value(&mut self, value: u16, property: FixtureProperty) {
        match property {
            FixtureProperty::Alpha => self.alpha = value as u8,
            FixtureProperty::Strobe => self.strobe_speed = value as u8,
            FixtureProperty::Focus => self.focus = value as u8,
            FixtureProperty::ColorHue => {
                self.color.h = value as f64;
            }
            FixtureProperty::ColorSaturation => {
                self.color.s = (value as f64).map_range(0.0..255.0, 0.0..1.0)
            }
            FixtureProperty::ColorValue => {
                self.color.v = (value as f64).map_range(0.0..255.0, 0.0..1.0)
            }
            FixtureProperty::Tilt => self.orientation.tilt = value as u8,
            FixtureProperty::Pan => self.orientation.pan = value as u8,
        }
    }

    fn get_value(&self, property: FixtureProperty) -> u16 {
        match property {
            FixtureProperty::Alpha => self.alpha as u16,
            FixtureProperty::Strobe => self.strobe_speed as u16,
            FixtureProperty::Focus => self.focus as u16,
            FixtureProperty::ColorHue => self.color.h as u16,
            FixtureProperty::ColorSaturation => self.color.s.map_range(0.0..1.0, 0.0..255.0) as u16,
            FixtureProperty::ColorValue => self.color.v.map_range(0.0..1.0, 0.0..255.0) as u16,
            FixtureProperty::Tilt => self.orientation.tilt as u16,
            FixtureProperty::Pan => self.orientation.pan as u16,
        }
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn apply(&mut self, ev: ControlEvent) -> Vec<FixtureProperty> {
        match ev {
            ControlEvent::SetAlpha(alpha) => {
                self.alpha = alpha;
                vec![FixtureProperty::Alpha]
            }
            ControlEvent::SetStrobeSpeed(speed) => {
                self.strobe_speed = speed;
                vec![FixtureProperty::Strobe]
            }
            ControlEvent::SetFocus(v) => {
                self.focus = v;
                vec![FixtureProperty::Focus]
            }
            ControlEvent::SetPan(pan) => {
                self.orientation.pan = pan;
                vec![FixtureProperty::Pan]
            }
            ControlEvent::SetTilt(tilt) => {
                self.orientation.tilt = tilt;
                vec![FixtureProperty::Tilt]
            }
            ControlEvent::SetColor(clr) => {
                let color: RGBColor = clr.into();
                self.color = color.into();
                vec![
                    FixtureProperty::ColorHue,
                    FixtureProperty::ColorSaturation,
                    FixtureProperty::ColorValue,
                ]
            }
            ControlEvent::SetColorHue(hue) => {
                self.color.h = (hue as f64).clamp(0.0, 360.0);
                println!("hue: {}", self.color.h);
                vec![FixtureProperty::ColorHue]
            }
            ControlEvent::SetColorSaturation(sat) => {
                self.color.s = (sat as f64).map_range(0.0..255.0, 0.0..1.0);
                vec![FixtureProperty::ColorSaturation]
            }
            ControlEvent::SetColorValue(val) => {
                self.color.v = (val as f64).map_range(0.0..255.0, 0.0..1.0);
                vec![FixtureProperty::ColorValue]
            }
            other => unreachable!("Not supported: {other:?}"),
        }
    }
}
