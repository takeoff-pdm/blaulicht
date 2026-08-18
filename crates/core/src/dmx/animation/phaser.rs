use std::{collections::BTreeMap, f32::consts::PI};

use blaulicht_shared::{
    palette::Palette, AnimationSpecBodyPhaser, FixtureProperty, MathematicalBaseFunction,
    PhaserKind,
};

pub fn generate(
    self_: &AnimationSpecBodyPhaser,
    degrees_raw: f64,
    property: FixtureProperty,
    palettes: &BTreeMap<u8, Palette>,
) -> u16 {
    let degrees = degrees_raw.rem_euclid(360.0) as f32;
    debug_assert!((0.0..=360.0).contains(&degrees));

    let value = match &self_.kind {
        PhaserKind::Mathematical(mathematical_phaser) => {
            let min = mathematical_phaser
                .amplitude_min
                .resolve(palettes, property) as f32;
            let max = mathematical_phaser
                .amplitude_max
                .resolve(palettes, property) as f32;
            let range = max - min;

            let mut mathematical_phaser = mathematical_phaser.clone();
            mathematical_phaser.stretch_factor = 1.0;
            // println!("min={min}, max={max}, range={range}");

            match mathematical_phaser.base {
                MathematicalBaseFunction::Sin => {
                    let radians = degrees * mathematical_phaser.stretch_factor * (PI / 180.0);
                    let sine = radians.sin();

                    // Map from [-1,1] to [min,max]
                    ((sine + 1.0) / 2.0) * range + min
                }
                MathematicalBaseFunction::Cos => {
                    let radians = degrees * mathematical_phaser.stretch_factor * (PI / 180.0);
                    let cosine = radians.cos();

                    // Map from [-1,1] to [min,max]
                    ((cosine + 1.0) / 2.0) * range + min
                }
                MathematicalBaseFunction::Triangle => {
                    let degrees = (degrees * mathematical_phaser.stretch_factor).rem_euclid(360.0);
                    let phase = degrees / 360.0;

                    let triangle = 4.0 * (phase - 0.5).abs() - 1.0; // -1 to 1

                    // Map from [-1,1] to [min,max]
                    ((triangle + 1.0) / 2.0) * range + min
                }
                MathematicalBaseFunction::Square1_2 => {
                    let angle = (degrees * mathematical_phaser.stretch_factor).rem_euclid(360.0);

                    // High in first half, low in second half
                    if angle < 180.0 {
                        max
                    } else {
                        min
                    }
                }
                MathematicalBaseFunction::Square1_8 => {
                    let angle = (degrees * mathematical_phaser.stretch_factor).rem_euclid(360.0);

                    // High in first eigth, low in second half
                    if angle < 360.0 / 8.0 {
                        max
                    } else {
                        min
                    }
                }
                MathematicalBaseFunction::Square1_16 => {
                    let angle = (degrees * mathematical_phaser.stretch_factor).rem_euclid(360.0);

                    if angle < 360.0 / 16.0 {
                        max
                    } else {
                        min
                    }
                }
                MathematicalBaseFunction::Spike1_8 => {
                    let angle = (degrees * mathematical_phaser.stretch_factor).rem_euclid(360.0);
                    let spike_width = 360.0 / 8.0;

                    if angle < spike_width {
                        let phase = angle / spike_width;
                        let smooth = (phase * PI).sin();
                        smooth * range + min
                    } else {
                        min
                    }
                }
                MathematicalBaseFunction::ExpSpike1_8 => {
                    let angle = (degrees * mathematical_phaser.stretch_factor).rem_euclid(360.0);
                    let hold = 360.0 / 8.0;
                    let ramp = 360.0 - hold;

                    if angle < ramp {
                        let phase = angle / ramp;
                        let exp = (phase * 6.0 - 6.0).exp();
                        exp * range + min
                    } else {
                        max
                    }
                }

                MathematicalBaseFunction::Sawtooth => {
                    let angle = (degrees * mathematical_phaser.stretch_factor).rem_euclid(360.0);
                    let phase = angle / 360.0;

                    // Linear ramp from min to max
                    min + phase * range
                }
                MathematicalBaseFunction::EaseIn => {
                    let t = degrees / 360.0;

                    let percent = if t < 0.5 {
                        2.0 * t * t
                    } else {
                        1.0 - 2.0 * (1.0 - t) * (1.0 - t)
                    };

                    percent * range + min
                }
                MathematicalBaseFunction::EaseOut => {
                    let t = degrees / 360.0;

                    let percent = if t < 0.5 {
                        2.0 * t * t
                    } else {
                        1.0 - 2.0 * (1.0 - t) * (1.0 - t)
                    };

                    (1.0 - percent) * range + min
                }
                MathematicalBaseFunction::EaseInOut => {
                    let radians = std::f32::consts::PI * degrees / 360.0;
                    let v = radians.sin() * range + min;
                    v.clamp(0.0, 255.0)
                }
            }
        }
        // Keyframed phasers are not implemented yet. Keep the runtime alive
        // and leave the driven property at a deterministic safe value.
        PhaserKind::Keyframed(_keyframed_phaser) => 0.0,
    };

    debug_assert!((0.0..=u16::MAX as f32).contains(&value));
    value as u16
}
