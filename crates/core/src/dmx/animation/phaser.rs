use std::f32::consts::PI;

use blaulicht_shared::{AnimationSpecBodyPhaser, MathematicalBaseFunction, PhaserKind};

pub fn generate(self_: &AnimationSpecBodyPhaser, degrees_raw: u64) -> u16 {
    let degrees = (degrees_raw % 360) as f32;
    debug_assert!((0.0..=360.0).contains(&degrees));

    let value = match &self_.kind {
        PhaserKind::Mathematical(mathematical_phaser) => {
            let min = mathematical_phaser.amplitude_min as f32;
            let max = mathematical_phaser.amplitude_max as f32;
            let range = max - min;

            let mut mathematical_phaser = mathematical_phaser.clone();
            mathematical_phaser.stretch_factor = 1.0;
            println!("min={min}, max={max}, range={range}");

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
        PhaserKind::Keyframed(_keyframed_phaser) => todo!(),
    };

    debug_assert!((0.0..=u16::MAX as f32).contains(&value));
    value as u16
}
