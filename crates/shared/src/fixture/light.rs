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

#[allow(nonstandard_style)]
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, EnumIter, Encode, Decode)]
pub enum Light {
    //
    // 0: Red
    // 1: Green
    // 2: Blue
    //
    Generic3ChanNoAlpha,
    //
    // 0: Color index (hue, 0..255 -> 0..360 deg)
    // 1: Brightness (alpha)
    //
    GenericColorAlphaLight,
    //
    // 0: Alpha
    // 1: Red
    // 2: Green
    // 3: Blue
    //
    Generic4ChanWithAlpha,
    //
    // 0: Red
    // 1: Green
    // 2: Blue
    // 4: Alpha
    // 5: Strobe
    //
    LEDPartyTCLSpot,
    //
    // 0: Red
    // 1: Green
    // 2: Blue
    // 4: UNKNOWN
    // 5: UNKNOWN
    // 6: Alpha
    //
    AdjMegaHexPar,
    //
    // 0: Red
    // 1: Green
    // 2: Blue
    // 4: UNKNOWN
    // 5: UNKNOWN
    // 6: UNKNOWN
    // 7: Alpha
    //
    LiteCraftMiniParAT10,
    //
    // 0: Alpha
    // 1: Strobe (0..30)
    // 2: Warm White
    // 2: Cold White
    //
    VaryTechVP1,
    //
    // 0: Master Dimmer
    // 1: Strobe
    // 2: Macro
    // 3: Macro Speed
    // 4: Red
    // 5: Green
    // 6: Blue
    // 7: White
    //
    LightMaxxVegaSilentPar2Quad,
    //
    // 0: Red
    // 1: Green
    // 2: Blue
    // 3: Dimmer
    // 4: Strobe (value 011-255)
    LEDPar64RGBSpot5Chan,
    //
    // 0: Red
    // 1: Green
    // 2: Blue
    // 3: White
    //
    CameoQSpot40RGBW_4Chan,
    //
    // 0: Hue
    // 1: Rotation
    // 2: Strobe
    // 3: Alpha
    //
    LightMaxxTripleDerbyHP,
    //
    // 0: UV Dimmer
    // 1: UV Dimmer
    // 2: Strobe (0-250)
    // 3: Matrix (maps from hue)
    // 5: Laser (maps from hue)
    // 4: Speed
    // 6: Laser Strobe (06-250)
    // 7: Laser Rotation (maps from tilt)
    // 8: White SMDS
    // 9: Speed
    EuroLiteLEDMultiFX_10Chan,
    //
    // 0: Alpha
    // 1: Red
    // 2: Green
    // 3: Blue
    // 4: BPM Control (0 = unchanged, 1-255 -> ~40-240 BPM)
    // 5: Focus
    //
    TakeOffLogo,
    //
    // 0: Red
    // 1: Green
    // 2: Blue
    // 4: Color macro (kept at 0 -> manual RGB)
    // 5: Strobe
    // 6: Color chase (kept at 0 -> chase off)
    // 7: Brightness
    //
    KuzeLEDPar,
    //
    // 0: Dimmer
    // 1: Strobe
    // 2: Red
    // 3: Green
    // 4: Blue
    // 5: Sound control (kept at 0 -> disabled)
    //
    StairvilleWildWash648RGB_6Chan,
    //
    // Stairville LED PAR 56 (24x3W RGB MKII), 7-channel mode.
    // 0: Red
    // 1: Green
    // 2: Blue
    // 3: Color macro (kept at 0 -> manual RGB)
    // 4: Strobe (0..15 = off, 16..255 = strobe frequency, when ch 5 in 0..31)
    // 5: Mode (kept at 0 -> strobe-active manual color)
    // 6: Master dimmer
    //
    StairvilleLEDPar56_7Chan,
}

impl Display for Light {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl Light {
    pub fn footprint(&self) -> usize {
        match self {
            Light::Generic3ChanNoAlpha => 3,
            Light::GenericColorAlphaLight => 2,
            Light::Generic4ChanWithAlpha => 4,
            Light::LEDPartyTCLSpot => 6,
            Light::AdjMegaHexPar => 7,
            Light::LiteCraftMiniParAT10 => 8,
            Light::VaryTechVP1 => 4,
            Light::LightMaxxVegaSilentPar2Quad => 8,
            Light::LEDPar64RGBSpot5Chan => 5,
            Light::CameoQSpot40RGBW_4Chan => 4,
            Light::LightMaxxTripleDerbyHP => 4,
            Light::EuroLiteLEDMultiFX_10Chan => 10,
            Light::TakeOffLogo => 6,
            Light::KuzeLEDPar => 8,
            Light::StairvilleWildWash648RGB_6Chan => 6,
            Light::StairvilleLEDPar56_7Chan => 7,
        }
    }

