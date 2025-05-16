mod fixture;

pub use fixture::{Dimmer, Fixture, FixtureType, Light, MovingHead};

use serde::Deserialize;

use crate::color::Color;

#[derive(Deserialize, Debug)]
pub struct FixtureGroup {
    pub enabled: bool,
    pub label: String,
    pub fixtures: Vec<Fixture>,
}

impl FixtureGroup {
    pub fn new(label: String, fixtures: Vec<Fixture>) -> Self {
        Self {
            enabled: true,
            label,
            fixtures,
        }
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        // TODO: should call blackout?
    }

    pub fn blackout(&mut self, dmx: &mut [u8]) {
        for fixture in self.fixtures.iter_mut() {
            fixture.blackout(dmx);
        }
    }

    pub fn set_color(&mut self, color: Color, dmx: &mut [u8]) {
        for fixture in self.fixtures.iter_mut() {
            fixture.set_color(color, dmx);
        }
    }

    pub fn set_alpha(&mut self, alpha: u8, dmx: &mut [u8]) {
        for fixture in self.fixtures.iter_mut() {
            fixture.set_alpha(alpha, dmx);
        }
    }
}

pub type StrobeGroup = FixtureGroup;
pub type MoodGroup = FixtureGroup;
pub type DimmerGroup = FixtureGroup;

#[derive(Deserialize, Debug)]
pub struct Config {
    pub strobe_groups: Vec<StrobeGroup>,
    pub mood_groups: Vec<MoodGroup>,
    pub dimmer_groups: Vec<DimmerGroup>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            strobe_groups: Default::default(),
            mood_groups: Default::default(),
            dimmer_groups: Default::default(),
        }
    }
}
