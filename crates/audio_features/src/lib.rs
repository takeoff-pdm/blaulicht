//! Streaming log-mel front end for the learned beat model.
//!
//! The live engine and the training pipeline both run raw mono PCM through
//! [`MelFrontend`], so the model sees bit-identical input on stage and in
//! training. It runs on fixed time units, not on the display spectrum.
//!
//! Timing: frames are causal and arrive at [`FRAME_RATE`]. Frame `k` is
//! emitted once `(k + 1) * hop` samples have been pushed and covers the
//! `window_len` samples before that point (zeros before the stream start).
//! A beat at time `t` seconds first becomes visible in frame
//! `floor(t * FRAME_RATE)`.
//!
//! Output is independent of how the PCM is split into blocks, and nearly
//! independent of the sample rate: window and filters are defined in seconds
//! and Hz, not in samples or bins.

use std::sync::Arc;

use rustfft::{num_complex::Complex, Fft, FftPlanner};

pub mod beat_model;

pub use beat_model::{BeatActivation, BeatModel, ModelInfo};

/// Bump when the output of [`MelFrontend`] changes, so cached features and
/// trained models built on an older front end can be detected.
pub const FEATURE_VERSION: u32 = 1;
pub const FRAME_RATE: u32 = 100;
pub const N_MELS: usize = 64;
pub const MEL_FMIN_HZ: f32 = 30.0;
pub const MEL_FMAX_HZ: f32 = 11_000.0;
/// ~2048 samples at 44.1 kHz: enough resolution to separate kick and bass.
const WINDOW_SECONDS: f64 = 0.0464;
/// Magnitudes are normalized to sine amplitude; this sets where log
/// compression starts.
const LOG_COMPRESSION: f32 = 1_000.0;

pub type MelFrame = [f32; N_MELS];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrontendConfig {
    pub sample_rate: u32,
    pub hop: usize,
    pub window_len: usize,
    pub fft_size: usize,
}

impl FrontendConfig {
    pub fn new(sample_rate: u32) -> Result<Self, String> {
        if sample_rate % FRAME_RATE != 0 {
            return Err(format!(
                "sample rate {sample_rate} Hz is not a multiple of the {FRAME_RATE} Hz frame rate"
            ));
        }
        if (sample_rate as f32) < 2.0 * MEL_FMAX_HZ {
            return Err(format!(
                "sample rate {sample_rate} Hz cannot represent {MEL_FMAX_HZ} Hz"
            ));
        }
        let window_len = (sample_rate as f64 * WINDOW_SECONDS).round() as usize;
        Ok(Self {
            sample_rate,
            hop: (sample_rate / FRAME_RATE) as usize,
            window_len,
            fft_size: window_len.next_power_of_two(),
        })
    }
}

/// One triangular mel filter over a contiguous range of FFT bins.
struct MelFilter {
    first_bin: usize,
    weights: Vec<f32>,
}

pub struct MelFrontend {
    config: FrontendConfig,
    /// Hann window scaled so a full-scale sine yields magnitude ~1.
    window: Vec<f32>,
    fft: Arc<dyn Fft<f32>>,
    buffer: Vec<Complex<f32>>,
    magnitudes: Vec<f32>,
    filters: Vec<MelFilter>,
    /// Ring buffer of the last `window_len` samples; `write` is the oldest.
    history: Vec<f32>,
    write: usize,
    since_frame: usize,
}

impl MelFrontend {
    pub fn new(sample_rate: u32) -> Result<Self, String> {
        let config = FrontendConfig::new(sample_rate)?;
        let hann: Vec<f32> = (0..config.window_len)
            .map(|i| {
                0.5 - 0.5
                    * (2.0 * std::f32::consts::PI * i as f32 / config.window_len as f32).cos()
            })
            .collect();
        let gain = 2.0 / hann.iter().sum::<f32>();
        Ok(Self {
            config,
            window: hann.iter().map(|w| w * gain).collect(),
            fft: FftPlanner::new().plan_fft_forward(config.fft_size),
            buffer: vec![Complex::default(); config.fft_size],
            magnitudes: vec![0.0; config.fft_size / 2 + 1],
            filters: mel_filters(sample_rate, config.fft_size),
            history: vec![0.0; config.window_len],
            write: 0,
            since_frame: 0,
        })
    }

    pub fn config(&self) -> FrontendConfig {
        self.config
    }

    /// Center frequency of each mel band in Hz.
    pub fn band_centers_hz() -> [f32; N_MELS] {
        let edges = mel_edges_hz();
        std::array::from_fn(|m| edges[m + 1])
    }

    /// Feed mono PCM; appends one frame to `out` per completed hop.
    pub fn push(&mut self, samples: &[f32], out: &mut Vec<MelFrame>) {
        for &sample in samples {
            self.history[self.write] = sample;
            self.write = (self.write + 1) % self.history.len();
            self.since_frame += 1;
            if self.since_frame == self.config.hop {
                self.since_frame = 0;
                out.push(self.frame());
            }
        }
    }

    fn frame(&mut self) -> MelFrame {
        let len = self.history.len();
        let (oldest, newest) = self.history.split_at(self.write);
        for ((slot, sample), w) in self
            .buffer
            .iter_mut()
            .zip(newest.iter().chain(oldest))
            .zip(&self.window)
        {
            *slot = Complex::new(sample * w, 0.0);
        }
        for slot in &mut self.buffer[len..] {
            *slot = Complex::default();
        }
        self.fft.process(&mut self.buffer);
        for (mag, bin) in self.magnitudes.iter_mut().zip(&self.buffer) {
            *mag = bin.norm();
        }

        std::array::from_fn(|m| {
            let filter = &self.filters[m];
            let energy: f32 = filter
                .weights
                .iter()
                .zip(&self.magnitudes[filter.first_bin..])
                .map(|(w, mag)| w * mag)
                .sum();
            (1.0 + LOG_COMPRESSION * energy).ln()
        })
    }
}

