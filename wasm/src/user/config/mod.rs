mod fixture;

pub use fixture::*;

use serde::Deserialize;

use crate::color::Color;

use super::{
    clock::Time,
    println,
    state::strobe::{StrobeGroupState, StrobeState},
};

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

    pub fn set_enabled(&mut self, dmx: &mut [u8], enabled: bool) {
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

    pub fn set_tilt_pan(&mut self, tilt: u8, pan: u8, dmx: &mut [u8]) {
        for fixture in self.fixtures.iter_mut() {
            fixture.set_tilt_pan(tilt, pan, dmx);
        }
    }

    pub fn setup(&mut self, time: Time, dmx: &mut [u8]) {
        for fixture in self.fixtures.iter_mut() {
            fixture.setup(time, dmx);
        }
    }
}

#[derive(Deserialize, Debug)]
pub struct StrobeGroup {
    pub supports_burst: bool,
    pub group: FixtureGroup,
    #[serde(skip)]
    pub state: StrobeGroupState,
}

impl StrobeGroup {
    pub fn new(supports_burst: bool, group: FixtureGroup) -> Self {
        Self {
            supports_burst,
            state: StrobeGroupState::default(),
            group,
        }
    }

    //
    // Strobe logic.
    //

    pub fn set_burst(&mut self, state: bool, dmx: &mut [u8]) {
        for fixture in self.group.fixtures.iter_mut() {
            // So that it gets deactivated in case it remains stuck on white.
            fixture.set_burst(state, dmx);
        }
        // println!("burst...");
    }

    //
    // End strobe logic.
    //

    pub fn set_enabled(&mut self, dmx: &mut [u8], enabled: bool) {
        self.group.set_enabled(dmx, enabled);
    }

    pub fn blackout(&mut self, dmx: &mut [u8]) {
        self.group.blackout(dmx);
    }

    pub fn set_color(&mut self, color: Color, dmx: &mut [u8]) {
        self.group.set_color(color, dmx);
    }

    pub fn set_alpha(&mut self, alpha: u8, dmx: &mut [u8]) {
        self.group.set_alpha(alpha, dmx);
    }

    pub fn set_tilt_pan(&mut self, tilt: u8, pan: u8, dmx: &mut [u8]) {
        self.group.blackout(dmx);
    }

    pub fn setup(&mut self, time: Time, dmx: &mut [u8]) {
        self.group.set_color(Color::white(), dmx);
        self.group.set_alpha(0, dmx);
        self.group.setup(time, dmx);
    }
}

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
