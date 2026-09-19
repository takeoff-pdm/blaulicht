use crate::{
    HSVColor,
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
    // Martin MAC 250 Entour, 16-bit Extended personality (PSET = 16 EX), 18 channels.
    //  0: Shutter / strobe / reset / lamp (0-19 closed | 20-49 open | 50-72 strobe fast->slow
    //     | 208-217 reset | 228-237 lamp on | 248-255 lamp off)
    //  1: Dimmer
    //  2: Dimmer fine
    //  3: Colour wheel (0-143 continuous | 156-207 stepped | 208-245 rotation | 246-255 random)
    //  4: Colour fine
    //  5: Rotating gobo select (0-4 open)
    //  6: Gobo rotation
    //  7: Gobo rotation fine
    //  8: Static gobo wheel (0-7 open)
    //  9: Focus
    // 10: Focus fine
    // 11: Prism / macros (0-19 off)
    // 12: Pan (128 neutral)
    // 13: Pan fine
    // 14: Tilt (128 neutral)
    // 15: Tilt fine
    // 16: Pan/tilt speed (0-2 tracking)
    // 17: Effects speed (0-2 tracking)
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

/// MAC 250 Entour colour wheel: DMX value (centre of the stepped-scroll range on
/// the colour channel) and the approximate hue of the filter. `None` is white.
const MAC250E_WHEEL: [(u8, Option<f64>); 12] = [
    (157, None),        // White
    (185, Some(0.0)),   // Red 301
    (197, Some(30.0)),  // Orange 306
    (165, Some(55.0)),  // Yellow 603
    (177, Some(125.0)), // Green 206
    (201, Some(150.0)), // Dark green
    (193, Some(200.0)), // Blue 101 (light)
    (181, Some(220.0)), // Blue 108
    (169, Some(240.0)), // Blue 104
    (205, Some(275.0)), // Purple 502
    (189, Some(305.0)), // Magenta 507
    (173, Some(335.0)), // Pink 312
];

const MAC250E_WHITE: u8 = MAC250E_WHEEL[0].0;

fn hue_distance(a: f64, b: f64) -> f64 {
    let d = (a - b).rem_euclid(360.0);
    d.min(360.0 - d)
}

/// Picks the wheel filter closest to `color`; desaturated or black colours map to white.
fn mac250e_nearest_wheel_color(color: &HSVColor) -> u8 {
    if color.s < 0.15 || color.v < 0.05 {
        return MAC250E_WHITE;
    }
    MAC250E_WHEEL
        .iter()
        .filter_map(|(dmx, hue)| hue.map(|h| (*dmx, hue_distance(h, color.h))))
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(dmx, _)| dmx)
        .unwrap_or(MAC250E_WHITE)
}

/// Inverse of [`mac250e_nearest_wheel_color`]: the colour of the wheel entry whose
/// DMX value is closest to `dmx`.
fn mac250e_wheel_color_from_dmx(dmx: u8) -> HSVColor {
    let (_, hue) = MAC250E_WHEEL
        .iter()
        .min_by_key(|(v, _)| (*v as i16 - dmx as i16).abs())
        .copied()
        .unwrap_or((MAC250E_WHITE, None));
    match hue {
        None => HSVColor {
            h: 0.0,
            s: 0.0,
            v: 1.0,
        },
        Some(h) => HSVColor { h, s: 1.0, v: 1.0 },
    }
}

/// Strobe channel: 50-72 is strobe fast -> slow. `speed` 1 = slowest (72), 255 = fastest (50).
fn mac250e_strobe_to_dmx(speed: u8) -> u8 {
    match speed {
        0 => 30, // shutter open
        v => 72 - crate::fixture::map_range_u8(v, (1, 255), (0, 22)),
    }
}

