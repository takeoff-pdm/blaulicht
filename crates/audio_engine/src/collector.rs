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
    AudioSource, Frequency, Signal, BASS_FRAMES, BASS_PEAK_FRAMES, LONG_HISTORIC_FRAMES,
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
    pub(crate) bass_peaks: VecDeque<usize>,
    pub(crate) time_of_last_bpm_marker: usize, // Time marker
    pub(crate) num_beat_mismatches: usize,
    pub(crate) beat_needs_sync: bool,

    pub(crate) is_on_beat: bool,
    pub(crate) actual_onset_peak: bool,

    // Onset + tempo tracking.
    pub(crate) onset_history: VecDeque<f32>,
    pub(crate) band_onset_history: [VecDeque<f32>; 3],
    pub(crate) last_onset_sample_time: usize,
    pub(crate) onset_ema: f32,
    pub(crate) band_energy_ema: [f32; 3],
    pub(crate) band_energy_ema_short: [f32; 3],
    pub(crate) band_energy_rise_ema: [f32; 3],
    pub(crate) band_energy_fall_ema: [f32; 3],
    pub(crate) band_transient_history: [VecDeque<(usize, f32)>; 3],
    pub(crate) band_weights: [f32; 3],
    pub(crate) beat_interval_ms: f32,
    pub(crate) bpm_estimate: f32,

    pub(crate) last_calibrate_time: usize,

    pub(crate) beat_volume_volume_samples_buffer: Vec<usize>,
}

#[derive(Clone, Copy)]
pub struct CollectorScratchParameters {
    pub volume_frames: usize,
    pub long_historic_frames: usize,
    pub rolling_frames: usize,
    pub bass_frames: usize,
    pub bass_peak_frames: usize,
}