fn hz_to_mel(hz: f32) -> f32 {
    2595.0 * (1.0 + hz / 700.0).log10()
}

fn mel_to_hz(mel: f32) -> f32 {
    700.0 * (10f32.powf(mel / 2595.0) - 1.0)
}

/// `N_MELS + 2` band edges, evenly spaced on the mel scale.
fn mel_edges_hz() -> [f32; N_MELS + 2] {
    let (lo, hi) = (hz_to_mel(MEL_FMIN_HZ), hz_to_mel(MEL_FMAX_HZ));
    std::array::from_fn(|i| mel_to_hz(lo + (hi - lo) * i as f32 / (N_MELS + 1) as f32))
}

/// Triangular filters normalized to unit weight sum, so each band is a
/// weighted mean magnitude and narrow low bands are not quieter than wide
/// high ones.
fn mel_filters(sample_rate: u32, fft_size: usize) -> Vec<MelFilter> {
    let bin_hz = sample_rate as f32 / fft_size as f32;
    let edges = mel_edges_hz();
    (0..N_MELS)
        .map(|m| {
            let (lo, center, hi) = (edges[m], edges[m + 1], edges[m + 2]);
            let first_bin = (lo / bin_hz).floor() as usize;
            let last_bin = ((hi / bin_hz).ceil() as usize).min(fft_size / 2);
            let mut weights: Vec<f32> = (first_bin..=last_bin)
                .map(|b| {
                    let f = b as f32 * bin_hz;
                    if f <= center {
                        (f - lo) / (center - lo)
                    } else {
                        (hi - f) / (hi - center)
                    }
                    .max(0.0)
                })
                .collect();
            let sum: f32 = weights.iter().sum();
            if sum > 0.0 {
                weights.iter_mut().for_each(|w| *w /= sum);
            } else {
                // Narrower than one bin: take the nearest bin.
                weights = vec![0.0; weights.len()];
                let nearest = ((center / bin_hz).round() as usize).clamp(first_bin, last_bin);
                weights[nearest - first_bin] = 1.0;
            }
            MelFilter { first_bin, weights }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(sample_rate: u32, pcm: &[f32], block: usize) -> Vec<MelFrame> {
        let mut frontend = MelFrontend::new(sample_rate).unwrap();
        let mut out = Vec::new();
        for chunk in pcm.chunks(block) {
            frontend.push(chunk, &mut out);
        }
        out
    }

    fn noise(len: usize) -> Vec<f32> {
        let mut state = 0x1234_5678u32;
        (0..len)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                state as f32 / u32::MAX as f32 - 0.5
            })
            .collect()
    }

    fn sine(sample_rate: u32, hz: f32, seconds: f32) -> Vec<f32> {
        (0..(sample_rate as f32 * seconds) as usize)
            .map(|i| (2.0 * std::f32::consts::PI * hz * i as f32 / sample_rate as f32).sin())
            .collect()
    }

    #[test]
    fn output_does_not_depend_on_block_size() {
        let pcm = noise(44_100);
        let reference = run(44_100, &pcm, pcm.len());
        assert_eq!(reference.len(), 100);
        for block in [1, 7, 441, 1_000, 4_096] {
            assert_eq!(run(44_100, &pcm, block), reference, "block {block}");
        }
    }

    #[test]
    fn onset_first_appears_in_the_frame_of_its_time() {
        let sample_rate = 48_000;
        let onset = 12_345;
        let mut pcm = vec![0.0; onset];
        pcm.extend(noise(sample_rate as usize - onset));
        let frames = run(sample_rate, &pcm, 480);
        let first = frames.iter().position(|f| f.iter().any(|v| *v > 0.0)).unwrap();
        assert_eq!(first, onset * FRAME_RATE as usize / sample_rate as usize);
    }

    #[test]
    fn sine_peaks_in_its_mel_band() {
        let centers = MelFrontend::band_centers_hz();
        for hz in [60.0, 440.0, 5_000.0] {
            let frames = run(44_100, &sine(44_100, hz, 0.5), 441);
            let last = frames.last().unwrap();
            let peak = (0..N_MELS).max_by(|a, b| last[*a].total_cmp(&last[*b])).unwrap();
            let nearest = (0..N_MELS)
                .min_by(|a, b| (centers[*a] - hz).abs().total_cmp(&(centers[*b] - hz).abs()))
                .unwrap();
            assert!(peak.abs_diff(nearest) <= 1, "{hz} Hz peaked in band {peak}, expected {nearest}");
        }
    }

    #[test]
    fn sample_rates_give_similar_features() {
        let tone = |sr| {
            let pcm: Vec<f32> = sine(sr, 100.0, 0.5)
                .iter()
                .zip(sine(sr, 2_500.0, 0.5))
                .map(|(a, b)| 0.5 * a + 0.2 * b)
                .collect();
            *run(sr, &pcm, 1_024).last().unwrap()
        };
        let (a, b) = (tone(44_100), tone(48_000));
        let max_diff = a.iter().zip(&b).map(|(x, y)| (x - y).abs()).fold(0.0, f32::max);
        let peak = a.iter().copied().fold(0.0, f32::max);
        assert!(max_diff < 0.1 * peak, "max diff {max_diff} vs peak {peak}");
    }

    #[test]
    fn rejects_rates_without_a_whole_hop() {
        assert!(MelFrontend::new(22_050).is_err());
        assert!(MelFrontend::new(16_000).is_err());
        assert!(MelFrontend::new(44_100).is_ok());
    }
}
