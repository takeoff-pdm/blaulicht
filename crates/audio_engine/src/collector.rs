//
// Provides class for capturing audio.
// use audioviz::{
//     audio_capture::{capture::Capture, config::Config as CaptureConfig},
//     spectrum::{config::StreamConfig, stream::Stream},
// };
use blaulicht_shared::CollectedAudioSnapshot;
use serde::Serialize;
// use cpal::{traits::DeviceTrait, Device};
use crate::{
    AudioSource, Frequency, Signal, BASS_FRAMES, LONG_HISTORIC_FRAMES, ONSET_SAMPLE_PERIOD_MS,
    ROLLING_AVERAGE_FRAMES, ROLLING_AVERAGE_VOLUME_SAMPLE_SIZE,
};
use std::{collections::VecDeque, ops::Range};

// Exists for unifying the output used for the main engine (DMX + plugins) and the spectrogram.
// The problem is: both run at different refresh rates.
// Could just use the faster refresh rate for both; but this is more general.
#[derive(Debug, Clone, Copy)]
pub struct CollectorOutputSpec {
    // update_every: Duration,
    // last_update: Instant,
    pub bins_p_column: Option<usize>, // If None, no columns will be included
    pub raw: bool,                    // Whether to apply envelopes, etc.
}

impl Default for CollectorOutputSpec {
    fn default() -> Self {
        Self {
            // update_every: Default::default(),
            // last_update: Instant::now(),
            bins_p_column: Default::default(),
            raw: false,
        }
    }
}

#[derive(Default, Debug, Clone)]
pub struct CollectorOutput {
    pub snapshot: CollectedAudioSnapshot,
    pub current_audio_colunn: AudioColumn,
    pub debug_data: SignalDebugData,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct AudioEventCursor {
    pub beat_event_id: u64,
    pub onset_event_id: u64,
}

pub struct CollectorScratch {
    // Volume.
    pub(crate) time_of_last_volume_publish: usize, // Time marker
    pub(crate) volume_samples: VecDeque<usize>,

    // Beat
    pub(crate) time_of_last_beat_publish: usize, // Time marker
    pub(crate) last_index: usize,
    // rolling_average_frames = 100;
    // let long_historic_frames = rolling_average_frames * 1000;
    pub(crate) long_historic: VecDeque<usize>,
    pub(crate) historic: VecDeque<usize>,

    pub(crate) bass_samples: VecDeque<u8>,
    pub(crate) bass_samples_sum: u64,
    pub(crate) last_bass_gate_open_time: usize,
    pub(crate) time_of_last_bpm_marker: usize, // Time marker
    pub(crate) num_beat_mismatches: usize,
    pub(crate) beat_needs_sync: bool,

    pub(crate) is_on_beat: bool,
    pub(crate) actual_onset_peak: bool,

    // Onset + tempo tracking.
    pub(crate) onset_history: VecDeque<f32>,
    pub(crate) band_onset_history: [VecDeque<f32>; 3],
    pub(crate) last_onset_sample_time: usize,
    pub(crate) onset_sample_period_ema_ms: f32,
    pub(crate) onset_ema: f32,
    pub(crate) band_energy_ema: [f32; 3],
    pub(crate) band_energy_ema_short: [f32; 3],
    pub(crate) band_energy_rise_ema: [f32; 3],
    pub(crate) band_energy_fall_ema: [f32; 3],
    pub(crate) band_transient_history: [VecDeque<(usize, f32)>; 3],
    pub(crate) band_weights: [f32; 3],
    pub(crate) beat_interval_ms: f32,
    pub(crate) bpm_estimate: f32,
    pub(crate) bpm_confidence_ema: f32,
    pub(crate) bpm_detect_status: BpmDetectStatus,
    pub(crate) bass_avg_short: f32,

    // Cached per-frame analysis results, reused while no new FFT frame is available.
    pub(crate) max_freq_cached: Option<f32>,
    pub(crate) last_band_energies: [f32; 3],
    pub(crate) last_band_onset_peakiness: [f32; 3],
    pub(crate) last_band_onset_periodicity: [f32; 3],
    pub(crate) last_band_transient_strength: [f32; 3],

