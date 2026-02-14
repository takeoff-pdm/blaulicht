//
// Provides analysis on the audio.
//

use crate::{AudioSource, BpmInfo, Signal, SignalCollector, SignalDebugData};
use itertools::Itertools;
use map_range::MapRange;
use std::u8;

#[derive(Clone, Copy, Debug, Default)]
pub struct Frequency {
    pub volume: f32,

    /// Actual frequency in hz, can range from 0 to `config.sample_rate` / 2
    ///
    /// Accuracy can vary and is not guaranteed
    pub freq: f32,

    /// Relative position of single frequency in range (0..=1)
    ///
    /// Used to make lower freqs occupy more space than higher ones, to mimic human hearing
    ///
    /// Should not be Important, except when distributing freqs manually
    ///
    /// To do this manually set `config.interpolation` equal to `Interpolation::None`
    pub position: f32,
}

#[cfg(feature = "stream_in")]
impl From<audioviz::spectrum::Frequency> for Frequency {
    fn from(value: audioviz::spectrum::Frequency) -> Self {
        Self {
            volume: value.volume,
            freq: value.freq,
            position: value.position,
        }
    }
}

#[cfg(feature = "stream_in")]
impl From<&audioviz::spectrum::Frequency> for Frequency {
    fn from(value: &audioviz::spectrum::Frequency) -> Self {
        Self {
            volume: value.volume,
            freq: value.freq,
            position: value.position,
        }
    }
}

// Constants.
pub const BASS_FRAMES: usize = 10000;
pub const BASS_PEAK_FRAMES: usize = 800;
pub const BASS_MODIFIER: usize = 60;
pub const ONSET_SAMPLE_PERIOD_MS: usize = 10;
pub const ONSET_HISTORY_FRAMES: usize = 600;
const TRANSIENT_HISTORY_MS: usize = 10_000;
const ONSET_EMA_ALPHA: f32 = 0.2;
const ONSET_LONG_ALPHA: f32 = 0.01;
const ONSET_PEAK_STDDEV: f32 = 1.5;
const DEFAULT_BAND_WEIGHTS: [f32; 3] = [0.6, 0.3, 0.1];
const PHASE_WINDOW_FRACTION: f32 = 0.2;
const BEAT_DEBOUNCE_FRACTION: f32 = 0.25;
const PHASE_CORRECTION: f32 = 0.1;
const MIN_PHASE_WINDOW_MS: f32 = 25.0;
const ONSET_METRIC_EPS: f32 = 1e-6;
const WEIGHT_SOFTMAX_TEMPERATURE: f32 = 1.0;
// Lower -> steadier weights, higher -> faster adaptation.
// With ~10 ms updates, 0.01 is roughly a ~1s time constant.
const WEIGHT_SMOOTH_ALPHA: f32 = 0.0001;
// Faster EMA for transient tracking.
const TRANSIENT_SHORT_ALPHA: f32 = 0.3;
// Smoothing for attack/decay slope tracking.
const TRANSIENT_SLOPE_ALPHA: f32 = 0.05;
const MIN_BPM: f32 = 80.0;
const MAX_BPM: f32 = 200.0;
const MID_FREQ_HIGH: f32 = 2000.0;
const HIGH_FREQ_HIGH: f32 = 8000.0;

// TODO: what is this constant even
pub const ROLLING_AVERAGE_VOLUME_SAMPLE_SIZE: usize = 100;
pub const ROLLING_AVERAGE_FRAMES: usize = 100;
pub const LONG_HISTORIC_FRAMES: usize = ROLLING_AVERAGE_FRAMES * 1000;

///
/// Vector push operations.
///

#[macro_export]
macro_rules! shift_push {
    ($vector:expr,$capacity:ident,$item:expr) => {
        $vector.push_back($item);
        if $vector.len() > $capacity {
            $vector.pop_front();
        }
    };
}