fn mac250e_strobe_from_dmx(dmx: u8) -> u8 {
    match dmx {
        50..=72 => crate::fixture::map_range_u8(72 - dmx, (0, 22), (1, 255)),
        _ => 0,
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
        match self {
            MovingHead::MartinMac250E => {
                // Shutter / strobe. Never emit >= 73: those ranges pulse, reset or kill the lamp.
                fixture_channel!(dmx, this, 0) = mac250e_strobe_to_dmx(state.strobe_speed);
                fixture_channel!(dmx, this, 1) = state.alpha;
                fixture_channel!(dmx, this, 2) = 0;
                fixture_channel!(dmx, this, 3) = mac250e_nearest_wheel_color(&state.color);
                fixture_channel!(dmx, this, 4) = 0;
                // Gobos open, no rotation.
                fixture_channel!(dmx, this, 5) = 0;
                fixture_channel!(dmx, this, 6) = 0;
                fixture_channel!(dmx, this, 7) = 0;
                fixture_channel!(dmx, this, 8) = 0;
                fixture_channel!(dmx, this, 9) = state.focus;
                fixture_channel!(dmx, this, 10) = 0;
                // Prism off.
                fixture_channel!(dmx, this, 11) = 0;
                fixture_channel!(dmx, this, 12) = state.orientation.pan;
                fixture_channel!(dmx, this, 13) = 0;
                fixture_channel!(dmx, this, 14) = state.orientation.tilt;
                fixture_channel!(dmx, this, 15) = 0;
                // Pan/tilt and effect speed: tracking.
                fixture_channel!(dmx, this, 16) = 0;
                fixture_channel!(dmx, this, 17) = 0;
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
    }

    pub fn state_from_dmx(&self, this: &Fixture, dmx: &[u8]) -> FixtureState {
        let resolved: ResolvedFixtureState = match self {
            MovingHead::MartinMac250E => ResolvedFixtureState {
                color: mac250e_wheel_color_from_dmx(fixture_channel!(dmx, this, 3)),
                alpha: fixture_channel!(dmx, this, 1),
                orientation: FixtureOrientation {
                    pan: fixture_channel!(dmx, this, 12),
                    tilt: fixture_channel!(dmx, this, 14),
                },
                strobe_speed: mac250e_strobe_from_dmx(fixture_channel!(dmx, this, 0)),
                focus: fixture_channel!(dmx, this, 9),
            },
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
                // Bring every channel into a sane state first.
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

                // Lamp on (228-237) during the first seconds, afterwards `write` already
                // left the shutter open.
                if time <= 5000 {
                    fixture_channel!(dmx, this, 0) = 232;
                }
            }
            MovingHead::VaryTechHeroSpot60 => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture::FixtureType;

    fn fixture() -> Fixture {
        Fixture::new(
            0,
            400,
            "mac".to_string(),
            FixtureType::from(MovingHead::MartinMac250E),
        )
    }

    #[test]
    fn mac250e_shutter_open_and_effects_off() {
        let fix = fixture();
        let state = ResolvedFixtureState {
            color: HSVColor {
                h: 5.0,
                s: 1.0,
                v: 1.0,
            },
            alpha: 255,
            orientation: FixtureOrientation { pan: 10, tilt: 200 },
            strobe_speed: 0,
            focus: 77,
        };
        let mut dmx = [0u8; 513];
        fix.write(&state, &mut dmx);

        assert!(
            (20..=49).contains(&dmx[400]),
            "shutter must be open, got {}",
            dmx[400]
        );
        assert_eq!(dmx[401], 255);
        assert_eq!(dmx[403], 185, "red 301");
        assert_eq!(dmx[405], 0, "rotating gobo open");
        assert_eq!(dmx[408], 0, "static gobo open");
        assert_eq!(dmx[409], 77, "focus");
        assert_eq!(dmx[411], 0, "prism off");
        assert_eq!(dmx[412], 10);
        assert_eq!(dmx[414], 200);
        assert_eq!(dmx[418], 0, "no bleed into the next head");

        let back: ResolvedFixtureState = fix
            .type_
            .state_from_dmx(&fix, &dmx)
            .resolve(&Default::default());
        assert_eq!(back.alpha, 255);
        assert_eq!(back.strobe_speed, 0);
        assert_eq!(back.orientation.pan, 10);
        assert_eq!(back.orientation.tilt, 200);
        assert_eq!(back.focus, 77);
        assert_eq!(back.color.h as u16, 0);
    }

    #[test]
    fn mac250e_strobe_stays_in_strobe_range() {
        let fix = fixture();
        let mut dmx = [0u8; 513];
        for speed in [1u8, 128, 255] {
            let state = ResolvedFixtureState {
                strobe_speed: speed,
                ..Default::default()
            };
            fix.write(&state, &mut dmx);
            assert!(
                (50..=72).contains(&dmx[400]),
                "speed {speed} -> {}",
                dmx[400]
            );
        }
        assert_eq!(mac250e_strobe_to_dmx(255), 50, "fastest");
        assert_eq!(mac250e_strobe_to_dmx(1), 72, "slowest");
        assert_eq!(mac250e_strobe_from_dmx(50), 255);
        assert_eq!(mac250e_strobe_from_dmx(72), 1);
        assert_eq!(mac250e_strobe_from_dmx(30), 0);
    }

    #[test]
    fn mac250e_wheel_selection() {
        let white = HSVColor {
            h: 120.0,
            s: 0.0,
            v: 1.0,
        };
        assert_eq!(mac250e_nearest_wheel_color(&white), 157);
        let green = HSVColor {
            h: 120.0,
            s: 1.0,
            v: 1.0,
        };
        assert_eq!(mac250e_nearest_wheel_color(&green), 177);
        let red_wrap = HSVColor {
            h: 358.0,
            s: 1.0,
            v: 1.0,
        };
        assert_eq!(mac250e_nearest_wheel_color(&red_wrap), 185);
        let blue = HSVColor {
            h: 240.0,
            s: 1.0,
            v: 1.0,
        };
        assert_eq!(mac250e_nearest_wheel_color(&blue), 169);
    }
}
