//! Offline loop classifier and tempo estimator.
//!
//! This is a Rust implementation of the tatum-quantization algorithm from
//! <https://github.com/saintmatthieu/loop-tempo-estimator>. Unlike the live
//! estimator in [`crate::SignalCollector`], it analyzes a complete, circular
//! audio loop at once and can reject input that does not look like a loop.

use std::{collections::BTreeMap, sync::Arc};

use rustfft::{num_complex::Complex, Fft, FftPlanner};

const TARGET_SAMPLE_RATE: f64 = 24_000.0;
const MAX_DURATION_SECONDS: f64 = 60.0;
const STRICT_THRESHOLD: f64 = 0.818_512_256_965_394_6;
const LENIENT_THRESHOLD: f64 = 0.800_450_087_348_855_7;
const BEATS_PER_BAR: usize = 4;
const MIN_BPM: f64 = 50.0;
const MAX_BPM: f64 = 200.0;
const TATUMS_PER_BAR: [usize; 7] = [1, 2, 4, 8, 12, 16, 24];

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum FalsePositiveTolerance {
    Strict,
    #[default]
    Lenient,
}

impl FalsePositiveTolerance {
    fn threshold(self) -> f64 {
        match self {
            Self::Strict => STRICT_THRESHOLD,
            Self::Lenient => LENIENT_THRESHOLD,
        }
    }
}

/// Why an analysis did not return an accepted loop tempo.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LoopTempoRejection {
    InvalidAudio,
    TooLong,
    TooShort,
    NoOnsets,
    SingleEvent,
    BelowThreshold,
}

/// Full result, including the candidate used to compare rejected inputs.
#[derive(Clone, Debug)]
pub struct LoopTempoAnalysis {
    /// BPM when the input passes the loop classifier.
    pub bpm: Option<f64>,
    /// Best BPM hypothesis even when the classifier rejects it.
    pub candidate_bpm: Option<f64>,
    /// Loop-likelihood score in the nominal range 0..=1.
    pub score: Option<f64>,
    pub threshold: f64,
    pub tatum_count: Option<usize>,
    pub onset_lag: Option<usize>,
    pub rejection: Option<LoopTempoRejection>,
}

