pub mod dimmer;
pub mod light;
pub mod moving_head;
pub mod state;

use bincode::{Decode, Encode};
use dimmer::*;
use light::*;
use moving_head::*;
use serde::{Deserialize, Serialize};
use state::*;

//
// Types.
//

#[derive(Serialize, Deserialize, Debug, Clone, Encode, Decode)]
pub enum FixtureType {
    MovingHead(MovingHead),
    Light(Light),
    Dimmer(Dimmer),
}

impl FixtureType {
    // How many channel this type uses.
    pub fn footprint(&self) -> usize {
        match self {
            FixtureType::MovingHead(moving_head) => moving_head.footprint(),
            FixtureType::Light(light) => light.footprint(),
            FixtureType::Dimmer(dimmer) => dimmer.footprint(),
        }
    }

    pub fn kind_string(&self) -> &'static str {
        match self {
            FixtureType::MovingHead(_) => "MovingHead",
            FixtureType::Light(_) => "Light",
            FixtureType::Dimmer(_) => "Dimmer",
        }
    }

    pub fn model_string(&self) -> String {
        match self {
            FixtureType::MovingHead(model) => model.to_string(),
            FixtureType::Light(model) => model.to_string(),
            FixtureType::Dimmer(model) => model.to_string(),
        }
    }

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

    pub fn setup(&self, this: &Fixture, time: i32, state: &FixtureState, dmx: &mut [u8]) {
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