impl<const NUM_OUTPUTS: usize, SourceT> SignalCollector<NUM_OUTPUTS, SourceT>
where
    SourceT: AudioSource,
{
    #[inline(always)]
    pub fn bass(&mut self, now: usize) -> anyhow::Result<()> {
        let signals = {
            let lower_volume_limit = self.params.bass_volume as f64;

            let bass_range =
                (self.params.bass_freq_low as f32)..(self.params.bass_freq_high as f32);

            let bass_samples = self
                .freq_buffer
                .iter()
                .copied()
                .filter(|f| bass_range.contains(&f.freq))
                .map(|f| f.volume as f32)
                .collect::<Vec<_>>();

            // let bass_samples = bass_samples_freqs
            //     .iter()
            //     .map(|f| f.volume as f32)
            //     .collect::<Vec<_>>();

            let avg = if bass_samples.is_empty() {
                0.0
            } else {
                bass_samples.iter().sum::<f32>() / bass_samples.len() as f32
            };
            let bass_sig = (avg * 100.0) as u8;

            // Bass samples.
            self.scratch.bass_samples.push_back(bass_sig);

            if self.scratch.bass_samples.len() >= BASS_FRAMES {
                self.scratch.bass_samples.pop_front();
            }

            let bass_moving_average = if self.scratch.bass_samples.is_empty() {
                0.0
            } else {
                self.scratch
                    .bass_samples
                    .iter()
                    .map(|v| *v as f64)
                    .sum::<f64>()
                    / self.scratch.bass_samples.len() as f64
            };

            let elapsed_since_last_peak = match self.scratch.bass_peaks.iter().last() {
                Some(last) => now - last,
                None => 10000,
            };

            // Must be in the upper 90% to be a peak.
            // Do not consider values under bass 10.
            let mut peaked = false;

            if bass_moving_average >= lower_volume_limit {
                let bass_signal_threshold_for_a_peak =
                    (bass_moving_average * 2.0) * (BASS_MODIFIER as f64 / 100.0);
                if bass_sig >= bass_signal_threshold_for_a_peak as u8
                    && elapsed_since_last_peak > 200
                {
                    self.scratch.bass_peaks.push_back(now);
                    peaked = true;
                }
            }

            if self.scratch.bass_peaks.len() >= BASS_PEAK_FRAMES {
                self.scratch.bass_peaks.pop_front();
            }

            let max_freq = self
                .freq_buffer
                .iter()
                .fold(0.0_f32, |acc, f| acc.max(f.freq));

            let mid_high = MID_FREQ_HIGH.min(max_freq);
            let high_high = HIGH_FREQ_HIGH.min(max_freq);
            let mid_range = bass_range.end..mid_high;
            let high_range = mid_high..high_high;

            let band_energies = [
                Self::band_energy(&self.freq_buffer, bass_range.clone()),
                Self::band_energy(&self.freq_buffer, mid_range),
                Self::band_energy(&self.freq_buffer, high_range),
            ];

            let mut band_onsets = [0.0_f32; 3];
            for (idx, energy) in band_energies.iter().enumerate() {
                let energy = *energy;
                let long_prev = self.scratch.band_energy_ema[idx];
                if long_prev <= 0.0 {
                    self.scratch.band_energy_ema[idx] = energy;
                } else {
                    band_onsets[idx] = (energy - long_prev).max(0.0);
                    self.scratch.band_energy_ema[idx] =
                        long_prev + ONSET_LONG_ALPHA * (energy - long_prev);
                }

                let short_prev = self.scratch.band_energy_ema_short[idx];
                let short_new = if short_prev <= 0.0 {
                    energy
                } else {
                    short_prev + TRANSIENT_SHORT_ALPHA * (energy - short_prev)
                };
                self.scratch.band_energy_ema_short[idx] = short_new;

                let delta = short_new - short_prev;
                let rise = if delta > 0.0 { delta } else { 0.0 };
                let fall = if delta < 0.0 { -delta } else { 0.0 };

                let rise_prev = self.scratch.band_energy_rise_ema[idx];
                let fall_prev = self.scratch.band_energy_fall_ema[idx];
                self.scratch.band_energy_rise_ema[idx] =
                    rise_prev + TRANSIENT_SLOPE_ALPHA * (rise - rise_prev);
                self.scratch.band_energy_fall_ema[idx] =
                    fall_prev + TRANSIENT_SLOPE_ALPHA * (fall - fall_prev);
            }

            let mut band_onset_peakiness = [0.0_f32; 3];
            let mut band_onset_periodicity = [0.0_f32; 3];
            for (idx, history) in self.scratch.band_onset_history.iter_mut().enumerate() {
                band_onset_peakiness[idx] = Self::onset_peakiness(history.make_contiguous());
                band_onset_periodicity[idx] =
                    Self::onset_periodicity(history.make_contiguous(), ONSET_SAMPLE_PERIOD_MS);
            }

            let band_weights = if self.params.auto_weight {
                let updated = Self::compute_band_weights(
                    self.scratch.band_weights,
                    band_energies,
                    band_onset_peakiness,
                    band_onset_periodicity,
                );
                self.scratch.band_weights = updated;
                updated
            } else {
                self.scratch.band_weights = DEFAULT_BAND_WEIGHTS;
                DEFAULT_BAND_WEIGHTS
            };

            let onset = band_onsets[0] * band_weights[0]
                + band_onsets[1] * band_weights[1]
                + band_onsets[2] * band_weights[2];
            self.scratch.onset_ema += ONSET_EMA_ALPHA * (onset - self.scratch.onset_ema);

            let mut onset_peak = false;
            let mut bpm_from_onset: Option<f32> = None;

            if now - self.scratch.last_onset_sample_time >= ONSET_SAMPLE_PERIOD_MS {
                self.scratch.last_onset_sample_time = now;
                self.scratch.onset_history.push_back(self.scratch.onset_ema);
                if self.scratch.onset_history.len() > ONSET_HISTORY_FRAMES {
                    self.scratch.onset_history.pop_front();
                }
                for (idx, onset_val) in band_onsets.iter().enumerate() {
                    let history = &mut self.scratch.band_onset_history[idx];
                    history.push_back(*onset_val);
                    if history.len() > ONSET_HISTORY_FRAMES {
                        history.pop_front();
                    }
                }

                let history = self.scratch.onset_history.make_contiguous();
                bpm_from_onset = Self::estimate_bpm_from_onset(history, ONSET_SAMPLE_PERIOD_MS);
                let (mean, threshold) = Self::onset_threshold(history);
                onset_peak = self.scratch.onset_ema > threshold
                    && (mean == 0.0 || self.scratch.onset_ema > mean * 1.2);

                if onset_peak {
                    for idx in 0..3 {
                        let rise = self.scratch.band_energy_rise_ema[idx];
                        let fall = self.scratch.band_energy_fall_ema[idx];
                        let strength = rise / (rise + fall + ONSET_METRIC_EPS);
                        let history = &mut self.scratch.band_transient_history[idx];
                        history.push_back((now, strength));
                    }
                }
            }

            const SECONDS_IN_A_MINUTE: f64 = 60.0;
            let min_bpm_secs = SECONDS_IN_A_MINUTE / MIN_BPM as f64;
            let max_bpm_secs = SECONDS_IN_A_MINUTE / MAX_BPM as f64;

            let bass_peak_durations =
                self.scratch
                    .bass_peaks
                    .iter()
                    .tuple_windows()
                    .filter_map(|(a, b)| {
                        let d = (*b as f64 - *a as f64) / 1000.0;
                        if d > max_bpm_secs && d < min_bpm_secs {
                            Some(d)
                        } else {
                            None
                        }
                    });

            let bass_len = bass_peak_durations.clone().count();
            let bass_peak_sum = bass_peak_durations.sum::<f64>();
            let avg_bass_peak_durations = if bass_len > 0 {
                bass_peak_sum / bass_len as f64
            } else {
                0.0
            };

            let mut bpm_from_peaks = 0.0_f32;
            if bass_moving_average > lower_volume_limit && bass_len > 0 {
                bpm_from_peaks = (SECONDS_IN_A_MINUTE / avg_bass_peak_durations) as f32;
            }

            if let Some(new_bpm) = bpm_from_onset {
                let new_bpm = new_bpm.clamp(MIN_BPM, MAX_BPM);
                if self.scratch.bpm_estimate <= 0.0 {
                    self.scratch.bpm_estimate = new_bpm;
                } else {
                    self.scratch.bpm_estimate = (self.scratch.bpm_estimate * 0.8) + (new_bpm * 0.2);
                }
            }

            if self.scratch.bpm_estimate <= 0.0 && bpm_from_peaks > 0.0 {
                self.scratch.bpm_estimate = bpm_from_peaks;
            }

            if self.scratch.bpm_estimate > 0.0 {
                self.scratch.beat_interval_ms = 60000.0 / self.scratch.bpm_estimate;
            }

            let bpm_f32 = self.scratch.bpm_estimate;
            // let bpm = bpm_f32.round().clamp(0.0, u8::MAX as f32) as u8;
            let time_between_beats_millis = if self.scratch.beat_interval_ms > 0.0 {
                self.scratch.beat_interval_ms.round() as u16
            } else {
                0
            };

            if self.scratch.beat_interval_ms > 0.0 {
                let now_f = now as f32;
                let expected =
                    self.scratch.time_of_last_bpm_marker as f32 + self.scratch.beat_interval_ms;
                let window = (self.scratch.beat_interval_ms * PHASE_WINDOW_FRACTION)
                    .max(MIN_PHASE_WINDOW_MS);

                // let min_debounce_ms = self.scratch.beat_interval_ms * BEAT_DEBOUNCE_FRACTION;
                // let since_last_beat =
                //     now.saturating_sub(self.scratch.time_of_last_bpm_marker) as f32;
                // let debounce_ok = since_last_beat >= min_debounce_ms;

                if onset_peak {
                    let phase_error = now_f - expected;
                    if self.scratch.beat_needs_sync || phase_error.abs() <= window {
                        self.scratch.is_on_beat = true;
                        self.scratch.actual_onset_peak = true;
                        self.scratch.time_of_last_bpm_marker = now;
                        self.scratch.beat_needs_sync = false;

                        let adjusted =
                            self.scratch.beat_interval_ms + phase_error * PHASE_CORRECTION;
                        self.scratch.beat_interval_ms =
                            adjusted.clamp(60000.0 / MAX_BPM, 60000.0 / MIN_BPM);
                    }
                }

                if !self.scratch.is_on_beat && now_f >= expected {
                    self.scratch.is_on_beat = true;
                    self.scratch.time_of_last_bpm_marker = now;
                    if !onset_peak {
                        self.scratch.beat_needs_sync = true;
                    }
                }
            }

            // let is_bass_avg_short = (self.scratch.onset_ema * 10.0) as u8;

            let mut band_transient_strength = [0.0_f32; 3];
            for idx in 0..3 {
                let history = &mut self.scratch.band_transient_history[idx];
                while let Some((timestamp, _)) = history.front() {
                    if now.saturating_sub(*timestamp) > TRANSIENT_HISTORY_MS {
                        history.pop_front();
                    } else {
                        break;
                    }
                }

                if history.is_empty() {
                    band_transient_strength[idx] = 0.0;
                } else {
                    let sum = history.iter().map(|(_, v)| *v).sum::<f32>();
                    band_transient_strength[idx] = sum / history.len() as f32;
                }
            }

            let debug_data = SignalDebugData {
                bass_range,
                band_energies,
                band_onset_peakiness,
                band_onset_periodicity,
                band_transient_strength,
                band_weights: self.scratch.band_weights,
            };

            &[
                Signal::BeatTrigger(self.scratch.is_on_beat),
                Signal::Bass(bass_sig),
                Signal::Bpm(BpmInfo {
                    bpm: bpm_f32,
                    time_between_beats_millis,
                }),
                // Signal::BassAvgShort(is_bass_avg_short),
                Signal::BassAvg(bass_moving_average as u8),
                Signal::DebugData(debug_data),
            ]
        };

        self.send_signals(signals);

        Ok(())
    }

    fn band_energy(freqs: &[Frequency], range: std::ops::Range<f32>) -> f32 {
        if range.start >= range.end {
            return 0.0;
        }

        freqs
            .iter()
            .filter(|f| f.freq >= range.start && f.freq < range.end)
            .map(|f| f.volume)
            .sum()
    }

    fn onset_threshold(history: &[f32]) -> (f32, f32) {
        if history.is_empty() {
            return (0.0, 0.0);
        }

        let (mean, std, _) = Self::onset_stats(history);
        let threshold = mean + ONSET_PEAK_STDDEV * std;

        (mean, threshold)
    }

    fn onset_peakiness(history: &[f32]) -> f32 {
        if history.is_empty() {
            return 0.0;
        }

        let (mean, _, max) = Self::onset_stats(history);
        max / (mean + ONSET_METRIC_EPS)
    }

    fn compute_band_weights(
        prev: [f32; 3],
        band_energies: [f32; 3],
        band_onset_peakiness: [f32; 3],
        band_onset_periodicity: [f32; 3],
    ) -> [f32; 3] {
        let energy_sum = band_energies.iter().sum::<f32>();
        if !energy_sum.is_finite() || energy_sum <= 0.0 {
            return prev;
        }

        let mut scores = [0.0_f32; 3];
        for i in 0..3 {
            let energy_norm = band_energies[i] / energy_sum;
            let score = band_onset_periodicity[i] * band_onset_peakiness[i] * energy_norm;
            scores[i] = if score.is_finite() { score } else { 0.0 };
        }

        let score_sum = scores.iter().sum::<f32>();
        if !score_sum.is_finite() || score_sum <= 0.0 {
            return prev;
        }

        let target = match Self::softmax(scores, WEIGHT_SOFTMAX_TEMPERATURE) {
            Some(weights) => weights,
            None => return prev,
        };

        let mut weights = [0.0_f32; 3];
        for i in 0..3 {
            weights[i] = prev[i] + (target[i] - prev[i]) * WEIGHT_SMOOTH_ALPHA;
        }

        Self::normalize_weights(weights, prev)
    }

    fn softmax(scores: [f32; 3], temperature: f32) -> Option<[f32; 3]> {
        let temp = if temperature <= 0.0 { 1.0 } else { temperature };
        let inv_temp = 1.0 / temp;
        let mut max_score = f32::NEG_INFINITY;
        for score in scores {
            if score > max_score {
                max_score = score;
            }
        }
        if !max_score.is_finite() {
            return None;
        }

        let mut exps = [0.0_f32; 3];
        let mut sum = 0.0_f32;
        for (idx, score) in scores.iter().enumerate() {
            let value = ((*score - max_score) * inv_temp).exp();
            if !value.is_finite() {
                return None;
            }
            exps[idx] = value;
            sum += value;
        }

        if !sum.is_finite() || sum <= 0.0 {
            return None;
        }

        for value in &mut exps {
            *value /= sum;
        }

        Some(exps)
    }

    fn normalize_weights(weights: [f32; 3], fallback: [f32; 3]) -> [f32; 3] {
        let sum = weights.iter().sum::<f32>();
        if !sum.is_finite() || sum <= 0.0 {
            return fallback;
        }

        let mut normalized = [0.0_f32; 3];
        for i in 0..3 {
            normalized[i] = weights[i] / sum;
        }

        normalized
    }

    fn onset_periodicity(history: &[f32], sample_period_ms: usize) -> f32 {
        let n = history.len();
        if n < 2 || sample_period_ms == 0 {
            return 0.0;
        }

        let min_lag = ((60_000.0 / MAX_BPM) / sample_period_ms as f32)
            .round()
            .max(1.0) as usize;
        let max_lag = ((60_000.0 / MIN_BPM) / sample_period_ms as f32)
            .round()
            .max(1.0) as usize;

        if min_lag >= max_lag || n < max_lag * 2 {
            return 0.0;
        }

        let mean = history.iter().sum::<f32>() / n as f32;
        let mut best_corr = 0.0_f32;
        let mut sum_corr = 0.0_f32;
        let mut count = 0usize;

        for lag in min_lag..=max_lag {
            let mut corr = 0.0_f32;
            for i in 0..(n - lag) {
                let a = history[i] - mean;
                let b = history[i + lag] - mean;
                corr += a * b;
            }
            corr /= (n - lag) as f32;
            if corr > best_corr {
                best_corr = corr;
            }
            if corr > 0.0 {
                sum_corr += corr;
                count += 1;
            }
        }

        if count == 0 {
            return 0.0;
        }

        let avg_corr = sum_corr / count as f32;
        best_corr / (avg_corr + ONSET_METRIC_EPS)
    }

    fn onset_stats(history: &[f32]) -> (f32, f32, f32) {
        if history.is_empty() {
            return (0.0, 0.0, 0.0);
        }

        let mut sum = 0.0_f32;
        let mut max = 0.0_f32;
        for v in history {
            sum += *v;
            if *v > max {
                max = *v;
            }
        }
        let mean = sum / history.len() as f32;

        let mut var = 0.0_f32;
        for v in history {
            let d = *v - mean;
            var += d * d;
        }
        let std = (var / history.len() as f32).sqrt();

        (mean, std, max)
    }

    fn estimate_bpm_from_onset(history: &[f32], sample_period_ms: usize) -> Option<f32> {
        let n = history.len();
        if n < 2 || sample_period_ms == 0 {
            return None;
        }

        let min_lag = ((60_000.0 / MAX_BPM) / sample_period_ms as f32)
            .round()
            .max(1.0) as usize;
        let max_lag = ((60_000.0 / MIN_BPM) / sample_period_ms as f32)
            .round()
            .max(1.0) as usize;

        if min_lag >= max_lag || n < max_lag * 2 {
            return None;
        }

        let mean = history.iter().sum::<f32>() / n as f32;
        let mut best_lag = 0usize;
        let mut best_corr = 0.0_f32;

        for lag in min_lag..=max_lag {
            let mut corr = 0.0_f32;
            for i in 0..(n - lag) {
                let a = history[i] - mean;
                let b = history[i + lag] - mean;
                corr += a * b;
            }
            if corr > best_corr {
                best_corr = corr;
                best_lag = lag;
            }
        }

        if best_lag == 0 || !best_corr.is_finite() {
            return None;
        }

        let period_ms = best_lag as f32 * sample_period_ms as f32;
        let bpm = 60_000.0 / period_ms;

        if bpm.is_finite() {
            Some(bpm)
        } else {
            None
        }
    }

    fn gradient(data: &[f64], spacing: f64) -> Vec<f64> {
        let n = data.len();
        if n < 2 {
            panic!("List must have at least 2 numbers to calculate a derivative");
        }

        let mut deriv = Vec::with_capacity(n);

        // 1. First Point (Forward Difference)
        // Formula: (y[1] - y[0]) / h
        deriv.push((data[1] - data[0]) / spacing);

        // 2. Middle Points (Central Difference)
        // Formula: (y[i+1] - y[i-1]) / 2h
        for i in 1..n - 1 {
            let val = (data[i + 1] - data[i - 1]) / (2.0 * spacing);
            deriv.push(val);
        }

        // 3. Last Point (Backward Difference)
        // Formula: (y[n] - y[n-1]) / h
        deriv.push((data[n - 1] - data[n - 2]) / spacing);

        deriv
    }

    #[inline(always)]
    pub fn beat_volume(&mut self) -> anyhow::Result<()> {
        // 1. Clear buffer 2. Fill it up again
        self.scratch.beat_volume_volume_samples_buffer.clear();
        self.scratch.beat_volume_volume_samples_buffer.extend(
            self.freq_buffer
                .chunks(2)
                .map(|f| f.iter().map(|e| e.volume as usize).max().unwrap_or(0)),
        );

        // let curr: Vec<usize> = self
        //     .freq_buffer
        //     .chunks(2)
        //     // TODO: only look at the bass line?
        //     .map(|f| f.iter().map(|e| e.volume as usize).max().unwrap())
        //     .collect();

        let curr_unfiltered: usize = self.freq_buffer.iter().map(|f| f.volume as usize).sum();
        shift_push!(
            self.scratch.long_historic,
            LONG_HISTORIC_FRAMES,
            curr_unfiltered
        );

        let curr_max = self
            .scratch
            .beat_volume_volume_samples_buffer
            .iter()
            .max()
            .unwrap_or(&0);
        shift_push!(self.scratch.historic, ROLLING_AVERAGE_FRAMES, *curr_max);

        let min = self
            .scratch
            .historic
            .iter()
            .min()
            .unwrap_or(&usize::MIN)
            .to_owned();

        let max = self
            .scratch
            .historic
            .iter()
            .max()
            .unwrap_or(&usize::MAX)
            .to_owned()
            .max(min + 1);

        const MAX_BEAT_VOLUME: u8 = 255;

        let index_mapped = curr_max.map_range(min..max, 0..MAX_BEAT_VOLUME as usize);

        if self.scratch.last_index != index_mapped {
            self.scratch.last_index = index_mapped;
            self.send_signals(&[Signal::BeatVolume(index_mapped as u8)]);
        }

        Ok(())
    }

    #[inline(always)]
    pub fn volume(&mut self) -> anyhow::Result<()> {
        self.send_signals({
            let volume_mean = ((self.scratch.volume_samples.iter().sum::<usize>() as f32)
                / (self.scratch.volume_samples.len() as f32)
                * 10.0) as usize;

            // let volume_sum = self.freqs.iter().map(|f| f.volume).sum::<f32>() * 10.0;
            // let volume_avg = volume_sum / self.freqs.len() as f32;

            let volume = volume_mean as u8;
            &[Signal::Volume(volume)]
        });

        let curr_max = (self
            .freq_buffer
            .iter()
            // .max_by_key(|f| (f.volume * 10.0) as usize)
            // .unwrap_or(&Frequency {
            //     volume: 0f32,
            //     freq: 0f32,
            //     position: 0f32,
            // })
            .filter(|f| f.freq > 0.0)
            .map(|f| f.volume)
            .sum::<f32>()
            * 10.0
            / self.freq_buffer.iter().filter(|f| f.freq > 0.0).count() as f32)
            as usize;
        // .volume as usize;

        // TODO: this is fake, this is not even the average.

        shift_push!(
            self.scratch.volume_samples,
            ROLLING_AVERAGE_VOLUME_SAMPLE_SIZE,
            curr_max
        );

        Ok(())
    }
}