    pub(crate) last_calibrate_time: usize,
    pub(crate) last_frame_time: usize,

    pub(crate) beat_volume_volume_samples_buffer: Vec<usize>,

    // Section detection (Breakdown / Drop / ActiveBeat).
    pub(crate) section_state: blaulicht_shared::SectionState,
    pub(crate) section_last_update_ms: usize,
    pub(crate) section_drop_started_ms: usize,
    pub(crate) section_breakdown_started_ms: usize,
    pub(crate) section_breakdown_accum_ms: usize,
    pub(crate) section_active_accum_ms: usize,
    pub(crate) section_prev_bass_hit: bool,
    pub(crate) section_drop_confirm_started_ms: Option<usize>,
}

#[derive(Clone, Copy)]
pub struct CollectorScratchParameters {
    pub volume_frames: usize,
    pub long_historic_frames: usize,
    pub rolling_frames: usize,
    pub bass_frames: usize,
}

impl Default for CollectorScratchParameters {
    fn default() -> Self {
        Self {
            volume_frames: ROLLING_AVERAGE_VOLUME_SAMPLE_SIZE,
            long_historic_frames: LONG_HISTORIC_FRAMES,
            rolling_frames: ROLLING_AVERAGE_FRAMES,
            bass_frames: BASS_FRAMES,
        }
    }
}

impl CollectorScratch {
    fn new(params: CollectorScratchParameters, now: usize) -> Self {
        // let now = Instant::now();

        Self {
            time_of_last_volume_publish: now,
            volume_samples: VecDeque::with_capacity(params.volume_frames),
            time_of_last_beat_publish: now,
            last_index: 0,
            long_historic: VecDeque::with_capacity(params.long_historic_frames),
            historic: VecDeque::with_capacity(params.rolling_frames),
            bass_samples: VecDeque::with_capacity(params.bass_frames),
            bass_samples_sum: 0,
            last_bass_gate_open_time: now,
            time_of_last_bpm_marker: now,
            num_beat_mismatches: 0,
            is_on_beat: false,
            actual_onset_peak: false,
            beat_needs_sync: true,
            onset_history: VecDeque::new(),
            band_onset_history: [VecDeque::new(), VecDeque::new(), VecDeque::new()],
            last_onset_sample_time: now,
            onset_sample_period_ema_ms: ONSET_SAMPLE_PERIOD_MS as f32,
            onset_ema: 0.0,
            band_energy_ema: [0.0; 3],
            band_energy_ema_short: [0.0; 3],
            band_energy_rise_ema: [0.0; 3],
            band_energy_fall_ema: [0.0; 3],
            band_transient_history: [VecDeque::new(), VecDeque::new(), VecDeque::new()],
            band_weights: [0.6, 0.3, 0.1],
            beat_interval_ms: 0.0,
            bpm_estimate: 0.0,
            bpm_confidence_ema: 0.0,
            bpm_detect_status: BpmDetectStatus::default(),
            bass_avg_short: 0.0,
            max_freq_cached: None,
            last_band_energies: [0.0; 3],
            last_band_onset_peakiness: [0.0; 3],
            last_band_onset_periodicity: [0.0; 3],
            last_band_transient_strength: [0.0; 3],
            last_calibrate_time: now,
            last_frame_time: now,
            beat_volume_volume_samples_buffer: vec![0; 2048], // TODO: make this more steerable?
            section_state: blaulicht_shared::SectionState::default(),
            section_last_update_ms: now,
            section_drop_started_ms: now,
            section_breakdown_started_ms: now,
            section_breakdown_accum_ms: 0,
            section_active_accum_ms: 0,
            section_prev_bass_hit: false,
            section_drop_confirm_started_ms: None,
        }
    }
}

#[derive(Clone, Copy)]
pub struct SignalCollectorParams {
    pub volume: u8,
    pub gate: u8,
    pub boost: Option<u8>,
    pub auto_calibrate: bool,
    pub auto_weight: bool,
    pub changed: bool,

    // Bass recognition parameters.
    pub bass_freq_low: usize,
    pub bass_freq_high: usize,
    pub bass_volume: usize,

