use std::f32::consts::PI;

use crate::dmx::animation::{AnimationSpecBodyPhaser, MathematicalBaseFunction, PhaserKind};

impl AnimationSpecBodyPhaser {
    pub fn generate(&self, degrees_raw: f32) -> u8 {
        let degrees = degrees_raw % 360.0;
        debug_assert!((0.0..=360.0).contains(&degrees));

        let value = match &self.kind {
            PhaserKind::Mathematical(mathematical_phaser) => {
                let min = mathematical_phaser.amplitude_min as f32;
                let max = mathematical_phaser.amplitude_max as f32;
                let range = max - min;

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
                        let degrees = (degrees * mathematical_phaser.stretch_factor) % 360.0;
                        let phase = degrees / 360.0;

                        let triangle = 4.0 * (phase - 0.5).abs() - 1.0; // -1 to 1

                        // Map from [-1,1] to [min,max]
                        ((triangle + 1.0) / 2.0) * range + min
                    }
                    MathematicalBaseFunction::Square => {
                        let angle = (degrees * mathematical_phaser.stretch_factor) % 360.0;

                        // High in first half, low in second half
                        if angle < 180.0 {
                            max
                        } else {
                            min
                        }
                    }
                    MathematicalBaseFunction::Sawtooth => {
                        let angle = (degrees * mathematical_phaser.stretch_factor) % 360.0;
                        let phase = angle / 360.0;

                        // Linear ramp from min to max
                        min + phase * range
                    }
                    MathematicalBaseFunction::EaseIn => {
                        let t = match degrees_raw {
                            v @ 0.0..360.0 => v,
                            _ => 360.0,
                        };

                        let t = t.clamp(0.0, 360.0) / 360.0;

                        let percent = if t < 0.5 {
                            2.0 * t * t
                        } else {
                            1.0 - 2.0 * (1.0 - t) * (1.0 - t)
                        };

                        percent * range + min
                    }
                    MathematicalBaseFunction::EaseOut => {
                        let t = match degrees_raw {
                            v @ 0.0..360.0 => v,
                            _ => 360.0,
                        };

                        let t = t.clamp(0.0, 360.0) / 360.0;

                        let percent = if t < 0.5 {
                            2.0 * t * t
                        } else {
                            1.0 - 2.0 * (1.0 - t) * (1.0 - t)
                        };

                        (1.0 - percent) * range + min
                    }
                    MathematicalBaseFunction::EaseInOut => {
                        let t = match degrees_raw {
                            v @ 0.0..360.0 => v,
                            _ => 360.0,
                        };
                        let radians = std::f32::consts::PI * t.clamp(0.0, 360.0) / 360.0;
                        let v = radians.sin() * range + min;
                        v.clamp(0.0, 255.0)
                    }
                }
            }
            PhaserKind::Keyframed(_keyframed_phaser) => todo!(),
        };

        debug_assert!((0.0..=255.0).contains(&value));
        value as u8
    }

    // fn generate(&self, degrees: f32) -> u8 {
    //     debug_assert!(degrees <= 360.0 && degrees >= 0.0);
    //
    //     let value = match &self.kind {
    //         PhaserKind::Mathematical(mathematical_phaser) => match mathematical_phaser.base {
    //             MathematicalBaseFunction::Sin => {
    //                 let radians = degrees * mathematical_phaser.stretch_factor * (PI / 180.0);
    //                 let sine = radians.sin();
    //
    //                 let value = mathematical_phaser.amplitude_max as f32 * sine
    //                     + mathematical_phaser.amplitude_min as f32;
    //
    //                 value
    //             }
    //             MathematicalBaseFunction::Cos => {
    //                 let radians = degrees * mathematical_phaser.stretch_factor * (PI / 180.0);
    //                 let sine = radians.cos();
    //
    //                 let value = mathematical_phaser.amplitude_max as f32 * sine
    //                     + mathematical_phaser.amplitude_min as f32;
    //
    //                 value
    //             }
    //             MathematicalBaseFunction::Triangle => {
    //                 let degrees = (degrees * mathematical_phaser.stretch_factor) % 360.0; // stretch and wrap degrees
    //                 let phase = degrees / 360.0; // normalize to [0,1)
    //
    //                 let value = 4.0 * (phase - 0.5).abs() - 1.0;
    //
    //                 let value = mathematical_phaser.amplitude_max as f32 * value
    //                     + mathematical_phaser.amplitude_min as f32;
    //
    //                 value
    //             }
    //             MathematicalBaseFunction::Square => {
    //                 let angle = (degrees * mathematical_phaser.stretch_factor) % 360.0;
    //                 if angle < 180.0 {
    //                     mathematical_phaser.amplitude_min.into()
    //                 } else {
    //                     mathematical_phaser.amplitude_max.into()
    //                 }
    //             }
    //             MathematicalBaseFunction::Sawtooth => {
    //                 let angle = (degrees * mathematical_phaser.stretch_factor) % 360.0;
    //                 let phase = angle / 360.0;
    //
    //                 let value = mathematical_phaser.amplitude_min as f32
    //                     + phase
    //                         * (mathematical_phaser.amplitude_max
    //                             - mathematical_phaser.amplitude_min)
    //                             as f32;
    //                 value
    //             }
    //             MathematicalBaseFunction::EaseIn => todo!(),
    //             MathematicalBaseFunction::EaseOut => todo!(),
    //             MathematicalBaseFunction::EaseInOut => todo!(),
    //             MathematicalBaseFunction::PerlinNoise => todo!(),
    //         },
    //         PhaserKind::Keyframed(keyframed_phaser) => todo!(),
    //     };
    //
    //     debug_assert!(value >= 0.0 && value <= 255.0);
    //     value as u8
    // }
}
