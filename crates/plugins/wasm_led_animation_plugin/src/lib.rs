//! BPM-paced, deterministic patterns for five independent 99-pixel tubes.
use serde::{Deserialize, Serialize};
use std::f64::consts::TAU;

#[cfg(target_arch = "wasm32")]
mod plugin;

pub const PIXELS: usize = 99;
pub const TUBES: usize = 5;
pub const COUNT: usize = PIXELS * TUBES;
pub type Rgb = [f64; 3];
pub const NAMES: [&str; 10] = [
    "Aurora",
    "Ocean Drift",
    "Ember Breath",
    "Stardust",
    "Liquid Plasma",
    "Double Helix",
    "Prism Flow",
    "Meteor Rush",
    "Acid Chase",
    "Glitch Rave",
];
pub const DESCRIPTIONS: [&str; 10] = [
    "Slow teal, violet and green curtains",
    "Layered ocean waves and turquoise crests",
    "Warm amber breathing and glowing embers",
    "Pastel stars fading into darkness",
    "Interfering liquid color waves",
    "Two contrasting trails winding past each other",
    "Stretching spectral ribbons",
    "Fast comets with long fading tails",
    "Lime and magenta runners changing direction",
    "Fragmented flashes, color cuts and dark gaps",
];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub effect: u8,
    /// Logarithmic: 0 = 0.25x, 100 = 1x, 200 = 4x.
    pub speed: u8,
    pub brightness: u8,
    pub hue: u8,
    pub reversed: [bool; TUBES],
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            effect: 0,
            speed: 100,
            brightness: 255,
            hue: 0,
            reversed: [false; TUBES],
        }
    }
}
impl Settings {
    pub fn sanitize(&mut self) {
        self.effect = self.effect.min(9);
        self.speed = self.speed.min(200);
    }
    pub fn load(raw: &str) -> Self {
        let mut settings: Self = serde_json::from_str(raw).unwrap_or_default();
        settings.sanitize();
        settings
    }
    pub fn multiplier(&self) -> f64 {
        2.0_f64.powf((self.speed.min(200) as f64 - 100.0) / 50.0)
    }
}
pub fn tempo(bpm: f64) -> (f64, bool) {
    if bpm.is_finite() && bpm > 0.0 {
        (bpm, false)
    } else {
        (120.0, true)
    }
}
fn wave(x: f64) -> f64 {
    0.5 + 0.5 * (TAU * x).sin()
}
fn hash(n: u64) -> f64 {
    let mut x = n.wrapping_add(0x9e3779b97f4a7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d049bb133111eb);
    ((x ^ (x >> 31)) >> 11) as f64 / ((1u64 << 53) as f64)
}
fn glow(distance: f64, width: f64) -> f64 {
    (-(distance / width).powi(2)).exp()
}
fn palette(t: f64, stops: &[Rgb]) -> Rgb {
    let q = t.clamp(0.0, 0.999999) * (stops.len() - 1) as f64;
    let i = q.floor() as usize;
    std::array::from_fn(|c| stops[i][c] + (stops[i + 1][c] - stops[i][c]) * q.fract())
}
pub fn hsv(h: f64, s: f64, v: f64) -> Rgb {
    let h = h.rem_euclid(360.0) / 60.0;
    let c = v * s;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let rgb = match h as u8 {
        0 => [c, x, 0.0],
        1 => [x, c, 0.0],
        2 => [0.0, c, x],
        3 => [0.0, x, c],
        4 => [x, 0.0, c],
        _ => [c, 0.0, x],
    };
    rgb.map(|a| a + v - c)
}
pub fn rgb_hsv(rgb: Rgb) -> (f64, f64, f64) {
    let [r, g, b] = rgb;
    let hi = r.max(g).max(b);
    let lo = r.min(g).min(b);
    let d = hi - lo;
    let h = if d < 1e-12 {
        0.0
    } else if hi == r {
        60.0 * ((g - b) / d).rem_euclid(6.0)
    } else if hi == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    (h, if hi > 0.0 { d / hi } else { 0.0 }, hi)
}