    /// Drop-detection sensitivity, 0..=100. Higher triggers more easily (lower
    /// bass-onset threshold + shorter required preceding quiet), at the cost of
    /// catching a DJ's fake/prank drop. Lower is safer but later.
    pub drop_sensitivity: u8,
    /// Minimum instantaneous `bass` level that counts as bass being present.
    pub drop_bass_min: u8,
    /// Minimum `bass_avg` (moving average) level that counts as bass being present.
    pub drop_bass_avg_min: u8,
    /// If true, both `drop_bass_min` AND `drop_bass_avg_min` must be met for bass
    /// to count as present; if false, either one is enough.
    pub drop_require_both: bool,
    /// How long (ms) bass must stay at/above the gates after a candidate hit
    /// before committing to `Drop` (the sustain that "creates the effect").
    pub drop_sustain_ms: u16,
    /// How long (ms) bass must stay below the gates before falling back to
    /// `Breakdown`.
    pub drop_breakdown_hold_ms: u16,
    /// Breakdown sensitivity, 0..=100. Sets how far bass must drop *below* the
    /// presence gates to count as gone: 100 = any dip below the gate counts
    /// (eager), 0 = bass must vanish entirely (lazy). Creates hysteresis between
    /// entering ActiveBeat and falling back to Breakdown.
    pub breakdown_sensitivity: u8,
    /// If true, also count toward breakdown when the BPM/periodicity confidence
    /// falls below `breakdown_bpm_confidence_min` (rhythm lost), not just on bass
    /// level. Catches beatless sections that still carry bass.
    pub breakdown_on_low_bpm: bool,
    /// Confidence threshold (0..=100, % of normalized confidence) below which the
    /// rhythm counts as lost for breakdown when `breakdown_on_low_bpm` is set.
    pub breakdown_bpm_confidence_min: u8,
    /// If true, a high bass-band onset *peakiness* can also arm a drop, in
    /// addition to the raw onset jump.
    pub drop_use_peakiness: bool,
    /// Bass-band peakiness (max/mean of onset flux) at or above which a drop is
    /// armed when `drop_use_peakiness` is set.
    pub drop_peakiness_min: u8,
}

impl Default for SignalCollectorParams {
    fn default() -> Self {
        Self {
            volume: 100,
            gate: 0,
            boost: None,
            auto_calibrate: false,
            auto_weight: false,
            changed: false,

            bass_freq_low: 0,
            bass_freq_high: 250,
            bass_volume: 25,
            drop_sensitivity: 70,
            drop_bass_min: 25,
            drop_bass_avg_min: 15,
            drop_require_both: false,
            drop_sustain_ms: 280,
            drop_breakdown_hold_ms: 2500,
            breakdown_sensitivity: 100,
            breakdown_on_low_bpm: false,
            breakdown_bpm_confidence_min: 40,
            drop_use_peakiness: false,
            drop_peakiness_min: 6,
        }
    }
}

pub struct SignalCollector<const NUM_OUTPUTS: usize, SourceT>
where
    SourceT: AudioSource,
{
    pub freq_buffer: Vec<Frequency>,
    pub freq_buffer_raw: Vec<Frequency>,

    pub audio_source: SourceT,
    // pub freqs: Vec<Frequency>,
    // pub freqs_raw: Vec<Frequency>, // Without transformations.
    pub current: CollectedAudioSnapshot,
    pub debug_data: SignalDebugData,
    pub params: SignalCollectorParams,
    pub scratch_params: CollectorScratchParameters,
    pub scratch: CollectorScratch,
    pub outputs: [CollectorOutputSpec; NUM_OUTPUTS],
    pub need_to_update_output_beat_trigger: [bool; NUM_OUTPUTS],
    pub need_to_update_output_beat_onset: [bool; NUM_OUTPUTS],
    pub output_event_cursors: [AudioEventCursor; NUM_OUTPUTS],
}

impl<const NUM_OUTPUTS: usize, SourceT> SignalCollector<NUM_OUTPUTS, SourceT>
where
    SourceT: AudioSource,
{
    /// Returns continuous values without consuming transient events.
    /// Consumers should use `snapshot_for` when they need beat/onset delivery.
    pub fn take_snapshot(&self) -> CollectedAudioSnapshot {
        let mut snapshot = self.current.clone();
        snapshot.beat_trigger = false;
        snapshot.actual_onset_peak = false;
        snapshot
    }

    pub fn snapshot_for(&self, cursor: &mut AudioEventCursor) -> CollectedAudioSnapshot {
        let mut snapshot = self.take_snapshot();
        snapshot.beat_trigger =
            self.current.beat_event_id != 0 && self.current.beat_event_id != cursor.beat_event_id;
        snapshot.actual_onset_peak = self.current.onset_event_id != 0
            && self.current.onset_event_id != cursor.onset_event_id;
        cursor.beat_event_id = self.current.beat_event_id;
        cursor.onset_event_id = self.current.onset_event_id;
        snapshot
    }

    pub fn send_signals(&mut self, signals: &[Signal]) {
        for s in signals {
            self.signal(s.clone());
        }
    }

    pub fn signal(&mut self, signal: Signal) {
        match signal {
            Signal::Volume(v) => {
                self.current.volume = v;
            }
            Signal::BeatVolume(v) => {
                self.current.beat_volume = v;
            }
            Signal::BeatTrigger(is_trigger) => {
                self.current.beat_trigger = is_trigger;
            }
            Signal::Bass(v) => {
                self.current.bass = v;
            }
            Signal::BassAvgShort(v) => {
                self.current.bass_avg_short = v;
            }
            Signal::BassAvg(v) => {
                self.current.bass_avg = v;
            }
            Signal::DebugData(v) => {
                self.debug_data = v.clone();
            }
            Signal::Bpm(v) => {
                self.current.bpm = v.bpm;
                self.current.time_between_beats_millis = v.time_between_beats_millis;
                self.current.bpm_confidence = v.confidence;
            }
            Signal::Section(s) => {
                self.current.section_state = s;
            }
        }
    }

    pub fn new(
        params: SignalCollectorParams,
        outputs: [CollectorOutputSpec; NUM_OUTPUTS],
        scratch_params: CollectorScratchParameters,
        audio_source: SourceT,
        now: usize,
        // TODO: breaking -> the freq buffer size is obtained automatically from the source.
        // freq_buffer_size: usize,
    ) -> anyhow::Result<Self> {
        let freq_buffer_size = audio_source.get_freq_buffer_size();

        if freq_buffer_size == 0 {
            anyhow::bail!("audio source returned a zero-sized frequency buffer");
        }

        Ok(Self {
            params,
            scratch_params,
            freq_buffer: vec![Frequency::default(); freq_buffer_size],
            freq_buffer_raw: vec![Frequency::default(); freq_buffer_size],
            current: CollectedAudioSnapshot::default(),
            debug_data: SignalDebugData::default(),
            scratch: CollectorScratch::new(scratch_params, now),
            outputs,
            need_to_update_output_beat_trigger: [true; NUM_OUTPUTS],
            need_to_update_output_beat_onset: [true; NUM_OUTPUTS],
            output_event_cursors: [AudioEventCursor::default(); NUM_OUTPUTS],
            audio_source,
        })
    }

    fn calibrate(&mut self, now: usize) {
        if now - self.scratch.last_calibrate_time > 1000 {
            println!("Calibration is new.");
            self.params.gate = 60;
            self.params.changed = true;
        }

        // Progressively decrement the gate until we get a BPM.
        if now - self.scratch.last_calibrate_time > 100 {
            if self.current.bpm == 0.0 && self.current.bass_avg > 50 {
                let last_gate = match self.params.gate {
                    0 => 60,
                    v => v,
                };

                println!("Gate: {last_gate}");

                self.params.gate = last_gate - 1;
                self.params.changed = true;
            } else if self.current.bpm > 0.0 {
                self.params.auto_calibrate = false;
            }

            self.scratch.last_calibrate_time = now;
        }
    }

    fn get_frequencies(&mut self, now: usize) -> bool {
        let (values_raw, has_new) = self.audio_source.get_frequencies(now);

        if !has_new {
            return false;
        }

        self.current.source_status = blaulicht_shared::AudioSourceStatus::Active;
        self.current.frame_age_ms = 0;
        self.scratch.last_frame_time = now;

        self.freq_buffer_raw.clear();
        self.freq_buffer_raw.extend(values_raw);

        self.freq_buffer.clear();
        self.freq_buffer.extend(values_raw);
        for freq in &mut self.freq_buffer {
            if !freq.volume.is_finite() || !freq.freq.is_finite() || !freq.position.is_finite() {
                *freq = Frequency::default();
            } else {
                freq.volume = freq.volume.max(0.0);
            }
        }

        match self.params.volume {
            100 => {}
            adjust_percent => {
                let adjust = adjust_percent as f32 / 100.0;
                self.freq_buffer.iter_mut().for_each(|freq| {
                    freq.volume *= adjust;
                });
            }
        }

        match self.params.gate {
            0 => {}
            gate_min => {
                let min_freq = self.scratch.volume_samples.iter().max().unwrap_or(&0);
                let min_freq = *min_freq as f32 * (gate_min as f32 / 100.0);
                let boost = self.params.boost;

                self.freq_buffer.iter_mut().for_each(|freq| {
                    if freq.volume >= min_freq {
                        if let Some(b) = boost {
                            freq.volume *= (b as f32) / 100.0;
                        }
                        return;
                    }
                    freq.volume = 0.0;
                });
            }
        }

        true
    }

    pub fn clear(&mut self) {
        // self.freqs = vec![];
        self.scratch = CollectorScratch::new(self.scratch_params, 0);
        self.need_to_update_output_beat_trigger = [false; NUM_OUTPUTS];
        self.need_to_update_output_beat_onset = [false; NUM_OUTPUTS];
        self.output_event_cursors = [AudioEventCursor::default(); NUM_OUTPUTS];
    }

    //
    // Runs as often as the mainloop calls it. Heavy analysis is gated on
    // whether the underlying audio source produced a new FFT frame; the cheap
    // beat-scheduling logic still runs every tick so predicted beats stay
    // aligned to wall-clock time.
    //
    pub fn tick(&mut self, now: u64) -> anyhow::Result<()> {
        let now = now as usize;

        // Event booleans describe this analysis tick only. Consumers use the
        // event IDs to retain delivery semantics across different tick rates.
        self.current.beat_trigger = false;
        self.current.actual_onset_peak = false;

        if self.params.auto_calibrate {
            self.calibrate(now);
        }

        let has_new_frame = self.get_frequencies(now);

        if !has_new_frame {
            self.current.source_status = blaulicht_shared::AudioSourceStatus::NoFrame;
            self.current.frame_age_ms = now
                .saturating_sub(self.scratch.last_frame_time)
                .min(u32::MAX as usize) as u32;
            if self.current.frame_age_ms >= 500 {
                self.current.source_status = blaulicht_shared::AudioSourceStatus::Disconnected;
                self.current.volume = 0;
                self.current.beat_volume = 0;
                self.current.bass = 0;
                self.current.bass_avg = 0;
                self.current.bass_avg_short = 0;
                self.current.bpm = 0.0;
                self.current.time_between_beats_millis = 0;
                self.scratch.beat_interval_ms = 0.0;
            }
        }

        if has_new_frame {
            self.volume()?;
            self.beat_volume()?;
        }

        // Once the source has been silent long enough to be considered gone,
        // do not let the predictor synthesize beats from stale history.
        if has_new_frame
            || self.current.source_status != blaulicht_shared::AudioSourceStatus::Disconnected
        {
            self.bass(now, has_new_frame)?;
        }

        if self.scratch.is_on_beat {
            self.current.beat_event_id = self.current.beat_event_id.wrapping_add(1).max(1);
            self.need_to_update_output_beat_trigger.fill(true);
            self.scratch.is_on_beat = false;
        }

        if self.scratch.actual_onset_peak {
            self.current.onset_event_id = self.current.onset_event_id.wrapping_add(1).max(1);
            self.need_to_update_output_beat_onset.fill(true);
            self.scratch.actual_onset_peak = false;
        }

        if has_new_frame {
            self.current.time += 1;
        }

        Ok(())
    }

    // TODO: make this one mutable on the collector output.
    pub fn tick_output<const OUTPUT_INDEX: usize>(&mut self) -> CollectorOutput {
        let output_spec = self.outputs[OUTPUT_INDEX];

        let freqs = match output_spec.raw {
            true => &self.freq_buffer_raw,
            false => &self.freq_buffer,
        };

        // TODO: this also allocates in a hot loop :/
        let current_audio_colunn = match output_spec.bins_p_column {
            Some(num_bins) => bin_spectrum_to_u8(freqs, num_bins),
            None => vec![],
        };

        let mut cursor = self.output_event_cursors[OUTPUT_INDEX];
        let snapshot = self.snapshot_for(&mut cursor);
        self.output_event_cursors[OUTPUT_INDEX] = cursor;

        let output = CollectorOutput {
            snapshot,
            debug_data: self.debug_data.clone(),
            current_audio_colunn,
        };

        // Ensure time-critical flags are set.
        output
    }
}

//
// Spectrogram utils.
//

#[derive(Debug, Clone, Default, Serialize)]
pub struct AudioBucket {
    pub volume: u8,
    pub freq_bound_lower: u64,
    pub freq_bound_upper: u64,
}

/// Why the tempo estimator is (or isn't) producing a fresh BPM this frame.
/// Surfaced in the audio-page info popup so it's clear which gating-chain
/// element is currently missing. See `SignalCollector::estimate_bpm_from_onset`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub enum BpmDetectStatus {
    /// Producing a fresh tempo estimate from the onset autocorrelation.
    Detecting,
    /// No band energy above the noise floor — nothing to analyze (BPM gate closed).
    NoEnergy,
    /// Onset history still filling. `have` samples collected, `need` required
    /// before autocorrelation can run.
    Warmup { have: usize, need: usize },
    /// Onset envelope too flat (no transients) to find any period.
    FlatOnset,
    /// A period was found but its autocorrelation peak is below the confidence
    /// threshold, so the previous estimate is held instead.
    WeakPeriodicity { strength: f32, threshold: f32 },
}

