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
#[allow(nonstandard_style)]
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
    //
    // Martin Rush MH 6 Wash (RGBW LED wash with zoom), its only DMX mode, 14 channels.
    // Source: the QLC+ fixture definition (Martin-Rush-MH6-Wash.qxf), which matches the
    // manual's channel positions (RGBW 4-7, zoom 9, pan 10, tilt 12, control 14, 1-based).
    //  0: Shutter / strobe (0-7 closed | 8-15 open | 16-131 strobe slow->fast | 132-139 open
    //     | 140-231 pulse effects | 232-239 open | 240-247 random | 248-255 open)
    //  1: Dimmer
    //  2: Dimmer fine
    //  3: Red
    //  4: Green
    //  5: Blue
    //  6: White
    //  7: Colour presets (0-10 off, so RGBW mixing applies | 11-190 presets | 193-255 scroll/random)
    //  8: Zoom
    //  9: Pan (128 neutral)
    // 10: Pan fine
    // 11: Tilt (128 neutral)
    // 12: Tilt fine
    // 13: Fixture control (0-9 no function | 10-14 reset | 55-59 enable calibration
    //     | 230-249 store/reset calibrations). Always 0 here.
    //
    MartinRushMH6Wash_14Chan,
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

/// Rush MH 6 shutter: 16-131 is strobe slow -> fast. `speed` 1 = slowest, 255 = fastest.
/// Speed 0 is shutter open (8-15); the pulse and random ranges above 131 are never emitted.
fn rush_mh6_strobe_to_dmx(speed: u8) -> u8 {
    match speed {
        0 => 12,
        v => crate::fixture::map_range_u8(v, (1, 255), (16, 131)),
    }
}

fn rush_mh6_strobe_from_dmx(dmx: u8) -> u8 {
    match dmx {
        16..=131 => crate::fixture::map_range_u8(dmx, (16, 131), (1, 255)),
        _ => 0,
    }
}

/// Below this HSV value a colour counts as unset and the head runs on its white LED.
const RUSH_MH6_BLACK_V: f64 = 0.05;

/// Splits an RGB colour into RGBW: the grey part `min(r, g, b)` goes to the white LED.
/// [`rush_mh6_rgb_from_rgbw`] reverses it exactly.
fn rush_mh6_rgbw(color: RGBColor) -> [u8; 4] {
    let w = color.r.min(color.g).min(color.b);
    [color.r - w, color.g - w, color.b - w, w]
}

fn rush_mh6_rgb_from_rgbw(r: u8, g: u8, b: u8, w: u8) -> RGBColor {
    RGBColor {
        r: r.saturating_add(w),
        g: g.saturating_add(w),
        b: b.saturating_add(w),
    }
}