/// Linear tube coordinate, not a rectangular installation coordinate.
pub fn pixel(effect: u8, tube: usize, x: f64, beats: f64) -> Rgb {
    let arm = tube as f64;
    let offset = arm * 0.173;
    match effect {
        0 => {
            let t = beats / 32.0;
            let n = 0.55 * wave(x * 1.3 - t + offset)
                + 0.3 * wave(x * 3.1 + t * 0.7 + offset)
                + 0.15 * wave(x * 6.0 - t * 0.4);
            palette(
                n,
                &[
                    [0.01, 0.02, 0.1],
                    [0.16, 0.01, 0.45],
                    [0.0, 0.55, 0.45],
                    [0.15, 0.85, 0.35],
                ],
            )
            .map(|v| v * (0.3 + 0.7 * n))
        }
        1 => {
            let t = beats / 16.0;
            let n = 0.55 * wave(x * 2.0 - t + offset)
                + 0.3 * wave(x * 3.7 + t * 0.8)
                + 0.15 * wave(x * 7.0 - t * 1.3 + offset);
            palette(
                n,
                &[
                    [0.0, 0.005, 0.03],
                    [0.0, 0.06, 0.35],
                    [0.0, 0.5, 0.65],
                    [0.4, 0.95, 0.9],
                ],
            )
        }
        2 => {
            let breath = 0.2 + 0.8 * wave(beats / 8.0 + offset);
            let ember = wave(x * 3.2 + beats / 19.0 + offset) * wave(x * 7.1 - beats / 23.0);
            hsv(8.0 + 32.0 * ember, 0.95, breath * (0.12 + 0.88 * ember))
        }
        3 => {
            let mut out = [0.0; 3];
            let epoch = (beats / 2.0).floor() as u64;
            for ago in 0..4 {
                if epoch < ago {
                    continue;
                }
                let birth = epoch - ago;
                let age = beats / 2.0 - birth as f64;
                for star in 0..3 {
                    let seed = birth
                        .wrapping_mul(79)
                        .wrapping_add(tube as u64 * 7919 + star * 113);
                    let position = hash(seed);
                    let envelope = (std::f64::consts::PI * (age / 4.0)).sin().max(0.0).powi(2);
                    let rgb = hsv(
                        hash(seed + 1) * 360.0,
                        0.25 + hash(seed + 2) * 0.4,
                        envelope * glow(x - position, 0.012 + hash(seed + 3) * 0.012),
                    );
                    for c in 0..3 {
                        out[c] = (out[c] + rgb[c]).min(1.0);
                    }
                }
            }
            out
        }
        4 => {
            let t = beats / 8.0;
            let n = (TAU * (x * 2.3 + t + offset)).sin()
                + (TAU * (x * 4.1 - t * 0.71)).sin()
                + (TAU * (x * 1.7 + t * 0.43 + offset)).cos();
            hsv(
                220.0 + n * 85.0 + t * 35.0,
                0.9,
                0.25 + 0.75 * wave(n * 0.19),
            )
        }
        5 => {
            let t = beats / 4.0 + offset;
            let a = glow(x - wave(t), 0.065);
            let b = glow(x - wave(t + 0.5), 0.065);
            [
                0.95 * b + 0.05 * a,
                0.85 * a + 0.04 * b,
                (a + b * 0.7).min(1.0),
            ]
        }
        6 => {
            let t = beats / 8.0;
            let ribbon = x * 2.2 - t + 0.18 * (TAU * (x + t * 0.4 + offset)).sin() + offset;
            hsv(ribbon * 360.0, 0.88, 0.08 + 0.92 * wave(ribbon).powi(3))
        }
        7 => {
            let travel = (beats / 2.0 + offset).rem_euclid(1.0) * 1.5;
            let distance = travel - x;
            let tail = if distance >= 0.0 {
                (-distance / 0.16).exp()
            } else {
                glow(distance, 0.018)
            };
            let rgb = hsv(190.0 + arm * 24.0, 0.85, tail);
            let head = glow(distance, 0.018);
            rgb.map(|v| (v + head * 0.65).min(1.0))
        }
        8 => {
            let section = (beats / 4.0).floor() as u64;
            let t = (beats / 4.0).fract() * 8.0;
            let direction = if section % 2 == 0 { 1.0 } else { -1.0 };
            let segment = (x * 12.0 - direction * t + arm * 2.0).rem_euclid(6.0);
            let on = if segment.rem_euclid(3.0) < 1.5 {
                1.0
            } else {
                0.015
            };
            hsv(if segment < 3.0 { 95.0 } else { 310.0 }, 1.0, on)
        }
        _ => {
            let step = (beats * 4.0).floor() as u64;
            let block = (x * 11.0).floor() as u64;
            let seed = step
                .wrapping_mul(65537)
                .wrapping_add(tube as u64 * 131 + block * 17);
            let active = hash(seed) > 0.64 && (beats * 4.0).fract() < 0.62;
            let colors = [95.0, 185.0, 285.0, 325.0];
            hsv(
                colors[(hash(seed + 1) * 4.0) as usize],
                if hash(seed + 2) > 0.9 { 0.1 } else { 1.0 },
                if active { 1.0 } else { 0.0 },
            )
        }
    }
}