impl Default for BpmDetectStatus {
    fn default() -> Self {
        Self::NoEnergy
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct SignalDebugData {
    pub bass_range: Range<f32>,
    pub band_energies: [f32; 3],
    pub band_onset_peakiness: [f32; 3],
    pub band_onset_periodicity: [f32; 3],
    pub band_transient_strength: [f32; 3],
    pub band_weights: [f32; 3],
    /// Live tempo-detection gating state (warmup / flat onset / weak periodicity).
    pub bpm_status: BpmDetectStatus,
    /// Latched tempo estimate at the time of the snapshot (0 = none yet).
    pub bpm_estimate: f32,
}

pub type AudioColumn = Vec<AudioBucket>;

/// Needs to "summarize" the entire frequency spectrum into chunks
pub fn bin_spectrum_to_u8(values: &[Frequency], bins: usize) -> AudioColumn {
    if bins == 0 || values.is_empty() {
        return Vec::new();
    }

    let chunk_size = (values.len() / bins).max(1);

    // println!("chunk size: {chunk_size}");

    // let chunk_size = values.len() as f32 / bins as f32;
    let chunks: Vec<AudioBucket> = values
        .chunks(chunk_size)
        .map(|c| {
            let volume = c
                .iter()
                .filter(|datapoint| datapoint.volume.is_finite())
                .map(|datapoint| datapoint.volume.max(0.0) * 15.0)
                .sum::<f32>()
                / c.len().max(1) as f32;

            let freq_low = c
                .iter()
                .filter(|datapoint| datapoint.freq.is_finite())
                .map(|datapoint| datapoint.freq.max(0.0) as u64)
                .min()
                .unwrap_or(0);

            let freq_high = c
                .iter()
                .filter(|datapoint| datapoint.freq.is_finite())
                .map(|datapoint| datapoint.freq.max(0.0) as u64)
                .max()
                .unwrap_or(freq_low + 1);

            (volume, freq_low, freq_high)
        })
        .map(|(vol, freq_low, freq_high)| AudioBucket {
            volume: vol.clamp(0.0, u8::MAX as f32) as u8,
            freq_bound_lower: freq_low,
            freq_bound_upper: freq_high,
        })
        .collect();

    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binning_handles_more_bins_than_input() {
        let values = [Frequency {
            volume: 1.0,
            freq: 100.0,
            position: 0.0,
        }];

        let output = bin_spectrum_to_u8(&values, 128);

        assert_eq!(output.len(), 1);
    }

    #[test]
    fn binning_handles_empty_input_and_zero_bins() {
        assert!(bin_spectrum_to_u8(&[], 128).is_empty());
        assert!(bin_spectrum_to_u8(&[Frequency::default()], 0).is_empty());
    }

    #[test]
    fn binning_sanitizes_non_finite_values() {
        let output = bin_spectrum_to_u8(
            &[Frequency {
                volume: f32::NAN,
                freq: f32::INFINITY,
                position: f32::NEG_INFINITY,
            }],
            1,
        );

        assert_eq!(output.len(), 1);
        assert_eq!(output[0].volume, 0);
        assert_eq!(output[0].freq_bound_lower, 0);
        assert_eq!(output[0].freq_bound_upper, 1);
    }

    #[test]
    fn event_delivery_is_once_per_consumer() {
        let source = StaticSourceForTest::default();
        let mut collector = SignalCollector::new(
            SignalCollectorParams::default(),
            [CollectorOutputSpec::default()],
            CollectorScratchParameters::default(),
            source,
            0,
        )
        .unwrap();
        collector.current.beat_event_id = 7;
        collector.current.onset_event_id = 3;

        let mut first_consumer = AudioEventCursor::default();
        let first = collector.snapshot_for(&mut first_consumer);
        let second = collector.snapshot_for(&mut first_consumer);
        assert!(first.beat_trigger && first.actual_onset_peak);
        assert!(!second.beat_trigger && !second.actual_onset_peak);

        let mut independent_consumer = AudioEventCursor::default();
        let independent = collector.snapshot_for(&mut independent_consumer);
        assert!(independent.beat_trigger && independent.actual_onset_peak);
    }

    #[test]
    fn disconnected_source_is_neutralized_without_predicted_events() {
        let mut collector = SignalCollector::new(
            SignalCollectorParams::default(),
            [CollectorOutputSpec::default()],
            CollectorScratchParameters::default(),
            StaticSourceForTest::default(),
            0,
        )
        .unwrap();
        collector.current.bpm = 120.0;
        collector.scratch.beat_interval_ms = 500.0;

        collector.tick(500).unwrap();

        assert_eq!(
            collector.current.source_status,
            blaulicht_shared::AudioSourceStatus::Disconnected
        );
        assert_eq!(collector.current.volume, 0);
        assert_eq!(collector.current.bpm, 0.0);
        assert_eq!(collector.current.beat_event_id, 0);
    }

    #[test]
    fn zero_sized_source_is_rejected() {
        let result = SignalCollector::new(
            SignalCollectorParams::default(),
            [CollectorOutputSpec::default()],
            CollectorScratchParameters::default(),
            EmptySourceForTest,
            0,
        );

        assert!(result.is_err());
    }

    #[derive(Default)]
    struct StaticSourceForTest;

    impl AudioSource for StaticSourceForTest {
        fn get_freq_buffer_size(&self) -> usize {
            1
        }
        fn get_frequencies(&mut self, _now: usize) -> (&[Frequency], bool) {
            static FREQS: [Frequency; 1] = [Frequency {
                volume: 0.0,
                freq: 0.0,
                position: 0.0,
            }];
            (&FREQS, false)
        }
    }

    struct EmptySourceForTest;

    impl AudioSource for EmptySourceForTest {
        fn get_freq_buffer_size(&self) -> usize {
            0
        }

        fn get_frequencies(&mut self, _now: usize) -> (&[Frequency], bool) {
            (&[], false)
        }
    }
}