impl MovingHead {
    pub fn footprint(&self) -> usize {
        match self {
            MovingHead::MartinMac250E => 18,
            MovingHead::VaryTechHeroSpot60 => 8,
            MovingHead::MartinRushMH6Wash_14Chan => 14,
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
            MovingHead::MartinRushMH6Wash_14Chan => {
                // Black means "no colour set": light the white LED so dimmer and
                // position alone produce light, like the MAC 250 colour wheel.
                let [r, g, b, w] = if state.color.v < RUSH_MH6_BLACK_V {
                    [0, 0, 0, 255]
                } else {
                    rush_mh6_rgbw(state.color.into())
                };
                fixture_channel!(dmx, this, 0) = rush_mh6_strobe_to_dmx(state.strobe_speed);
                fixture_channel!(dmx, this, 1) = state.alpha;
                fixture_channel!(dmx, this, 2) = 0;
                fixture_channel!(dmx, this, 3) = r;
                fixture_channel!(dmx, this, 4) = g;
                fixture_channel!(dmx, this, 5) = b;
                fixture_channel!(dmx, this, 6) = w;
                // Colour presets off, otherwise they override the RGBW mix.
                fixture_channel!(dmx, this, 7) = 0;
                fixture_channel!(dmx, this, 8) = state.focus;
                fixture_channel!(dmx, this, 9) = state.orientation.pan;
                fixture_channel!(dmx, this, 10) = 0;
                fixture_channel!(dmx, this, 11) = state.orientation.tilt;
                fixture_channel!(dmx, this, 12) = 0;
                // Fixture control: no function. Other values reset or recalibrate the head.
                fixture_channel!(dmx, this, 13) = 0;
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
            MovingHead::MartinRushMH6Wash_14Chan => ResolvedFixtureState {
                color: rush_mh6_rgb_from_rgbw(
                    fixture_channel!(dmx, this, 3),
                    fixture_channel!(dmx, this, 4),
                    fixture_channel!(dmx, this, 5),
                    fixture_channel!(dmx, this, 6),
                )
                .into(),
                alpha: fixture_channel!(dmx, this, 1),
                orientation: FixtureOrientation {
                    pan: fixture_channel!(dmx, this, 9),
                    tilt: fixture_channel!(dmx, this, 11),
                },
                strobe_speed: rush_mh6_strobe_from_dmx(fixture_channel!(dmx, this, 0)),
                focus: fixture_channel!(dmx, this, 8),
            },
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
            // LED head: no lamp to strike. Park it centred with presets and control channel off.
            MovingHead::MartinRushMH6Wash_14Chan => self.write(
                this,
                &ResolvedFixtureState {
                    orientation: FixtureOrientation {
                        pan: 127,
                        tilt: 127,
                    },
                    ..state.clone()
                },
                dmx,
            ),
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

    fn rush_mh6() -> Fixture {
        Fixture::new(
            0,
            400,
            "rush".to_string(),
            FixtureType::from(MovingHead::MartinRushMH6Wash_14Chan),
        )
    }

    #[test]
    fn rush_mh6_writes_rgbw_and_keeps_presets_and_control_off() {
        let fix = rush_mh6();
        assert_eq!(fix.type_.footprint(), 14);
        let state = ResolvedFixtureState {
            // Pastel orange: RGB (255, 191, 127), whose grey part 127 goes to the white LED.
            color: RGBColor {
                r: 255,
                g: 191,
                b: 127,
            }
            .into(),
            alpha: 200,
            orientation: FixtureOrientation { pan: 10, tilt: 200 },
            strobe_speed: 0,
            focus: 77,
        };
        let mut dmx = [0xAAu8; 513];
        fix.write(&state, &mut dmx);

        assert!(
            (8..=15).contains(&dmx[400]),
            "shutter open, got {}",
            dmx[400]
        );
        assert_eq!(dmx[401], 200, "dimmer");
        assert_eq!(dmx[402], 0, "dimmer fine");
        let [r, g, b, w] = [dmx[403], dmx[404], dmx[405], dmx[406]];
        assert_eq!(w, 127, "white takes the grey part");
        assert_eq!((r, g, b), (128, 64, 0), "rgb minus white");
        assert_eq!(dmx[407], 0, "colour presets off");
        assert_eq!(dmx[408], 77, "zoom");
        assert_eq!(dmx[409], 10, "pan");
        assert_eq!(dmx[410], 0);
        assert_eq!(dmx[411], 200, "tilt");
        assert_eq!(dmx[412], 0);
        assert_eq!(dmx[413], 0, "fixture control: no function");
        assert_eq!(dmx[414], 0xAA, "no bleed into the next fixture");
        assert_eq!(dmx[399], 0xAA, "no bleed into the previous fixture");

        let back: ResolvedFixtureState = fix
            .type_
            .state_from_dmx(&fix, &dmx)
            .resolve(&Default::default());
        let rgb: RGBColor = back.color.into();
        assert_eq!((rgb.r, rgb.g, rgb.b), (255, 191, 127));
        assert_eq!(back.alpha, 200);
        assert_eq!(back.orientation.pan, 10);
        assert_eq!(back.orientation.tilt, 200);
        assert_eq!(back.strobe_speed, 0);
        assert_eq!(back.focus, 77);
    }

    #[test]
    fn rush_mh6_black_or_unset_colour_lights_the_white_led() {
        let fix = rush_mh6();
        let mut dmx = [0u8; 513];
        // Default state: colour unset (black), only dimmer and position set.
        let state = ResolvedFixtureState {
            alpha: 255,
            orientation: FixtureOrientation { pan: 40, tilt: 90 },
            ..Default::default()
        };
        fix.write(&state, &mut dmx);
        assert_eq!(&dmx[403..407], &[0, 0, 0, 255], "RGBW");
        assert_eq!(dmx[401], 255, "dimmer");

        // Just above the threshold the real colour is used again.
        let dim_red = ResolvedFixtureState {
            color: HSVColor {
                h: 0.0,
                s: 1.0,
                v: 0.1,
            },
            alpha: 255,
            ..Default::default()
        };
        fix.write(&dim_red, &mut dmx);
        assert_eq!(dmx[406], 0, "no white for a dim saturated colour");
        assert!(dmx[403] > 0, "red");
    }

    #[test]
    fn rush_mh6_rgbw_split_is_lossless() {
        for (r, g, b) in [
            (0, 0, 0),
            (255, 255, 255),
            (255, 0, 0),
            (12, 200, 99),
            (0, 180, 220),
        ] {
            let [rr, gg, bb, w] = rush_mh6_rgbw(RGBColor { r, g, b });
            assert_eq!(rr.min(gg).min(bb), 0, "one of r, g, b is always 0");
            let back = rush_mh6_rgb_from_rgbw(rr, gg, bb, w);
            assert_eq!((back.r, back.g, back.b), (r, g, b));
        }
        assert_eq!(
            rush_mh6_rgbw(RGBColor {
                r: 255,
                g: 255,
                b: 255
            }),
            [0, 0, 0, 255]
        );
    }

    #[test]
    fn rush_mh6_strobe_stays_in_strobe_range() {
        assert_eq!(rush_mh6_strobe_to_dmx(1), 16, "slowest");
        assert_eq!(rush_mh6_strobe_to_dmx(255), 131, "fastest");
        for speed in 1..=255u8 {
            let dmx = rush_mh6_strobe_to_dmx(speed);
            assert!((16..=131).contains(&dmx), "speed {speed} -> {dmx}");
            let back = rush_mh6_strobe_from_dmx(dmx);
            assert!(
                back.abs_diff(speed) <= 3,
                "speed {speed} -> {dmx} -> {back}"
            );
        }
        assert_eq!(rush_mh6_strobe_from_dmx(12), 0);
        assert_eq!(
            rush_mh6_strobe_from_dmx(140),
            0,
            "pulse effects are not strobe"
        );
        assert_eq!(
            rush_mh6_strobe_from_dmx(244),
            0,
            "random strobe is not decoded"
        );
    }
}
