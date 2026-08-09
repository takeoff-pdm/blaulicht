//
// Provides analysis on the audio.
//

use crate::{AudioSource, BpmDetectStatus, BpmInfo, Signal, SignalCollector, SignalDebugData};
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CollectorOutputSpec, CollectorScratchParameters, SignalCollectorParams};

    #[derive(Clone)]
    struct StaticAudioSource {
        freqs: Vec<Frequency>,
    }

    impl AudioSource for StaticAudioSource {
        fn get_freq_buffer_size(&self) -> usize {
            self.freqs.len()
        }

        fn get_frequencies(&mut self, _now: usize) -> (&[Frequency], bool) {
            (&self.freqs, true)
        }
    }

    fn frequency(freq: f32, volume: f32) -> Frequency {
        Frequency {
            volume,
            freq,
            position: 0.0,
        }
    }

    fn collector_with_freqs(
        freqs: Vec<Frequency>,
        bass_volume: usize,
    ) -> SignalCollector<1, StaticAudioSource> {
        let source = StaticAudioSource {
            freqs: freqs.clone(),
        };
        let mut collector = SignalCollector::new(
            SignalCollectorParams {
                bass_volume,
                ..SignalCollectorParams::default()
            },
            [CollectorOutputSpec::default()],
            CollectorScratchParameters::default(),
            source,
            0,
        )
        .unwrap();
        collector.freq_buffer = freqs;
        collector
    }

    #[test]
    fn quiet_bass_holds_bpm_and_resyncs_beat() {
        let mut collector = collector_with_freqs(
            vec![
                frequency(60.0, 0.2),
                frequency(500.0, 8.0),
                frequency(4_000.0, 8.0),
            ],
            100,
        );
        let known_bpm = 123.0;
        collector.scratch.bpm_estimate = known_bpm;
        collector.scratch.beat_interval_ms = 60_000.0 / known_bpm;
        collector.scratch.beat_needs_sync = false;
        collector.scratch.last_onset_sample_time = 0;
        collector.scratch.onset_history.extend([0.1, 0.2, 0.1]);

        collector.bass(ONSET_SAMPLE_PERIOD_MS, true).unwrap();

        // BPM gate is open (mid/high has energy), so onset_history grows,
        // but BPM estimate holds because the autocorrelation on a tiny history
        // can't produce a peak above confidence threshold.
        assert!((collector.scratch.bpm_estimate - known_bpm).abs() < f32::EPSILON);
        assert!((collector.current.bpm - known_bpm).abs() < f32::EPSILON);
        assert_eq!(
            collector.current.time_between_beats_millis,
            (60_000.0_f32 / known_bpm).round() as u16
        );
        assert!(collector.current.bass_avg < collector.params.bass_volume as u8);
    }

    #[test]
    fn loud_bass_allows_onset_history_updates() {
        let mut collector = collector_with_freqs(
            vec![
                frequency(60.0, 2.0),
                frequency(500.0, 1.0),
                frequency(4_000.0, 1.0),
            ],
            100,
        );
        collector.scratch.last_onset_sample_time = 0;

        collector.bass(ONSET_SAMPLE_PERIOD_MS, true).unwrap();

        assert_eq!(collector.scratch.onset_history.len(), 1);
        assert!(collector.current.bass_avg >= collector.params.bass_volume as u8);
    }

    #[test]
    fn first_tempo_lock_anchors_at_now_without_replaying_backlog() {
        let mut collector =
            collector_with_freqs(vec![frequency(60.0, 2.0), frequency(500.0, 1.0)], 100);
        collector.scratch.bpm_estimate = 150.0;

        collector.bass(22_630, true).unwrap();

        assert!(collector.scratch.beat_scheduler_initialized);
        assert_eq!(collector.scratch.beat_marker_ms, 22_630.0);
        assert!(!collector.scratch.is_on_beat);
    }

    #[test]
    fn overdue_scheduler_emits_at_most_one_current_beat() {
        let mut collector = collector_with_freqs(vec![frequency(60.0, 2.0)], 100);
        collector.scratch.beat_scheduler_initialized = true;
        collector.scratch.beat_interval_ms = 400.0;
        collector.scratch.beat_marker_ms = 0.0;
        collector.scratch.last_bass_gate_open_time = 5_000;

        collector.schedule_beat(5_000, false);
        assert!(collector.scratch.is_on_beat);
        assert_eq!(collector.scratch.beat_marker_ms, 4_800.0);

        collector.scratch.is_on_beat = false;
        collector.schedule_beat(5_010, false);
        assert!(!collector.scratch.is_on_beat);
        assert_eq!(collector.scratch.beat_marker_ms, 4_800.0);
    }

    #[test]
    fn onset_inside_refractory_window_corrects_without_duplicate() {
        let mut collector = collector_with_freqs(vec![frequency(60.0, 2.0)], 100);
        collector.scratch.beat_scheduler_initialized = true;
        collector.scratch.beat_interval_ms = 400.0;
        collector.scratch.beat_marker_ms = 400.0;
        collector.scratch.time_of_last_bpm_marker = 400;
        collector.scratch.beat_needs_sync = true;

        collector.schedule_beat(420, true);

        assert!(!collector.scratch.is_on_beat);
        assert!(!collector.scratch.actual_onset_peak);
        assert!((collector.scratch.beat_marker_ms - 402.0).abs() < 0.001);
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
pub const BASS_FRAMES: usize = 300;
// Short bass EMA alpha — at ~23ms frame rate this gives ~2.8s time constant.
const BASS_SHORT_ALPHA: f32 = 0.015;
pub const ONSET_SAMPLE_PERIOD_MS: usize = 10;
pub const ONSET_HISTORY_FRAMES: usize = 600;
const TRANSIENT_HISTORY_MS: usize = 10_000;
const ONSET_EMA_ALPHA: f32 = 0.2;
const ONSET_LONG_ALPHA: f32 = 0.01;
const ONSET_PEAK_STDDEV: f32 = 1.5;
const DEFAULT_BAND_WEIGHTS: [f32; 3] = [0.6, 0.3, 0.1];
const PHASE_WINDOW_FRACTION: f32 = 0.2;
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
// EMA weight for BPM updates. Low value yields steady BPM over a few seconds of beats.
const BPM_EMA_ALPHA: f32 = 0.05;
// Treat the prediction path as silent (and skip firing) once this many beat
// intervals of bass-gate-closed have passed.
const SILENCE_BEAT_MULTIPLIER: f32 = 2.0;
// Rayleigh tempo-prior center (BPM). The autocorrelation is multiplied by a
// Rayleigh window whose mode sits at this lag, biasing ambiguous resolutions
// toward a plausible tempo without overriding strong evidence elsewhere.
// Standard Davies & Plumbley weighting; librosa uses a log-normal variant.
const TEMPO_PRIOR_CENTER_BPM: f32 = 120.0;
// Minimum weighted autocorrelation peak to accept a new BPM estimate. The
// weighted score includes a tempo prior, so quiet or heavily layered tracks
// can have a stable rhythm below the old 0.01 cutoff. 0.008 retains the
// confidence gate while admitting those tracks without accepting near-zero
// correlation noise.
const BPM_CONFIDENCE_THRESHOLD: f32 = 0.008;
const MIN_BPM: f32 = 80.0;
const MAX_BPM: f32 = 200.0;
const MID_FREQ_HIGH: f32 = 2000.0;
const HIGH_FREQ_HIGH: f32 = 8000.0;

// TODO: what is this constant even
pub const ROLLING_AVERAGE_VOLUME_SAMPLE_SIZE: usize = 100;
pub const ROLLING_AVERAGE_FRAMES: usize = 100;
pub const LONG_HISTORIC_FRAMES: usize = ROLLING_AVERAGE_FRAMES * 1000;

// --- Section (Drop / Breakdown / ActiveBeat) detection ---
// How long the transient `Drop` state is held before settling into `ActiveBeat`.
const DROP_DURATION_MS: usize = 5000;
// Required preceding quiet before a sudden bass can count as a drop. Sensitivity
// interpolates between MAX (safe, needs a long quiet) and MIN (sensitive).
const DROP_MIN_QUIET_MIN_MS: usize = 3000;
const DROP_MIN_QUIET_MAX_MS: usize = 8000;
// If bass_avg_short stays above the breakdown gate for this long while in
// Breakdown, transition directly to ActiveBeat (no Drop needed).
const ACTIVE_BEAT_DIRECT_MS: usize = 5000;
// If bass goes quiet again within the confirm window, abandon the candidate
// (this is the prank/fake-drop rejection).
// Bass-onset (log-flux) threshold at sensitivity 0; scales toward 0 as
// sensitivity rises so a louder jump is required when less sensitive.
const DROP_ONSET_THRESH_MAX: f32 = 0.5;
// Raw autocorrelation-peak strength treated as "fully confident" when
// normalizing BPM confidence to 0..1 for display/use.
const BPM_CONFIDENCE_REF: f32 = 0.05;
// EMA smoothing for the (normalized) BPM confidence signal.
const BPM_CONFIDENCE_ALPHA: f32 = 0.05;

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
    /// Classify the macro musical section (Breakdown / Drop / ActiveBeat).
    ///
    /// `bass_onset` is the bass-band log-flux for this frame. "Bass present" is
    /// judged from the explicit `drop_bass_min` / `drop_bass_avg_min` thresholds
    /// combined per the `drop_require_both` toggle. Runs on the FFT-frame cadence.
    /// See the `DROP_*` / `SECTION_*` constants for the timing tuning.
    fn update_section(
        &mut self,
        now: usize,
        bass_sig: u8,
        bass_avg_short: f32,
        bass_onset: f32,
        bass_peakiness: f32,
        bpm_confidence: f32,
    ) {
        use blaulicht_shared::SectionState;

        let dt = now.saturating_sub(self.scratch.section_last_update_ms);
        self.scratch.section_last_update_ms = now;

        // Sensitivity (0..=100) -> timing/contrast thresholds. Higher = easier.
        let sens = (self.params.drop_sensitivity.min(100) as f32) / 100.0;
        let lerp =
            |max: usize, min: usize| -> usize { (max as f32 - sens * (max - min) as f32) as usize };
        let min_quiet_ms = lerp(DROP_MIN_QUIET_MAX_MS, DROP_MIN_QUIET_MIN_MS);
        let onset_thresh = (1.0 - sens) * DROP_ONSET_THRESH_MAX;

        // Explicit durations (ms).
        let confirm_ms = self.params.drop_sustain_ms as usize;
        let breakdown_hold_ms = self.params.drop_breakdown_hold_ms as usize;

        // Drop pre-condition: must have been in Breakdown for at least min_quiet_ms.
        let in_breakdown_ms = now.saturating_sub(self.scratch.section_breakdown_started_ms);
        let was_breakdown_long_enough = self.scratch.section_state == SectionState::Breakdown
            && in_breakdown_ms >= min_quiet_ms;

        let bass_hit = bass_sig >= self.params.drop_bass_min;
        let rising = bass_hit && !self.scratch.section_prev_bass_hit;
        let strong_onset = bass_onset >= onset_thresh
            || (self.params.drop_use_peakiness
                && bass_peakiness >= self.params.drop_peakiness_min as f32);

        // Breakdown: if bass_avg_short stays below the threshold for the hold
        // duration, we're in a breakdown. The threshold (breakdown_sensitivity,
        // 0..=255, default 100) is compared directly against bass_avg_short.
        let breakdown_threshold = self.params.breakdown_sensitivity as f32;
        let breakdown_bass_present = bass_avg_short >= breakdown_threshold;
        // Optionally, weak rhythm (low BPM confidence) also counts as "not active"
        // for breakdown, so beatless-but-bassy sections fall back too.
        let rhythm_lost = self.params.breakdown_on_low_bpm
            && bpm_confidence * 100.0 < self.params.breakdown_bpm_confidence_min as f32;
        if breakdown_bass_present && !rhythm_lost {
            self.scratch.section_breakdown_accum_ms = 0;
            self.scratch.section_active_accum_ms =
                self.scratch.section_active_accum_ms.saturating_add(dt);
        } else {
            self.scratch.section_breakdown_accum_ms =
                self.scratch.section_breakdown_accum_ms.saturating_add(dt);
            self.scratch.section_active_accum_ms = 0;
        }

        let beats_regular = self.scratch.beat_interval_ms > 0.0
            && (now.saturating_sub(self.scratch.time_of_last_bpm_marker) as f32)
                < SILENCE_BEAT_MULTIPLIER * self.scratch.beat_interval_ms;
        let bass_gone = self.scratch.section_breakdown_accum_ms >= breakdown_hold_ms;

        match self.scratch.section_state {
            SectionState::Breakdown => match self.scratch.section_drop_confirm_started_ms {
                Some(started) => {
                    if !bass_hit {
                        self.scratch.section_drop_confirm_started_ms = None;
                    } else if now.saturating_sub(started) >= confirm_ms && beats_regular {
                        self.scratch.section_state = SectionState::Drop;
                        self.scratch.section_drop_started_ms = now;
                        self.scratch.section_drop_confirm_started_ms = None;
                        self.scratch.section_breakdown_accum_ms = 0;
                    }
                }
                None => {
                    if rising && was_breakdown_long_enough && strong_onset {
                        self.scratch.section_drop_confirm_started_ms = Some(now);
                    } else if self.scratch.section_active_accum_ms >= ACTIVE_BEAT_DIRECT_MS {
                        self.scratch.section_state = SectionState::ActiveBeat;
                        self.scratch.section_breakdown_accum_ms = 0;
                    }
                }
            },
            SectionState::Drop => {
                if bass_gone {
                    self.scratch.section_state = SectionState::Breakdown;
                    self.scratch.section_breakdown_started_ms = now;
                } else if now.saturating_sub(self.scratch.section_drop_started_ms)
                    >= DROP_DURATION_MS
                {
                    self.scratch.section_state = SectionState::ActiveBeat;
                }
            }
            SectionState::ActiveBeat => {
                if bass_gone {
                    self.scratch.section_state = SectionState::Breakdown;
                    self.scratch.section_breakdown_started_ms = now;
                }
            }
        }

        self.scratch.section_prev_bass_hit = bass_hit;
    }

    pub fn bass(&mut self, now: usize, has_new_frame: bool) -> anyhow::Result<()> {
        let mut onset_peak = false;
        let mut bass_sig = self.current.bass;
        let mut bass_moving_average = self.current.bass_avg as f64;

        if has_new_frame {
            let lower_volume_limit = self.params.bass_volume as f64;
            let bass_low = self.params.bass_freq_low as f32;
            let bass_high = self.params.bass_freq_high as f32;

            // Cache max_freq: the FFT bin frequencies are fixed per stream.
            let max_freq = match self.scratch.max_freq_cached {
                Some(v) => v,
                None => {
                    let v = self
                        .freq_buffer
                        .iter()
                        .fold(0.0_f32, |acc, f| acc.max(f.freq));
                    self.scratch.max_freq_cached = Some(v);
                    v
                }
            };

            let mid_high = MID_FREQ_HIGH.min(max_freq);
            let high_high = HIGH_FREQ_HIGH.min(max_freq);

            // Single-pass: bass average + per-band energies.
            let mut band_energies = [0.0_f32; 3];
            let mut bass_count: u32 = 0;
            for f in &self.freq_buffer {
                if f.freq >= bass_low && f.freq < bass_high {
                    band_energies[0] += f.volume;
                    bass_count += 1;
                } else if f.freq >= bass_high && f.freq < mid_high {
                    band_energies[1] += f.volume;
                } else if f.freq >= mid_high && f.freq < high_high {
                    band_energies[2] += f.volume;
                }
            }

            let avg = if bass_count > 0 {
                band_energies[0] / bass_count as f32
            } else {
                0.0
            };
            bass_sig = (avg * 100.0) as u8;

            // Short bass EMA for responsive section detection.
            self.scratch.bass_avg_short +=
                BASS_SHORT_ALPHA * (bass_sig as f32 - self.scratch.bass_avg_short);

            // Incremental moving average over bass_samples (avoids re-summing the deque).
            self.scratch.bass_samples.push_back(bass_sig);
            self.scratch.bass_samples_sum += bass_sig as u64;
            if self.scratch.bass_samples.len() > BASS_FRAMES {
                if let Some(old) = self.scratch.bass_samples.pop_front() {
                    self.scratch.bass_samples_sum =
                        self.scratch.bass_samples_sum.saturating_sub(old as u64);
                }
            }
            bass_moving_average = if self.scratch.bass_samples.is_empty() {
                0.0
            } else {
                self.scratch.bass_samples_sum as f64 / self.scratch.bass_samples.len() as f64
            };

            let bass_gate_open =
                bass_moving_average >= lower_volume_limit || bass_sig as f64 >= lower_volume_limit;
            if bass_gate_open {
                self.scratch.last_bass_gate_open_time = now;
            }

            // BPM gate: any band has energy above a noise floor. Decoupled from
            // the strict bass gate so pre-drop hi-hats/snares still feed the
            // tempo estimator even when bass is absent.
            let total_energy: f32 = band_energies.iter().sum();
            let bpm_gate_open = total_energy > ONSET_METRIC_EPS;

            // Per-band onsets (log-magnitude spectral flux) + transient EMAs.
            // Log-compressed flux produces sharp onset peaks even from quiet
            // hi-hats during pre-drops, where linear flux would be negligible.
            let mut band_onsets = [0.0_f32; 3];
            for idx in 0..3 {
                let energy = band_energies[idx];
                let long_prev = self.scratch.band_energy_ema[idx];
                if long_prev <= 0.0 {
                    self.scratch.band_energy_ema[idx] = energy;
                } else {
                    let log_energy = (1.0 + energy).ln();
                    let log_prev = (1.0 + long_prev).ln();
                    band_onsets[idx] = (log_energy - log_prev).max(0.0);
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

            // Per-band periodicity drives adaptive ODF weighting: bands with
            // clearer periodic content contribute more to the BPM input signal.
            let mut band_onset_peakiness = [0.0_f32; 3];
            let mut band_onset_periodicity = [0.0_f32; 3];
            if bpm_gate_open {
                let period_for_periodicity =
                    self.scratch.onset_sample_period_ema_ms.round().max(1.0) as usize;
                for (idx, history) in self.scratch.band_onset_history.iter_mut().enumerate() {
                    band_onset_peakiness[idx] = Self::onset_peakiness(history.make_contiguous());
                    band_onset_periodicity[idx] =
                        Self::onset_periodicity(history.make_contiguous(), period_for_periodicity);
                }
            }

            let band_weights = if self.params.auto_weight {
                if bass_gate_open {
                    let updated = Self::compute_band_weights(
                        self.scratch.band_weights,
                        band_energies,
                        band_onset_peakiness,
                        band_onset_periodicity,
                    );
                    self.scratch.band_weights = updated;
                    updated
                } else {
                    self.scratch.band_weights
                }
            } else {
                self.scratch.band_weights = DEFAULT_BAND_WEIGHTS;
                DEFAULT_BAND_WEIGHTS
            };
            let _ = band_weights;

            // Adaptive spectral-flux ODF: weight each band's onset by its
            // periodicity score. Bands with stronger periodic content contribute
            // more to BPM detection. Falls back to equal bass+low-mid weighting
            // when periodicity data isn't available yet.
            let flux = if bpm_gate_open {
                let p_sum = band_onset_periodicity.iter().sum::<f32>();
                if p_sum > ONSET_METRIC_EPS {
                    let w0 = band_onset_periodicity[0] / p_sum;
                    let w1 = band_onset_periodicity[1] / p_sum;
                    let w2 = band_onset_periodicity[2] / p_sum;
                    band_onsets[0] * w0 + band_onsets[1] * w1 + band_onsets[2] * w2
                } else {
                    band_onsets[0] + band_onsets[1]
                }
            } else {
                0.0
            };
            // Smoothed flux drives the peak-picking threshold for the
            // phase-locked beat scheduler downstream; the autocorrelation gets
            // the raw signal.
            self.scratch.onset_ema += ONSET_EMA_ALPHA * (flux - self.scratch.onset_ema);

            let mut bpm_from_onset: Option<(f32, f32)> = None;

            let elapsed_since_sample = now - self.scratch.last_onset_sample_time;
            if elapsed_since_sample >= ONSET_SAMPLE_PERIOD_MS {
                // Track the true inter-sample period: onset samples arrive at the FFT
                // frame rate (~23ms), not the 10ms minimum gate. The autocorrelation
                // needs this to map lags to BPM correctly. Alpha 0.02 ≈ 1s settling.
                let elapsed_f = elapsed_since_sample as f32;
                self.scratch.onset_sample_period_ema_ms = self.scratch.onset_sample_period_ema_ms
                    + 0.02 * (elapsed_f - self.scratch.onset_sample_period_ema_ms);
                self.scratch.last_onset_sample_time = now;
                if bpm_gate_open {
                    self.scratch.onset_history.push_back(flux);
                    if self.scratch.onset_history.len() > ONSET_HISTORY_FRAMES {
                        self.scratch.onset_history.pop_front();
                    }
                    for idx in 0..3 {
                        let history = &mut self.scratch.band_onset_history[idx];
                        history.push_back(band_onsets[idx]);
                        if history.len() > ONSET_HISTORY_FRAMES {
                            history.pop_front();
                        }
                    }

                    let actual_period =
                        self.scratch.onset_sample_period_ema_ms.round().max(1.0) as usize;
                    let history = self.scratch.onset_history.make_contiguous();
                    let (estimate, status) = Self::estimate_bpm_from_onset(history, actual_period);
                    bpm_from_onset = estimate;
                    self.scratch.bpm_detect_status = status;
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
                } else {
                    self.scratch.beat_needs_sync = true;
                    self.scratch.bpm_detect_status = BpmDetectStatus::NoEnergy;
                }

                // Track normalized periodicity confidence. Silence / weak
                // periodicity pulls it toward 0; a strong autocorrelation peak
                // toward 1.
                let conf_target = match bpm_from_onset {
                    Some((_, raw)) => (raw / BPM_CONFIDENCE_REF).clamp(0.0, 1.0),
                    None => 0.0,
                };
                self.scratch.bpm_confidence_ema +=
                    BPM_CONFIDENCE_ALPHA * (conf_target - self.scratch.bpm_confidence_ema);
            }

            if let Some((new_bpm, _conf)) = bpm_from_onset {
                let new_bpm = new_bpm.clamp(MIN_BPM, MAX_BPM);
                if self.scratch.bpm_estimate <= 0.0 {
                    self.scratch.bpm_estimate = new_bpm;
                } else {
                    self.scratch.bpm_estimate = self.scratch.bpm_estimate
                        + BPM_EMA_ALPHA * (new_bpm - self.scratch.bpm_estimate);
                }
            }

            if self.scratch.bpm_estimate > 0.0 {
                self.scratch.beat_interval_ms = 60000.0 / self.scratch.bpm_estimate;
                if !self.scratch.beat_scheduler_initialized {
                    // A tempo can lock several seconds after startup. Anchor at lock time
                    // instead of replaying every beat between process start and now.
                    self.scratch.beat_marker_ms = now as f64;
                    self.scratch.time_of_last_bpm_marker = now;
                    self.scratch.beat_scheduler_initialized = true;
                    self.scratch.beat_needs_sync = true;
                }
            }

            // Trim transient history + compute average strength for debug data.
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

            self.scratch.last_band_energies = band_energies;
            self.scratch.last_band_onset_peakiness = band_onset_peakiness;
            self.scratch.last_band_onset_periodicity = band_onset_periodicity;
            self.scratch.last_band_transient_strength = band_transient_strength;

            // Classify the macro section from the bass band. Uses the per-frame
            // bass level + moving average + bass onset (+ optional peakiness /
            // BPM confidence); beat regularity comes from the scheduler state.
            self.update_section(
                now,
                bass_sig,
                self.scratch.bass_avg_short,
                band_onsets[0],
                band_onset_peakiness[0],
                self.scratch.bpm_confidence_ema,
            );
        }

        self.schedule_beat(now, onset_peak);

        if has_new_frame {
            let bpm_f32 = self.scratch.bpm_estimate;
            let time_between_beats_millis = if self.scratch.beat_interval_ms > 0.0 {
                self.scratch.beat_interval_ms.round() as u16
            } else {
                0
            };

            let debug_data = SignalDebugData {
                bass_range: (self.params.bass_freq_low as f32)..(self.params.bass_freq_high as f32),
                band_energies: self.scratch.last_band_energies,
                band_onset_peakiness: self.scratch.last_band_onset_peakiness,
                band_onset_periodicity: self.scratch.last_band_onset_periodicity,
                band_transient_strength: self.scratch.last_band_transient_strength,
                band_weights: self.scratch.band_weights,
                bpm_status: self.scratch.bpm_detect_status,
                bpm_estimate: self.scratch.bpm_estimate,
            };

            self.send_signals(&[
                Signal::BeatTrigger(self.scratch.is_on_beat),
                Signal::Bass(bass_sig),
                Signal::BassAvgShort(self.scratch.bass_avg_short as u8),
                Signal::Bpm(BpmInfo {
                    bpm: bpm_f32,
                    time_between_beats_millis,
                    confidence: self.scratch.bpm_confidence_ema,
                }),
                Signal::BassAvg(bass_moving_average as u8),
                Signal::DebugData(debug_data),
                Signal::Section(self.scratch.section_state),
            ]);
        }

        Ok(())
    }

    fn schedule_beat(&mut self, now: usize, onset_peak: bool) {
        if !self.scratch.beat_scheduler_initialized || self.scratch.beat_interval_ms <= 0.0 {
            return;
        }

        let now_f = now as f64;
        let interval = self.scratch.beat_interval_ms as f64;
        let expected = self.scratch.beat_marker_ms + interval;
        let window = (interval * PHASE_WINDOW_FRACTION as f64).max(MIN_PHASE_WINDOW_MS as f64);
        let refractory = (interval * 0.35).max(MIN_PHASE_WINDOW_MS as f64);

        if onset_peak {
            let since_last = (now_f - self.scratch.beat_marker_ms).max(0.0);
            let phase_error = now_f - expected;
            if since_last >= refractory
                && (self.scratch.beat_needs_sync || phase_error.abs() <= window)
            {
                self.scratch.is_on_beat = true;
                self.scratch.actual_onset_peak = true;
                self.scratch.beat_marker_ms = now_f;
                self.scratch.time_of_last_bpm_marker = now;
                self.scratch.beat_needs_sync = false;
            } else if since_last < refractory && self.scratch.beat_needs_sync {
                // A predicted beat already fired. Pull its anchor toward the nearby
                // onset without publishing a second beat event.
                self.scratch.beat_marker_ms += since_last * PHASE_CORRECTION as f64;
                self.scratch.time_of_last_bpm_marker =
                    self.scratch.beat_marker_ms.max(0.0).round() as usize;
            }
        }

        if !self.scratch.is_on_beat && now_f >= expected {
            // Jump directly to the most recent due beat. This emits at most one
            // event after a scheduler stall and never drains a historical backlog.
            let periods_due = ((now_f - self.scratch.beat_marker_ms) / interval)
                .floor()
                .max(1.0);
            let expected_marker = self.scratch.beat_marker_ms + periods_due * interval;
            let silence_threshold =
                (SILENCE_BEAT_MULTIPLIER * self.scratch.beat_interval_ms) as usize;
            let elapsed_silence = now.saturating_sub(self.scratch.last_bass_gate_open_time);

            if elapsed_silence < silence_threshold {
                self.scratch.is_on_beat = true;
                if !onset_peak {
                    self.scratch.beat_needs_sync = true;
                }
            } else {
                self.scratch.beat_needs_sync = true;
            }
            self.scratch.beat_marker_ms = expected_marker;
            self.scratch.time_of_last_bpm_marker = expected_marker.round() as usize;
        }
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

    /// Returns the tempo estimate (if any) plus a status describing which gating
    /// element is currently active, for the audio-page diagnostics readout.
    fn estimate_bpm_from_onset(
        history: &[f32],
        sample_period_ms: usize,
    ) -> (Option<(f32, f32)>, BpmDetectStatus) {
        let n = history.len();
        if sample_period_ms == 0 {
            return (None, BpmDetectStatus::Warmup { have: n, need: 0 });
        }

        let min_lag = ((60_000.0 / MAX_BPM) / sample_period_ms as f32)
            .round()
            .max(1.0) as usize;
        let max_lag = ((60_000.0 / MIN_BPM) / sample_period_ms as f32)
            .round()
            .max(1.0) as usize;

        // Need at least two full periods of the slowest tempo before the
        // autocorrelation has anything to lock onto.
        let need = max_lag * 2;
        if n < 2 || min_lag >= max_lag || n < need {
            return (None, BpmDetectStatus::Warmup { have: n, need });
        }

        let mean = history.iter().sum::<f32>() / n as f32;
        let mut variance = 0.0_f32;
        for v in history {
            let d = *v - mean;
            variance += d * d;
        }
        variance /= n as f32;
        if !variance.is_finite() || variance <= ONSET_METRIC_EPS {
            return (None, BpmDetectStatus::FlatOnset);
        }
        let inv_variance = 1.0 / variance;

        // Rayleigh tempo prior: nudges ambiguous lags toward TEMPO_PRIOR_CENTER_BPM
        // without overriding strong evidence at other tempos. Davies & Plumbley 2007.
        // σ in lag-space puts the prior's mode at the center BPM.
        let sigma_lag = (60_000.0 / TEMPO_PRIOR_CENTER_BPM) / sample_period_ms as f32;
        let sigma_sq = sigma_lag * sigma_lag;

        // Normalize each lag's correlation by (pairs * variance) so longer lags (which
        // average over fewer pairs) and louder passages don't dominate the choice,
        // then multiply by the Rayleigh prior before picking argmax.
        let mut corrs = Vec::with_capacity(max_lag - min_lag + 1);
        let mut best_lag = min_lag;
        let mut best_weighted = f32::NEG_INFINITY;
        for lag in min_lag..=max_lag {
            let pairs = n - lag;
            let mut corr = 0.0_f32;
            for i in 0..pairs {
                let a = history[i] - mean;
                let b = history[i + lag] - mean;
                corr += a * b;
            }
            let normalized = (corr * inv_variance) / pairs as f32;
            let lag_f = lag as f32;
            let prior = (lag_f / sigma_sq) * (-(lag_f * lag_f) / (2.0 * sigma_sq)).exp();
            let weighted = normalized * prior;
            corrs.push(weighted);
            if weighted > best_weighted {
                best_weighted = weighted;
                best_lag = lag;
            }
        }

        if !best_weighted.is_finite() || best_weighted <= 0.0 {
            return (
                None,
                BpmDetectStatus::WeakPeriodicity {
                    strength: best_weighted.max(0.0),
                    threshold: BPM_CONFIDENCE_THRESHOLD,
                },
            );
        }

        // Confidence gate: if the winning peak is too weak, the signal lacks
        // clear periodicity — return None to hold the previous BPM estimate.
        if best_weighted < BPM_CONFIDENCE_THRESHOLD {
            return (
                None,
                BpmDetectStatus::WeakPeriodicity {
                    strength: best_weighted,
                    threshold: BPM_CONFIDENCE_THRESHOLD,
                },
            );
        }

        // Parabolic interpolation around the winning peak: gives sub-bin lag accuracy,
        // which matters because at 10 ms steps each bin is ~5 BPM wide near 120 BPM.
        let lag_idx = best_lag - min_lag;
        let refined_lag = if lag_idx > 0 && lag_idx + 1 < corrs.len() {
            let y_minus = corrs[lag_idx - 1];
            let y_0 = corrs[lag_idx];
            let y_plus = corrs[lag_idx + 1];
            let denom = y_minus - 2.0 * y_0 + y_plus;
            if denom.abs() > ONSET_METRIC_EPS {
                let delta = 0.5 * (y_minus - y_plus) / denom;
                best_lag as f32 + delta.clamp(-0.5, 0.5)
            } else {
                best_lag as f32
            }
        } else {
            best_lag as f32
        };

        let period_ms = refined_lag * sample_period_ms as f32;
        if period_ms <= 0.0 {
            return (None, BpmDetectStatus::FlatOnset);
        }
        let bpm = 60_000.0 / period_ms;
        if bpm.is_finite() && bpm > 0.0 {
            // Second element is the winning autocorrelation-peak strength, used as
            // a periodicity-confidence measure.
            (Some((bpm, best_weighted)), BpmDetectStatus::Detecting)
        } else {
            (None, BpmDetectStatus::FlatOnset)
        }
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
        let volume_mean = if self.scratch.volume_samples.is_empty() {
            0
        } else {
            ((self.scratch.volume_samples.iter().sum::<usize>() as f32)
                / self.scratch.volume_samples.len() as f32
                * 10.0) as usize
        };
        self.send_signals({
            // let volume_sum = self.freqs.iter().map(|f| f.volume).sum::<f32>() * 10.0;
            // let volume_avg = volume_sum / self.freqs.len() as f32;

            let volume = volume_mean as u8;
            &[Signal::Volume(volume)]
        });

        let positive_count = self.freq_buffer.iter().filter(|f| f.freq > 0.0).count();
        let curr_max = if positive_count == 0 {
            0
        } else {
            (self
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
                / positive_count as f32) as usize
        };
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