pub struct Engine {
    pub settings: Settings,
    pub beats: f64,
    active: u8,
    last: Vec<Rgb>,
    fade_from: Option<Vec<Rgb>>,
    fade_seconds: f64,
}
impl Default for Engine {
    fn default() -> Self {
        Self {
            settings: Settings::default(),
            beats: 0.0,
            active: 0,
            last: vec![[0.0; 3]; COUNT],
            fade_from: None,
            fade_seconds: 0.0,
        }
    }
}
impl Engine {
    pub fn frame(
        &mut self,
        seconds: f64,
        bpm: f64,
        host_speed: f64,
        paused: bool,
        count: usize,
    ) -> Option<Vec<Rgb>> {
        if paused || count != COUNT {
            return None;
        }
        self.settings.sanitize();
        if self.active != self.settings.effect {
            self.fade_from = Some(self.last.clone());
            self.fade_seconds = 0.0;
            self.active = self.settings.effect;
            self.beats = 0.0;
        }
        let dt = if seconds.is_finite() {
            seconds.max(0.0)
        } else {
            0.0
        };
        let rate = if host_speed.is_finite() {
            host_speed.max(0.0)
        } else {
            1.0
        };
        self.beats += dt * tempo(bpm).0 / 60.0 * self.settings.multiplier() * rate;
        self.fade_seconds += dt;
        let mix = (self.fade_seconds / 0.4).min(1.0);
        for i in 0..COUNT {
            let tube = i / PIXELS;
            let p = i % PIXELS;
            let x = if self.settings.reversed[tube] {
                PIXELS - 1 - p
            } else {
                p
            } as f64
                / (PIXELS - 1) as f64;
            let raw = pixel(self.active, tube, x, self.beats);
            let (h, s, v) = rgb_hsv(raw);
            let rgb = hsv(
                h + self.settings.hue as f64 / 255.0 * 360.0,
                s,
                v * self.settings.brightness as f64 / 255.0,
            );
            self.last[i] = match &self.fade_from {
                Some(previous) => {
                    std::array::from_fn(|c| previous[i][c] * (1.0 - mix) + rgb[c] * mix)
                }
                None => rgb,
            }
            .map(|v| v.clamp(0.0, 1.0));
        }
        if mix >= 1.0 {
            self.fade_from = None;
        }
        Some(self.last.clone())
    }
}

#[cfg(test)]
mod tests;
