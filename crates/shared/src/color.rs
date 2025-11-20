use bincode::{Decode, Encode};
use color_space::{FromColor, FromRgb};
use core::{f32, f64};
use map_range::MapRange;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Copy, Default, Encode, Decode)]
pub struct HSVColor {
    pub h: f64,
    pub s: f64,
    pub v: f64,
}

impl HSVColor {
    pub const BLACK: Self = Self {
        h: 0.0,
        s: 0.0,
        v: 0.0,
    };
}

impl From<RGBColor> for HSVColor {
    fn from(value: RGBColor) -> Self {
        let converted = color_space::Hsv::from_rgb(&color_space::Rgb {
            r: value.r as f64,
            g: value.g as f64,
            b: value.b as f64,
        });

        Self {
            h: converted.h,
            s: converted.s,
            v: converted.v,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RGBColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl RGBColor {
    pub fn parse_dmx(dmx: &[u8], start_addr: usize) -> Self {
        Self {
            r: dmx[start_addr + 0],
            g: dmx[start_addr + 1],
            b: dmx[start_addr + 2],
        }
    }

    pub fn with_alpha(&self, alpha: u8) -> Self {
        let alpha_percent = (alpha as f64).map_range(0.0..255.0, 0.0..1.0);

        Self {
            r: (self.r as f64 * alpha_percent) as u8,
            g: (self.g as f64 * alpha_percent) as u8,
            b: (self.b as f64 * alpha_percent) as u8,
        }
    }
}

impl From<HSVColor> for RGBColor {
    fn from(value: HSVColor) -> Self {
        let converted = color_space::Rgb::from_color(&color_space::Hsv {
            h: value.h,
            s: value.s,
            v: value.v,
        });

        Self {
            r: converted.r as u8,
            g: converted.g as u8,
            b: converted.b as u8,
        }
    }
}

impl From<(u8, u8, u8)> for RGBColor {
    fn from(value: (u8, u8, u8)) -> Self {
        Self {
            r: value.0,
            g: value.1,
            b: value.2,
        }
    }
}

// impl Default for RGBColor {
//     fn default() -> Self {
//         Self { r: 0, g: 0, b: 0 }
//     }
// }

impl RGBColor {
    pub fn tup(&self) -> (u8, u8, u8) {
        (self.r, self.g, self.b)
    }

    pub fn white() -> Self {
        (255, 255, 255).into()
    }
}

/// Uses input ranging from 0 to 360.
pub fn hsv_to_rgb(h: u16, s: f32, v: f32) -> RGBColor {
    // Catch white.
    // if h == -1 {
    //     return (255, 255, 255).into();
    // }

    // Normalize hue to [0, 360)
    // let mut h = h % 360;
    // if h < 0 {
    //     h += 360;
    // }

    let c = v * s;
    let h_prime = h as f32 / 60.0;
    let x = c * (1.0 - ((h_prime % 2.0) - 1.0).abs());
    let m = v - c;

    let (rf, gf, bf) = match h {
        0..=59 => (c, x, 0.0),
        60..=119 => (x, c, 0.0),
        120..=179 => (0.0, c, x),
        180..=239 => (0.0, x, c),
        240..=299 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };

    let r = ((rf + m) * 255.0).round() as u8;
    let g = ((gf + m) * 255.0).round() as u8;
    let b = ((bf + m) * 255.0).round() as u8;

    (r, g, b).into()
}

// #[macro_export]
// macro_rules! colorize {
//     ($rgb_triple:expr, $dmx:expr, $dmx_start:expr) => {
//         $dmx[$dmx_start + 0] = $rgb_triple.0;
//         $dmx[$dmx_start + 1] = $rgb_triple.1;
//         $dmx[$dmx_start + 2] = $rgb_triple.2;
//     };
// }

// pub use colorize;
