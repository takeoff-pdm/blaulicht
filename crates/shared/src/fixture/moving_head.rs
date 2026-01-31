use crate::{
    HSVColor, RGBColor,
    fixture::state::{FixtureOrientation, FixtureState, Position},
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

    pub fn write(&self, this: &Fixture, state: &FixtureState, dmx: &mut [u8]) {
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
                dmx[this.start_addr + 0] = state.strobe_speed as u8;

                // Alpha.
                dmx[this.start_addr + 1] = state.alpha;

                // Color.
                dmx[this.start_addr + 3] = state.color.h.map_range(0.0..360.0, 0.0..255.0) as u8;

                // Gobo wheel.
                dmx[this.start_addr + 11] = state.color.s.map_range(0.0..1.0, 0.0..255.0) as u8;

                // Gobo rot.
                dmx[this.start_addr + 5] = state.color.v.map_range(0.0..1.0, 0.0..255.0) as u8;

                // Focus
                // dmx[this.start_addr + 6] = state.color.h.map_range(0.0..360.0, 0.0..255.0) as u8;

                // dmx[this.start_addr + 5] = state.color.s.map_range(0.0..1.0, 0.0..255.0) as u8;

                // Actually gobo shit
                //dmx[this.start_addr + 9] = color.r; // WTF.
                //dmx[this.start_addr + 10] = color.g;
                //dmx[this.start_addr + 11] = color.b;

                // Position
                dmx[this.start_addr + 12] = state.orientation.pan;
                dmx[this.start_addr + 13] = 0;

                // tilt
                dmx[this.start_addr + 14] = state.orientation.tilt;
                dmx[this.start_addr + 15] = 0;
            }
            MovingHead::VaryTechHeroSpot60 => {
                dmx[this.start_addr + 0] = state.orientation.pan;
                dmx[this.start_addr + 1] = state.orientation.tilt;
                dmx[this.start_addr + 2] = 0;
                dmx[this.start_addr + 3] = state.alpha;
                dmx[this.start_addr + 4] = match state.strobe_speed {
                    0 => 0,
                    v => v.map_range(1..255, 10..250),
                };
                dmx[this.start_addr + 5] = state.focus;
                dmx[this.start_addr + 6] = state.color.s.map_range(0.0..1.0, 0.0..255.0) as u8;
            }
        }
        // match self {
        //     MovingHead::Generic3ChanNoAlpha => todo!(),
        //     MovingHead::Generic4ChanWithAlpha => todo!(),
        // }
    }

    pub fn state_from_dmx(&self, this: &Fixture, dmx: &[u8]) -> FixtureState {
        match self {
            MovingHead::MartinMac250E => {
                let strobe_speed = dmx[this.start_addr + 0];
                let alpha = dmx[this.start_addr + 1];
                let hue = (dmx[this.start_addr + 3] as f64).map_range(0.0..255.0, 0.0..360.0);
                let saturation =
                    (dmx[this.start_addr + 11] as f64).map_range(0.0..255.0, 0.0..1.0);
                let value = (dmx[this.start_addr + 5] as f64).map_range(0.0..255.0, 0.0..1.0);
                let pan = dmx[this.start_addr + 12];
                let tilt = dmx[this.start_addr + 14];

                FixtureState {
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
                let pan = dmx[this.start_addr + 0];
                let tilt = dmx[this.start_addr + 1];
                let alpha = dmx[this.start_addr + 3];
                let strobe = match dmx[this.start_addr + 4] {
                    0 => 0,
                    v if v < 10 => 0,
                    v => v.map_range(10..250, 1..255),
                };
                let focus = dmx[this.start_addr + 5];
                let mut color = HSVColor::default();
                color.s = (dmx[this.start_addr + 6] as f64).map_range(0.0..255.0, 0.0..1.0);

                FixtureState {
                    color,
                    alpha,
                    orientation: FixtureOrientation { pan, tilt },
                    strobe_speed: strobe,
                    focus,
                }
            }
        }
    }

    pub fn blackout(&self, this: &Fixture, state: &FixtureState, dmx: &mut [u8]) {
        // match self {
        //     MovingHead::Generic3ChanNoAlpha => todo!(),
        //     MovingHead::Generic4ChanWithAlpha => todo!(),
        // }
    }

    pub fn setup(&self, this: &Fixture, time: i32, state: &FixtureState, dmx: &mut [u8]) {
        match self {
            MovingHead::MartinMac250E => {
                self.write(
                    this,
                    &FixtureState {
                        orientation: FixtureOrientation {
                            pan: 127,
                            tilt: 127,
                        },
                        ..state.clone()
                    },
                    dmx,
                ); // Ensure all other values are not fucked up.

                match time {
                    v if v <= 5000 => {
                        // Enable lamp.
                        println!("ENABLE LAMP");
                        dmx[this.start_addr + 0] = 237;
                    }
                    v => {
                        println!("DONT ENABLE LAMP");
                        dmx[this.start_addr + 0] = 20;
                    }
                }
            }
            MovingHead::VaryTechHeroSpot60 => {}
        }
        // TODO: just call write.
        // self.write(this, dmx);
        // match self {
        //     MovingHead::Generic3ChanNoAlpha => todo!(),
        //     MovingHead::Generic4ChanWithAlpha => todo!(),
        // }
    }
}