    pub fn write(&self, this: &Fixture, state: &ResolvedFixtureState, dmx: &mut [u8]) {
        let color: RGBColor = state.color.into();

        match self {
            Light::Generic3ChanNoAlpha => {
                let (r, g, b) = color.tup();
                let alpha = state.alpha;

                fixture_channel!(dmx, this, 0) = (r as f32 / 255.0 * alpha as f32) as u8;
                fixture_channel!(dmx, this, 1) = (g as f32 / 255.0 * alpha as f32) as u8;
                fixture_channel!(dmx, this, 2) = (b as f32 / 255.0 * alpha as f32) as u8;
            }
            Light::GenericColorAlphaLight => {
                fixture_channel!(dmx, this, 0) =
                    (state.color.h.map_range(0.0..360.0, 0.0..255.0)) as u8;
                fixture_channel!(dmx, this, 1) = state.alpha;
            }
            Light::Generic4ChanWithAlpha => {
                fixture_channel!(dmx, this, 0) = state.alpha;
                fixture_channel!(dmx, this, 1) = color.r;
                fixture_channel!(dmx, this, 2) = color.g;
                fixture_channel!(dmx, this, 3) = color.b;
            }
            Light::LEDPartyTCLSpot => {
                fixture_channel!(dmx, this, 0) = color.r;
                fixture_channel!(dmx, this, 1) = color.g;
                fixture_channel!(dmx, this, 2) = color.b;
                fixture_channel!(dmx, this, 3) = state.alpha;
            }
            Light::AdjMegaHexPar => {
                fixture_channel!(dmx, this, 0) = color.r;
                fixture_channel!(dmx, this, 1) = color.g;
                fixture_channel!(dmx, this, 2) = color.b;
                fixture_channel!(dmx, this, 6) = state.alpha;
            }
            Light::LiteCraftMiniParAT10 => {
                fixture_channel!(dmx, this, 0) = color.r;
                fixture_channel!(dmx, this, 1) = color.g;
                fixture_channel!(dmx, this, 2) = color.b;
                fixture_channel!(dmx, this, 7) = state.alpha;
            }
            Light::VaryTechVP1 => {
                fixture_channel!(dmx, this, 0) = state.alpha;
                fixture_channel!(dmx, this, 1) = state.strobe_speed; // strobe
                fixture_channel!(dmx, this, 2) = 127; // warm white
                fixture_channel!(dmx, this, 3) = 127; // cold white
            }
            Light::LightMaxxVegaSilentPar2Quad => {
                fixture_channel!(dmx, this, 0) = state.alpha;
                fixture_channel!(dmx, this, 1) = state.strobe_speed;
                fixture_channel!(dmx, this, 2) = 0;
                fixture_channel!(dmx, this, 3) = 0; // MACRO
                fixture_channel!(dmx, this, 4) = color.r;
                fixture_channel!(dmx, this, 5) = color.g;
                fixture_channel!(dmx, this, 6) = color.b;
                // WHITE override.
                fixture_channel!(dmx, this, 7) =
                    if color.r == 255 && color.g == 255 && color.b == 255 {
                        255
                    } else {
                        0
                    };
            }
            Light::LEDPar64RGBSpot5Chan => {
                fixture_channel!(dmx, this, 0) = color.r;
                fixture_channel!(dmx, this, 1) = color.g;
                fixture_channel!(dmx, this, 2) = color.b;
                fixture_channel!(dmx, this, 3) = state.alpha;
                fixture_channel!(dmx, this, 4) = match state.strobe_speed {
                    0 => 0,
                    v => v.map_range(0..255, 11..255),
                };
            }
            Light::CameoQSpot40RGBW_4Chan => {
                let (r, g, b) = color.tup();
                let alpha = state.alpha;

                fixture_channel!(dmx, this, 0) = (r as f32 / 255.0 * alpha as f32) as u8;
                fixture_channel!(dmx, this, 1) = (g as f32 / 255.0 * alpha as f32) as u8;
                fixture_channel!(dmx, this, 2) = (b as f32 / 255.0 * alpha as f32) as u8;
                // WHITE override.
                fixture_channel!(dmx, this, 3) =
                    if color.r == 255 && color.g == 255 && color.b == 255 {
                        255
                    } else {
                        0
                    };
            }
            Light::LightMaxxTripleDerbyHP => {
                //
                // 0: Hue
                // 1: Rotation
                // 2: Strobe
                // 3: Alpha
                //
                fixture_channel!(dmx, this, 0) =
                    (state.color.h.map_range(0.0..360.0, 0.0..255.0)) as u8;
                fixture_channel!(dmx, this, 1) = state.orientation.pan;
                fixture_channel!(dmx, this, 2) = state.strobe_speed;
                fixture_channel!(dmx, this, 3) = state.alpha;
            }
            Light::EuroLiteLEDMultiFX_10Chan => {
                let hue_u8 = (state.color.h.map_range(0.0..360.0, 0.0..255.0)) as u8;
                fixture_channel!(dmx, this, 0) = state.alpha;
                fixture_channel!(dmx, this, 1) = state.alpha;
                fixture_channel!(dmx, this, 2) = state.strobe_speed;
                fixture_channel!(dmx, this, 3) = hue_u8;
                fixture_channel!(dmx, this, 4) = hue_u8;
                fixture_channel!(dmx, this, 5) = state.orientation.pan;
                fixture_channel!(dmx, this, 6) = state.strobe_speed;
                fixture_channel!(dmx, this, 7) = state.orientation.tilt;
                fixture_channel!(dmx, this, 8) = state.focus;
                fixture_channel!(dmx, this, 9) = state.orientation.pan;
            }
            Light::TakeOffLogo => {
                fixture_channel!(dmx, this, 0) = state.alpha;
                fixture_channel!(dmx, this, 1) = color.r;
                fixture_channel!(dmx, this, 2) = color.g;
                fixture_channel!(dmx, this, 3) = color.b;
                // Re-use focus property as BPM control (0 keeps tempo unchanged).
                fixture_channel!(dmx, this, 4) = state.focus;
                fixture_channel!(dmx, this, 5) = state.focus;
            }
            Light::KuzeLEDPar => {
                fixture_channel!(dmx, this, 0) = color.r;
                fixture_channel!(dmx, this, 1) = color.g;
                fixture_channel!(dmx, this, 2) = color.b;
                fixture_channel!(dmx, this, 4) = 0;
                fixture_channel!(dmx, this, 5) = state.strobe_speed;
                fixture_channel!(dmx, this, 6) = 0;
                fixture_channel!(dmx, this, 7) = state.alpha;
            }
            Light::StairvilleWildWash648RGB_6Chan => {
                fixture_channel!(dmx, this, 0) = state.alpha;
                fixture_channel!(dmx, this, 1) = state.strobe_speed;
                fixture_channel!(dmx, this, 2) = color.r;
                fixture_channel!(dmx, this, 3) = color.g;
                fixture_channel!(dmx, this, 4) = color.b;
                fixture_channel!(dmx, this, 5) = 0;
            }
            Light::StairvilleLEDPar56_7Chan => {
                fixture_channel!(dmx, this, 0) = color.r;
                fixture_channel!(dmx, this, 1) = color.g;
                fixture_channel!(dmx, this, 2) = color.b;
                fixture_channel!(dmx, this, 3) = 0;
                fixture_channel!(dmx, this, 4) = match state.strobe_speed {
                    0 => 0,
                    v => v.map_range(0..255, 16..255),
                };
                fixture_channel!(dmx, this, 5) = 0;
                fixture_channel!(dmx, this, 6) = state.alpha;
            }
        }
    }

