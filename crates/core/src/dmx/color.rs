use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Default for Color {
    fn default() -> Self {
        Self { r: 0, g: 0, b: 0 }
    }
}

impl Color {
    pub fn tup(&self) -> (u8, u8, u8) {
        (self.r, self.g, self.b)
    }

    pub fn white() -> Self {
        (255, 255, 255).into()
    }
}

impl From<(u8, u8, u8)> for Color {
    fn from(value: (u8, u8, u8)) -> Self {
        Self {
            r: value.0,
            g: value.1,
            b: value.2,
        }
    }
}

/// Uses input ranging from 0 to 360.
pub fn hsv_to_rgb(h: isize) -> Color {
    // Catch white.
    if h == -1 {
        return (255, 255, 255).into();
    }

    // Normalize hue to [0, 360)
    // let mut h = h % 360;
    // if h < 0 {
    //     h += 360;
    // }

    let s = 1.0f32;
    let v = 1.0f32;
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