impl LoopTempoAnalysis {
    fn rejected(threshold: f64, reason: LoopTempoRejection) -> Self {
        Self {
            bpm: None,
            candidate_bpm: None,
            score: None,
            threshold,
            tatum_count: None,
            onset_lag: None,
            rejection: Some(reason),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct LoopTempoEstimator {
    pub tolerance: FalsePositiveTolerance,
    /// Inputs longer than this are rejected before analysis. Set to `None` to
    /// analyze any duration.
    pub max_duration_seconds: Option<f64>,
}

impl Default for LoopTempoEstimator {
    fn default() -> Self {
        Self {
            tolerance: FalsePositiveTolerance::Lenient,
            max_duration_seconds: Some(MAX_DURATION_SECONDS),
        }
    }
}

impl LoopTempoEstimator {
    pub fn analyze(&self, samples: &[f32], sample_rate: u32) -> LoopTempoAnalysis {
        let threshold = self.tolerance.threshold();
        if sample_rate == 0 || samples.is_empty() {
            return LoopTempoAnalysis::rejected(threshold, LoopTempoRejection::InvalidAudio);
        }

        let duration = samples.len() as f64 / sample_rate as f64;
        if self
            .max_duration_seconds
            .is_some_and(|maximum| duration > maximum)
        {
            return LoopTempoAnalysis::rejected(threshold, LoopTempoRejection::TooLong);
        }

        let decimation = (sample_rate as f64 / TARGET_SAMPLE_RATE).ceil().max(1.0) as usize;
        let decimated_sample_count = samples.len() / decimation;
        let decimated: Vec<f32> = samples
            .iter()
            .step_by(decimation)
            .take(decimated_sample_count)
            .copied()
            .collect();
        let decimated_rate = sample_rate as f64 / decimation as f64;
        let analysis_duration = decimated.len() as f64 / decimated_rate;
        let Some(odf) = onset_detection_function(&decimated, decimated_rate) else {
            return LoopTempoAnalysis::rejected(threshold, LoopTempoRejection::TooShort);
        };

        let peaks = peak_indices(&odf);
        if peaks.is_empty() {
            return LoopTempoAnalysis::rejected(threshold, LoopTempoRejection::NoOnsets);
        }
        if is_single_event(&odf) {
            return LoopTempoAnalysis::rejected(threshold, LoopTempoRejection::SingleEvent);
        }

        let hypotheses = tatum_count_to_bar_counts(analysis_duration);
        if hypotheses.is_empty() {
            return LoopTempoAnalysis::rejected(threshold, LoopTempoRejection::TooShort);
        }

        let peak_values: Vec<f32> = peaks.iter().map(|&index| odf[index]).collect();
        let mut best: Option<Quantization> = None;
        for &tatum_count in hypotheses.keys() {
            let onset_lag = onset_lag(&odf, tatum_count);
            let error =
                quantization_distance(&peaks, &peak_values, odf.len(), tatum_count, onset_lag);
            if error.is_finite() && best.map_or(true, |current| error < current.error) {
                best = Some(Quantization {
                    error,
                    onset_lag,
                    tatum_count,
                });
            }
        }

        let Some(best) = best else {
            return LoopTempoAnalysis::rejected(threshold, LoopTempoRejection::NoOnsets);
        };
        let candidate_bpm = most_likely_bpm(&hypotheses[&best.tatum_count], analysis_duration);
        let score = 1.0 - best.error;
        let accepted = score >= threshold;

        LoopTempoAnalysis {
            bpm: accepted.then_some(candidate_bpm),
            candidate_bpm: Some(candidate_bpm),
            score: Some(score),
            threshold,
            tatum_count: Some(best.tatum_count),
            onset_lag: Some(best.onset_lag),
            rejection: (!accepted).then_some(LoopTempoRejection::BelowThreshold),
        }
    }
}

#[derive(Clone, Copy)]
struct Quantization {
    error: f64,
    onset_lag: usize,
    tatum_count: usize,
}

fn fft_size(sample_rate: f64) -> usize {
    let exponent = 11_i32 + (sample_rate / 44_100.0).log2().round() as i32;
    2usize.pow(exponent.max(1) as u32)
}

fn onset_detection_function(samples: &[f32], sample_rate: f64) -> Option<Vec<f32>> {
    let frame_count = (samples.len() as f64 / (0.01 * sample_rate)).round() as usize;
    if frame_count == 0 {
        return None;
    }

    let size = fft_size(sample_rate);
    let hop = samples.len() as f64 / frame_count as f64;
    let window = normalized_hann(size);
    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(size);
    let mut previous = vec![0.0; size / 2 + 1];
    let mut first = Vec::new();
    let mut odf = Vec::with_capacity(frame_count);

    for frame_index in 0..frame_count {
        let mut frame = circular_frame(samples, size, hop, frame_index);
        for (sample, weight) in frame.iter_mut().zip(&window) {
            sample.re *= *weight;
        }
        let spectrum = compressed_power_spectrum(&mut frame, &fft);
        if first.is_empty() {
            first = spectrum.clone();
        } else {
            odf.push(novelty(&previous, &spectrum));
        }
        previous = spectrum;
    }
    odf.push(novelty(&previous, &first));

    let moving_average = moving_average(&odf, sample_rate / hop);
    for (value, average) in odf.iter_mut().zip(moving_average) {
        *value = (*value - average).max(0.0);
    }
    Some(odf)
}

fn circular_frame(samples: &[f32], size: usize, hop: f64, frame_index: usize) -> Vec<Complex<f32>> {
    // Match the reference's truncated first-read position and rounded frame positions.
    let first_read_position = (hop - size as f64) as isize;
    let mut start = (first_read_position as f64 + frame_index as f64 * hop).round() as isize;
    while start < 0 {
        start += samples.len() as isize;
    }
    let start = start as usize;
    let mut frame = vec![Complex::default(); size];
    let first_count = size.min(samples.len().saturating_sub(start));
    for (output, &sample) in frame.iter_mut().zip(&samples[start..start + first_count]) {
        output.re = sample;
    }
    let remaining = (size - first_count).min(samples.len());
    for (output, &sample) in frame[first_count..].iter_mut().zip(&samples[..remaining]) {
        output.re = sample;
    }
    frame
}

fn compressed_power_spectrum(frame: &mut [Complex<f32>], fft: &Arc<dyn Fft<f32>>) -> Vec<f32> {
    fft.process(frame);
    frame[..=frame.len() / 2]
        .iter()
        .map(|bin| fast_log2(1.0 + 100.0 * bin.norm()))
        .collect()
}

// The reference uses this two-decimal-place approximation. Matching it keeps
// the classifier's calibrated score thresholds meaningful.
fn fast_log2(value: f32) -> f32 {
    let bits = value.to_bits();
    let mut result = (((bits >> 23) & 255) as i32 - 128) as f32;
    let normalized = f32::from_bits((bits & !(255 << 23)) + (127 << 23));
    result += ((-0.335_828_78 * normalized + 2.0) * normalized) - 0.658_717_6;
    result
}

fn novelty(previous: &[f32], current: &[f32]) -> f32 {
    previous
        .iter()
        .zip(current)
        .map(|(before, after)| (after - before).max(0.0))
        .sum()
}

fn normalized_hann(size: usize) -> Vec<f32> {
    let mut window: Vec<f32> = (0..size)
        .map(|index| 0.5 * (1.0 - (2.0 * std::f32::consts::PI * index as f32 / size as f32).cos()))
        .collect();
    let sum: f32 = window.iter().sum();
    for value in &mut window {
        *value /= sum;
    }
    window
}

fn moving_average(input: &[f32], frame_rate: f64) -> Vec<f32> {
    let radius = (0.2 * frame_rate / 4.0).round() as isize * 2 + 1;
    let window = normalized_hann((2 * radius + 1) as usize);
    (0..input.len())
        .map(|center| {
            let average: f32 = (-radius..=radius)
                .map(|offset| {
                    let index =
                        (center as isize + offset).rem_euclid(input.len() as isize) as usize;
                    input[index] * window[(offset + radius) as usize]
                })
                .sum();
            average * 1.5
        })
        .collect()
}

fn peak_indices(input: &[f32]) -> Vec<usize> {
    (0..input.len())
        .filter(|&index| {
            let before = input[(index + input.len() - 1) % input.len()];
            let after = input[(index + 1) % input.len()];
            before < input[index] && input[index] > after
        })
        .collect()
}

fn is_single_event(odf: &[f32]) -> bool {
    let sum: f64 = odf.iter().map(|&value| value as f64).sum();
    if sum <= f64::EPSILON {
        return false;
    }
    let maximum = odf
        .iter()
        .enumerate()
        .max_by(|(_, left), (_, right)| left.total_cmp(right))
        .map(|(index, _)| index)
        .unwrap_or(0);
    let shift = (odf.len() * 3 / 2 - maximum) % odf.len();
    let probability = |index: usize| odf[(index + odf.len() - shift) % odf.len()] as f64 / sum;
    let expected: f64 = (0..odf.len())
        .map(|index| index as f64 * probability(index))
        .sum();
    let mut variance = 0.0;
    let mut fourth_moment = 0.0;
    for index in 0..odf.len() {
        let centered = index as f64 - expected;
        variance += centered.powi(2) * probability(index);
        fourth_moment += centered.powi(4) * probability(index);
    }
    variance > f64::EPSILON && fourth_moment / variance.powi(2) > 20.0
}

fn tatum_count_to_bar_counts(duration: f64) -> BTreeMap<usize, Vec<usize>> {
    let min_bars = (duration * (MIN_BPM / BEATS_PER_BAR as f64) / 60.0)
        .ceil()
        .max(1.0) as usize;
    let max_bars = (duration * (MAX_BPM / BEATS_PER_BAR as f64) / 60.0).floor() as usize;
    let mut result = BTreeMap::new();
    if max_bars < min_bars {
        return result;
    }
    for bars in min_bars..=max_bars {
        for tatums_per_bar in TATUMS_PER_BAR {
            let tatum_count = tatums_per_bar * bars;
            let tatums_per_minute = 60.0 * tatum_count as f64 / duration;
            if MIN_BPM / (BEATS_PER_BAR as f64) < tatums_per_minute && tatums_per_minute < 720.0 {
                result
                    .entry(tatum_count)
                    .or_insert_with(Vec::new)
                    .push(bars);
            }
        }
    }
    result
}

fn onset_lag(odf: &[f32], tatum_count: usize) -> usize {
    let period = odf.len() as f64 / tatum_count as f64;
    let mut maximum = f32::NEG_INFINITY;
    for lag in 0..period.round() as usize {
        let value: f32 = (0..tatum_count)
            .map(|index| {
                let odf_index = (index as f64 * period).round() as usize + lag;
                odf.get(odf_index).copied().unwrap_or(0.0)
            })
            .sum();
        if value < maximum {
            return lag.saturating_sub(1);
        }
        maximum = value;
    }
    0
}

fn quantization_distance(
    peaks: &[usize],
    values: &[f32],
    odf_size: usize,
    tatum_count: usize,
    lag: usize,
) -> f64 {
    let weight_sum: f64 = values.iter().map(|&value| value as f64).sum();
    if weight_sum <= f64::EPSILON {
        return f64::NAN;
    }
    peaks
        .iter()
        .zip(values)
        .map(|(&peak, &weight)| {
            let position = (peak as f64 - lag as f64) / odf_size as f64;
            let nearest = (position * tatum_count as f64).round() / tatum_count as f64;
            2.0 * (position - nearest).abs() * tatum_count as f64 * weight as f64
        })
        .sum::<f64>()
        / weight_sum
}

fn most_likely_bpm(possible_bar_counts: &[usize], duration: f64) -> f64 {
    // Resolve the tatum ambiguity with the conventional 120 BPM prior.
    let bars = possible_bar_counts
        .iter()
        .copied()
        .min_by(|left, right| {
            let left_distance = (60.0 * (*left * BEATS_PER_BAR) as f64 / duration - 120.0).abs();
            let right_distance = (60.0 * (*right * BEATS_PER_BAR) as f64 / duration - 120.0).abs();
            left_distance.total_cmp(&right_distance)
        })
        .unwrap_or(1);
    60.0 * (bars * BEATS_PER_BAR) as f64 / duration
}

#[cfg(test)]
mod tests {
    use super::*;

    fn click_loop(bpm: f64, bars: usize, sample_rate: u32) -> Vec<f32> {
        let beats = bars * BEATS_PER_BAR;
        let duration = beats as f64 * 60.0 / bpm;
        let mut samples = vec![0.0; (duration * sample_rate as f64).round() as usize];
        let sixteenth = 60.0 / bpm / 4.0;
        for tatum in 0..beats * 4 {
            let start = (tatum as f64 * sixteenth * sample_rate as f64).round() as usize;
            for offset in 0..96.min(samples.len().saturating_sub(start)) {
                let accent = if tatum % 4 == 0 { 1.0 } else { 0.4 };
                samples[start + offset] += accent * (1.0 - offset as f32 / 96.0);
            }
        }
        samples
    }

    #[test]
    fn silence_is_rejected() {
        let result = LoopTempoEstimator::default().analyze(&vec![0.0; 44_100 * 4], 44_100);
        assert_eq!(result.bpm, None);
        assert_eq!(result.rejection, Some(LoopTempoRejection::NoOnsets));
    }

    #[test]
    fn estimates_a_quantized_loop() {
        let samples = click_loop(120.0, 2, 44_100);
        let result = LoopTempoEstimator::default().analyze(&samples, 44_100);
        assert_eq!(result.bpm, Some(120.0));
        assert!(result.score.unwrap() >= result.threshold, "{result:?}");
    }

    #[test]
    fn rejects_audio_over_the_default_duration_limit() {
        let samples = vec![0.0; 44_100 * 61];
        let result = LoopTempoEstimator::default().analyze(&samples, 44_100);
        assert_eq!(result.rejection, Some(LoopTempoRejection::TooLong));
    }
}