    pub fn state_from_dmx(&self, this: &Fixture, dmx: &[u8]) -> FixtureState {
        let resolved: ResolvedFixtureState = match self {
            Light::Generic3ChanNoAlpha => ResolvedFixtureState {
                color: RGBColor::parse_dmx(dmx, this.start_addr).into(),
                alpha: 255,
                orientation: FixtureOrientation::default(),
                strobe_speed: 0,
                focus: 0,
            },
            Light::GenericColorAlphaLight => {
                let hue = (fixture_channel!(dmx, this, 0) as f64).map_range(0.0..255.0, 0.0..360.0);
                let alpha = fixture_channel!(dmx, this, 1);

                ResolvedFixtureState {
                    color: HSVColor {
                        h: hue,
                        s: 1.0,
                        v: 1.0,
                    },
                    alpha,
                    orientation: FixtureOrientation::default(),
                    strobe_speed: 0,
                    focus: 0,
                }
            }
            Light::Generic4ChanWithAlpha => ResolvedFixtureState {
                color: RGBColor::parse_dmx(dmx, this.start_addr + 1).into(),
                alpha: fixture_channel!(dmx, this, 0),
                orientation: FixtureOrientation::default(),
                strobe_speed: 0,
                focus: 0,
            },
            Light::LEDPartyTCLSpot => {
                let rgb = RGBColor::parse_dmx(dmx, this.start_addr);
                let alpha = fixture_channel!(dmx, this, 3);

                ResolvedFixtureState {
                    color: rgb.into(),
                    alpha,
                    orientation: FixtureOrientation::default(),
                    strobe_speed: 0,
                    focus: 0,
                }
            }
            Light::AdjMegaHexPar => {
                let rgb = RGBColor::parse_dmx(dmx, this.start_addr);
                let alpha = fixture_channel!(dmx, this, 6);

                ResolvedFixtureState {
                    color: rgb.into(),
                    alpha,
                    orientation: FixtureOrientation::default(),
                    strobe_speed: 0,
                    focus: 0,
                }
            }
            Light::LiteCraftMiniParAT10 => {
                let rgb = RGBColor::parse_dmx(dmx, this.start_addr);
                let alpha = fixture_channel!(dmx, this, 7);

                ResolvedFixtureState {
                    color: rgb.into(),
                    alpha,
                    orientation: FixtureOrientation::default(),
                    strobe_speed: 0,
                    focus: 0,
                }
            }
            Light::VaryTechVP1 => ResolvedFixtureState {
                color: HSVColor::BLACK,
                alpha: fixture_channel!(dmx, this, 0),
                orientation: FixtureOrientation::default(),
                strobe_speed: fixture_channel!(dmx, this, 1),
                focus: 0,
            },
            Light::LightMaxxVegaSilentPar2Quad => {
                let rgb = RGBColor::parse_dmx(dmx, this.start_addr + 4);
                let alpha = fixture_channel!(dmx, this, 0);
                let strobe_speed = fixture_channel!(dmx, this, 1);

                ResolvedFixtureState {
                    color: rgb.into(),
                    alpha,
                    orientation: FixtureOrientation::default(),
                    strobe_speed,
                    focus: 0,
                }
            }
            Light::LEDPar64RGBSpot5Chan => {
                let rgb = RGBColor::parse_dmx(dmx, this.start_addr);
                let alpha = fixture_channel!(dmx, this, 3);
                let strobe_speed = match fixture_channel!(dmx, this, 4) {
                    0 => 0,
                    v if v < 11 => 0,
                    v => v.map_range(11..255, 0..255),
                };

                ResolvedFixtureState {
                    color: rgb.into(),
                    alpha,
                    orientation: FixtureOrientation::default(),
                    strobe_speed,
                    focus: 0,
                }
            }
            Light::CameoQSpot40RGBW_4Chan => {
                let white = fixture_channel!(dmx, this, 3);

                if white == 255 {
                    let r = fixture_channel!(dmx, this, 0);
                    let g = fixture_channel!(dmx, this, 1);
                    let b = fixture_channel!(dmx, this, 2);
                    let alpha = r.max(g).max(b);

                    ResolvedFixtureState {
                        color: RGBColor::white().into(),
                        alpha,
                        orientation: FixtureOrientation::default(),
                        strobe_speed: 0,
                        focus: 0,
                    }
                } else {
                    let rgb = RGBColor::parse_dmx(dmx, this.start_addr);

                    ResolvedFixtureState {
                        color: rgb.into(),
                        alpha: 255,
                        orientation: FixtureOrientation::default(),
                        strobe_speed: 0,
                        focus: 0,
                    }
                }
            }
            Light::LightMaxxTripleDerbyHP => {
                let hue = (fixture_channel!(dmx, this, 0) as f64).map_range(0.0..255.0, 0.0..360.0);
                let pan = fixture_channel!(dmx, this, 1);
                let strobe_speed = fixture_channel!(dmx, this, 2);
                let alpha = fixture_channel!(dmx, this, 3);

                ResolvedFixtureState {
                    color: HSVColor {
                        h: hue,
                        s: 1.0,
                        v: 1.0,
                    },
                    alpha,
                    orientation: FixtureOrientation { pan, tilt: 0 },
                    strobe_speed,
                    focus: 0,
                }
            }
            Light::EuroLiteLEDMultiFX_10Chan => {
                let hue = (fixture_channel!(dmx, this, 3) as f64).map_range(0.0..255.0, 0.0..360.0);
                let pan = fixture_channel!(dmx, this, 5);
                let tilt = fixture_channel!(dmx, this, 7);
                let alpha = fixture_channel!(dmx, this, 0);
                let strobe_speed = fixture_channel!(dmx, this, 2);
                let focus = fixture_channel!(dmx, this, 8);

                ResolvedFixtureState {
                    color: HSVColor {
                        h: hue,
                        s: 1.0,
                        v: 1.0,
                    },
                    alpha,
                    orientation: FixtureOrientation { pan, tilt },
                    strobe_speed,
                    focus,
                }
            }
            Light::TakeOffLogo => {
                let alpha = fixture_channel!(dmx, this, 0);
                let rgb = RGBColor::parse_dmx(dmx, this.start_addr + 1);
                let focus = fixture_channel!(dmx, this, 5);

                ResolvedFixtureState {
                    color: rgb.into(),
                    alpha,
                    orientation: FixtureOrientation::default(),
                    strobe_speed: 0,
                    focus,
                }
            }
            Light::KuzeLEDPar => {
                let rgb = RGBColor::parse_dmx(dmx, this.start_addr);
                let strobe_speed = fixture_channel!(dmx, this, 5);
                let alpha = fixture_channel!(dmx, this, 7);

                ResolvedFixtureState {
                    color: rgb.into(),
                    alpha,
                    orientation: FixtureOrientation::default(),
                    strobe_speed,
                    focus: 0,
                }
            }
            Light::StairvilleWildWash648RGB_6Chan => {
                let rgb = RGBColor::parse_dmx(dmx, this.start_addr + 2);
                let alpha = fixture_channel!(dmx, this, 0);
                let strobe_speed = fixture_channel!(dmx, this, 1);

                ResolvedFixtureState {
                    color: rgb.into(),
                    alpha,
                    orientation: FixtureOrientation::default(),
                    strobe_speed,
                    focus: 0,
                }
            }
            Light::StairvilleLEDPar56_7Chan => {
                let rgb = RGBColor::parse_dmx(dmx, this.start_addr);
                let strobe_speed = match fixture_channel!(dmx, this, 4) {
                    v if v < 16 => 0,
                    v => v.map_range(16..255, 0..255),
                };
                let alpha = fixture_channel!(dmx, this, 6);

                ResolvedFixtureState {
                    color: rgb.into(),
                    alpha,
                    orientation: FixtureOrientation::default(),
                    strobe_speed,
                    focus: 0,
                }
            }
        };
        resolved.into()
    }

    pub fn blackout(&self, _this: &Fixture, _state: &ResolvedFixtureState, _dmx: &mut [u8]) {}

    pub fn setup(&self, _this: &Fixture, _time: i32, _state: &ResolvedFixtureState, _dmx: &mut [u8]) {
        match self {
            Light::Generic3ChanNoAlpha => {}
            Light::GenericColorAlphaLight => {}
            Light::Generic4ChanWithAlpha => {}
            Light::LEDPartyTCLSpot => {}
            Light::AdjMegaHexPar => {}
            Light::LiteCraftMiniParAT10 => {}
            Light::VaryTechVP1 => {}
            Light::LightMaxxVegaSilentPar2Quad => {}
            Light::LEDPar64RGBSpot5Chan => {}
            Light::CameoQSpot40RGBW_4Chan => {}
            Light::LightMaxxTripleDerbyHP => {}
            Light::EuroLiteLEDMultiFX_10Chan => {}
            Light::TakeOffLogo => {}
            Light::KuzeLEDPar => {}
            Light::StairvilleWildWash648RGB_6Chan => {}
            Light::StairvilleLEDPar56_7Chan => {}
        }
    }
}
