use crate::{
    HSVColor, RGBColor,
    fixture::state::{FixtureOrientation, FixtureState, ResolvedFixtureState},
};

use super::Fixture;
use bincode::{Decode, Encode};
use map_range::MapRange;
use serde::{Deserialize, Serialize};
use std::fmt::Display;
use strum::EnumIter;

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, EnumIter, Encode, Decode)]
pub enum MovingHead {
    //
    // Channel map is complicated, look below.
    //
    MartinMac250E,
    //
    // 0: Pan (mid 128)
    // 1: Tilt (mid 128)
    // 3: Pan / Tilt Speed
    // 4: Alpha
    // 5: Strobe (0 = open | 10..250 = strobe)
    // 6: Focus
    // 7: Gobos
    //
    VaryTechHeroSpot60,
}

impl Display for MovingHead {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl MovingHead {
    pub fn footprint(&self) -> usize {
        match self {
            MovingHead::MartinMac250E => 18,
            MovingHead::VaryTechHeroSpot60 => 8,
        }
    }

    pub fn write(&self, this: &Fixture, state: &ResolvedFixtureState, dmx: &mut [u8]) {
        let color: RGBColor = state.color.into();

        match self {
            MovingHead::MartinMac250E => {
                // // shutter
                // if !value {
                //     dmx[start_addr + 0] = 0;
                // } else {
                //     dmx[start_addr + 0] = 50;
                // }
                //
                // dmx[start_addr + 1] = normal_bri;
                // dmx[start_addr + 2] = 0;

                // Strobe state.
                fixture_channel!(dmx, this, 0) = state.strobe_speed;
                // Alpha.
                fixture_channel!(dmx, this, 1) = state.alpha;

                // Color.
                fixture_channel!(dmx, this, 3) =
                    state.color.h.map_range(0.0..360.0, 0.0..255.0) as u8;

                // Gobo wheel.
                fixture_channel!(dmx, this, 11) =
                    state.color.s.map_range(0.0..1.0, 0.0..255.0) as u8;

                // Gobo rot.
                fixture_channel!(dmx, this, 5) =
                    state.color.v.map_range(0.0..1.0, 0.0..255.0) as u8;

                // Focus
                // fixture_channel!(dmx, this, 6) = state.color.h.map_range(0.0..360.0, 0.0..255.0) as u8;

                // fixture_channel!(dmx, this, 5) = state.color.s.map_range(0.0..1.0, 0.0..255.0) as u8;

                // Actually gobo shit
                //fixture_channel!(dmx, this, 9) = color.r; // WTF.
                //fixture_channel!(dmx, this, 10) = color.g;
                //fixture_channel!(dmx, this, 11) = color.b;

                // Position
                fixture_channel!(dmx, this, 12) = state.orientation.pan;
                fixture_channel!(dmx, this, 13) = 0;

                // tilt
                fixture_channel!(dmx, this, 14) = state.orientation.tilt;
                fixture_channel!(dmx, this, 15) = 0;
            }
            MovingHead::VaryTechHeroSpot60 => {
                fixture_channel!(dmx, this, 0) = state.orientation.pan;
                fixture_channel!(dmx, this, 1) = state.orientation.tilt;
                fixture_channel!(dmx, this, 2) = 0;
                fixture_channel!(dmx, this, 3) = state.alpha;
                fixture_channel!(dmx, this, 4) = match state.strobe_speed {
                    0 => 0,
                    v => crate::fixture::map_range_u8(v, (1, 255), (10, 250)),
                };
                fixture_channel!(dmx, this, 5) = state.focus;
                fixture_channel!(dmx, this, 6) =
                    state.color.s.map_range(0.0..1.0, 0.0..255.0) as u8;
            }
        }
        // match self {
        //     MovingHead::Generic3ChanNoAlpha => todo!(),
        //     MovingHead::Generic4ChanWithAlpha => todo!(),
        // }
    }

    pub fn state_from_dmx(&self, this: &Fixture, dmx: &[u8]) -> FixtureState {
        let resolved: ResolvedFixtureState = match self {
            MovingHead::MartinMac250E => {
                let strobe_speed = fixture_channel!(dmx, this, 0);
                let alpha = fixture_channel!(dmx, this, 1);
                let hue = (fixture_channel!(dmx, this, 3) as f64).map_range(0.0..255.0, 0.0..360.0);
                let saturation =
                    (fixture_channel!(dmx, this, 11) as f64).map_range(0.0..255.0, 0.0..1.0);
                let value = (fixture_channel!(dmx, this, 5) as f64).map_range(0.0..255.0, 0.0..1.0);
                let pan = fixture_channel!(dmx, this, 12);
                let tilt = fixture_channel!(dmx, this, 14);

                ResolvedFixtureState {
                    color: HSVColor {
                        h: hue,
                        s: saturation,
                        v: value,
                    },
                    alpha,
                    orientation: FixtureOrientation { pan, tilt },
                    strobe_speed,
                    focus: 0,
                }
            }
            MovingHead::VaryTechHeroSpot60 => {
                let pan = fixture_channel!(dmx, this, 0);
                let tilt = fixture_channel!(dmx, this, 1);
                let alpha = fixture_channel!(dmx, this, 3);
                let strobe = match fixture_channel!(dmx, this, 4) {
                    0 => 0,
                    v if v < 10 => 0,
                    v => crate::fixture::map_range_u8(v, (10, 250), (1, 255)),
                };
                let focus = fixture_channel!(dmx, this, 5);
                let mut color = HSVColor::default();
                color.s = (fixture_channel!(dmx, this, 6) as f64).map_range(0.0..255.0, 0.0..1.0);

                ResolvedFixtureState {
                    color,
                    alpha,
                    orientation: FixtureOrientation { pan, tilt },
                    strobe_speed: strobe,
                    focus,
                }
            }
        };
        resolved.into()
    }

    pub fn blackout(&self, _this: &Fixture, _state: &ResolvedFixtureState, _dmx: &mut [u8]) {}

    pub fn setup(&self, this: &Fixture, time: i32, state: &ResolvedFixtureState, dmx: &mut [u8]) {
        match self {
            MovingHead::MartinMac250E => {
                // Ensure all other values are not fucked up.
                self.write(
                    this,
                    &ResolvedFixtureState {
                        orientation: FixtureOrientation {
                            pan: 127,
                            tilt: 127,
                        },
                        ..state.clone()
                    },
                    dmx,
                );

                // Then manually set the shutter.
                match time {
                    v if v <= 5000 => {
                        // Enable lamp.
                        println!("ENABLE LAMP");
                        fixture_channel!(dmx, this, 0) = 237;
                    }
                    _ => {
                        println!("DONT ENABLE LAMP");
                        fixture_channel!(dmx, this, 0) = 20;
                    }
                }
            }
            MovingHead::VaryTechHeroSpot60 => {}
        }
    }
}
