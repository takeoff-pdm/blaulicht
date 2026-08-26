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
            let amp_a = mathematical_phaser
                .amplitude_min
                .resolve(palettes, property) as f32;
            let amp_b = mathematical_phaser
                .amplitude_max
                .resolve(palettes, property) as f32;
            // Normalize ordering: an inverted min/max would drive values
            // negative in the spike shapes.
            let (min, max) = if amp_a <= amp_b {
                (amp_a, amp_b)
            } else {
                (amp_b, amp_a)
            };
            let range = max - min;

            let mut mathematical_phaser = mathematical_phaser.clone();
            mathematical_phaser.stretch_factor =
                mathematical_phaser.stretch_factor.clamp(0.01, 100.0);

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
                    let angle = (degrees * mathematical_phaser.stretch_factor).rem_euclid(360.0);
                    let t = angle / 360.0;

                    let percent = if t < 0.5 {
                        2.0 * t * t
                    } else {
                        1.0 - 2.0 * (1.0 - t) * (1.0 - t)
                    };

                    percent * range + min
                }
                MathematicalBaseFunction::EaseOut => {
                    let angle = (degrees * mathematical_phaser.stretch_factor).rem_euclid(360.0);
                    let t = angle / 360.0;

                    let percent = if t < 0.5 {
                        2.0 * t * t
                    } else {
                        1.0 - 2.0 * (1.0 - t) * (1.0 - t)
                    };

                    (1.0 - percent) * range + min
                }
                MathematicalBaseFunction::EaseInOut => {
                    // sin over the half period already stays in min..min+range;
                    // clamping to 0..255 here broke 0..360 hue phasers.
                    let angle = (degrees * mathematical_phaser.stretch_factor).rem_euclid(360.0);
                    let radians = std::f32::consts::PI * angle / 360.0;
                    radians.sin() * range + min
                }
            }
        }
        // Keyframed phasers are not implemented yet. Keep the runtime alive
        // and leave the driven property at a deterministic safe value.
        PhaserKind::Keyframed(_keyframed_phaser) => 0.0,
    };

    value.clamp(0.0, u16::MAX as f32) as u16
}