impl Default for CollectorScratchParameters {
    fn default() -> Self {
        Self {
            volume_frames: ROLLING_AVERAGE_VOLUME_SAMPLE_SIZE,
            long_historic_frames: LONG_HISTORIC_FRAMES,
            rolling_frames: ROLLING_AVERAGE_FRAMES,
            bass_frames: BASS_FRAMES,
            bass_peak_frames: BASS_PEAK_FRAMES,
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
            bass_peaks: VecDeque::with_capacity(params.bass_peak_frames),
            time_of_last_bpm_marker: now,
            num_beat_mismatches: 0,
            is_on_beat: false,
            actual_onset_peak: false,
            beat_needs_sync: true,
            onset_history: VecDeque::new(),
            band_onset_history: [VecDeque::new(), VecDeque::new(), VecDeque::new()],
            last_onset_sample_time: now,
            onset_ema: 0.0,
            band_energy_ema: [0.0; 3],
            band_energy_ema_short: [0.0; 3],
            band_energy_rise_ema: [0.0; 3],
            band_energy_fall_ema: [0.0; 3],
            band_transient_history: [VecDeque::new(), VecDeque::new(), VecDeque::new()],
            band_weights: [0.6, 0.3, 0.1],
            beat_interval_ms: 0.0,
            bpm_estimate: 0.0,
            last_calibrate_time: now,
            beat_volume_volume_samples_buffer: vec![0; 2048], // TODO: make this more steerable?
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
            bass_volume: 100,
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
}

impl<const NUM_OUTPUTS: usize, SourceT> SignalCollector<NUM_OUTPUTS, SourceT>
where
    SourceT: AudioSource,
{
    //
    // NOTE: not idempotent.
    // If the audio snapshot includes the beat-trigger flag, it WILL BE cleared after a call to
    // this function.
    //
    pub fn take_snapshot(&self) -> CollectedAudioSnapshot {
        self.current.clone()
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
            // Signal::BassAvgShort(v) => {
            //     self.current.bass_avg_short = v;
            // }
            Signal::BassAvg(v) => {
                self.current.bass_avg = v;
            }
            Signal::DebugData(v) => {
                self.debug_data = v.clone();
            }
            Signal::Bpm(v) => {
                self.current.bpm = v.bpm;
                self.current.time_between_beats_millis = v.time_between_beats_millis;
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

        debug_assert!(freq_buffer_size > 0);

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

    fn get_frequencies(&mut self, now: usize) {
        let values_raw = self.audio_source.get_frequencies(now);

        // Copy into internal buffer.
        self.freq_buffer_raw.clear();
        self.freq_buffer_raw.extend(values_raw);

        self.freq_buffer.clear();
        self.freq_buffer.extend(values_raw);

        // self.freq_buffer_raw.copy_from_slice(values_raw);
        // self.freq_buffer.copy_from_slice(values_raw);

        //
        // Volume pass.
        //
        match self.params.volume {
            100 => {}
            adjust_percent => {
                self.freq_buffer.iter_mut().for_each(|freq| {
                    let adjust_percent_float = adjust_percent as f32 / 100.0;
                    freq.volume *= adjust_percent_float;
                });
            }
        }

        //
        // Gate pass.
        //
        match self.params.gate {
            0 => {}
            gate_min => {
                let min_freq = self.scratch.volume_samples.iter().max().unwrap_or(&0);
                let min_freq = *min_freq as f32 * (gate_min as f32 / 100.0);

                self.freq_buffer.iter_mut().for_each(|freq| {
                    if freq.volume >= min_freq {
                        match self.params.boost {
                            Some(b) => {
                                freq.volume *= (b as f32) / 100.0;
                            }
                            None => {}
                        }

                        return;
                    }

                    freq.volume = 0.0;
                });
            }
        }
    }

    pub fn clear(&mut self) {
        // self.freqs = vec![];
        self.scratch = CollectorScratch::new(self.scratch_params, 0);
        self.need_to_update_output_beat_trigger = [false; NUM_OUTPUTS];
        self.need_to_update_output_beat_onset = [false; NUM_OUTPUTS];
    }

    //
    // DOES NOT run every ~20 ms. This runs as often as possible.
    //
    pub fn tick(&mut self, now: u64) -> anyhow::Result<()> {
        let now = now as usize;

        if self.params.auto_calibrate {
            self.calibrate(now);
        }

        self.get_frequencies(now);

        // Volume
        self.volume()?;

        // Bass
        self.bass(now)?;

        // Beat Volume
        self.beat_volume()?;

        {
            // NOTE: this will cause a missing update if the consumer takes too long.
            // self.need_to_update_output_beat_trigger = [true; NUM_OUTPUTS];
            if self.scratch.is_on_beat {
                self.need_to_update_output_beat_trigger.fill(true);
                self.scratch.is_on_beat = false;
            }

            if self.scratch.actual_onset_peak {
                self.need_to_update_output_beat_onset.fill(true);
                self.scratch.actual_onset_peak = false;
            }
        }

        self.current.time += 1;

        Ok(())
    }

    // TODO: make this one mutable on the collector output.
    pub fn tick_output<const OUTPUT_INDEX: usize>(&mut self) -> CollectorOutput {
        let output_spec = self.outputs[OUTPUT_INDEX];

        let freqs = match output_spec.raw {
            true => &self.freq_buffer,
            false => &self.freq_buffer,
        };

        // TODO: this also allocates in a hot loop :/
        let current_audio_colunn = match output_spec.bins_p_column {
            Some(num_bins) => bin_spectrum_to_u8(freqs, num_bins),
            None => vec![],
        };

        let mut output = CollectorOutput {
            snapshot: self.take_snapshot(),
            debug_data: self.debug_data.clone(),
            current_audio_colunn,
        };

        // Ensure time-critical flags are set.
        if self.need_to_update_output_beat_trigger[OUTPUT_INDEX] {
            output.snapshot.beat_trigger = true;
            self.need_to_update_output_beat_trigger[OUTPUT_INDEX] = false;
        }

        if self.need_to_update_output_beat_onset[OUTPUT_INDEX] {
            output.snapshot.actual_onset_peak = true;
            self.need_to_update_output_beat_onset[OUTPUT_INDEX] = false;
        }

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

#[derive(Debug, Clone, Default, Serialize)]
pub struct SignalDebugData {
    pub bass_range: Range<f32>,
    pub band_energies: [f32; 3],
    pub band_onset_peakiness: [f32; 3],
    pub band_onset_periodicity: [f32; 3],
    pub band_transient_strength: [f32; 3],
    pub band_weights: [f32; 3],
}

pub type AudioColumn = Vec<AudioBucket>;

/// Needs to "summarize" the entire frequency spectrum into chunks
pub fn bin_spectrum_to_u8(values: &[Frequency], bins: usize) -> AudioColumn {
    debug_assert!(bins > 0);

    let chunk_size = match values.len() % bins == 0 {
        true => {
            let primitive = values.len() / bins;
            if primitive == 0 {
                1
            } else {
                primitive
            }
        }
        false => {
            // while values.len() % bins != 0 {
            //     bins -= 1
            // }

            let new = values.len() / bins;

            debug_assert!(new > 0);

            // println!("new len: {new}");
            new
        }
    };

    // println!("chunk size: {chunk_size}");

    // let chunk_size = values.len() as f32 / bins as f32;
    let chunks: Vec<AudioBucket> = values
        .chunks(chunk_size)
        .map(|c| {
            let volume = c
                .iter()
                .map(|datapoint| datapoint.volume * 15.0)
                .sum::<f32>()
                / c.len() as f32;

            let freq_low = c
                .iter()
                .map(|datapoint| datapoint.freq as u64)
                .min()
                .unwrap_or(0);

            let freq_high = c
                .iter()
                .map(|datapoint| datapoint.freq as u64)
                .max()
                .unwrap_or(freq_low + 1);

            (volume, freq_low, freq_high)
        })
        .map(|(vol, freq_low, freq_high)| AudioBucket {
            volume: vol as u8,
            freq_bound_lower: freq_low,
            freq_bound_upper: freq_high,
        })
        .collect();

    chunks
}
