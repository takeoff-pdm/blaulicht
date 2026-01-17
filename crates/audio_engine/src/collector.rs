//
// Provides class for capturing audio.
use anyhow::{anyhow, Context};
// use audioviz::{
//     audio_capture::{capture::Capture, config::Config as CaptureConfig},
//     spectrum::{config::StreamConfig, stream::Stream},
// };
use blaulicht_shared::CollectedAudioSnapshot;
// use cpal::{traits::DeviceTrait, Device};
use crate::{AudioSource, Frequency, Signal};
use std::collections::VecDeque;

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

    pub(crate) last_calibrate_time: usize,
}

#[derive(Clone, Copy)]
pub struct CollectorScratchParameters {
    pub volume_frames: usize,
    pub long_historic_frames: usize,
    pub rolling_frames: usize,
    pub bass_frames: usize,
    pub bass_peak_frames: usize,
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
            beat_needs_sync: true,
            last_calibrate_time: now,
        }
    }
}

#[derive(Clone, Copy)]
pub struct SignalCollectorParams {
    pub volume: u8,
    pub gate: u8,
    pub boost: Option<u8>,
    pub auto_calibrate: bool,
    pub changed: bool,
}

impl Default for SignalCollectorParams {
    fn default() -> Self {
        Self {
            volume: 100,
            gate: 0,
            boost: None,
            auto_calibrate: false,
            changed: false,
        }
    }
}

pub struct SignalCollector<const NUM_OUTPUTS: usize, SourceT>
where
    SourceT: AudioSource,
{
    pub audio_source: SourceT,
    pub freqs: Vec<Frequency>,
    pub freqs_raw: Vec<Frequency>, // Without transformations.
    pub current: CollectedAudioSnapshot,
    pub params: SignalCollectorParams,
    pub scratch_params: CollectorScratchParameters,
    pub scratch: CollectorScratch,
    pub outputs: [CollectorOutputSpec; NUM_OUTPUTS],
    pub need_to_update_output_beat_trigger: [bool; NUM_OUTPUTS],
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
        self.current
    }

    pub fn send_signals(&mut self, signals: &[Signal]) {
        for s in signals {
            self.signal(*s);
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
    ) -> anyhow::Result<Self> {
        Ok(Self {
            params,
            scratch_params,
            freqs: vec![],
            freqs_raw: vec![],
            // converter,
            // _capture,
            current: CollectedAudioSnapshot::default(),
            scratch: CollectorScratch::new(scratch_params, now),
            outputs,
            need_to_update_output_beat_trigger: [true; NUM_OUTPUTS],
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
            if self.current.bpm == 0 && self.current.bass_avg > 50 {
                let last_gate = match self.params.gate {
                    0 => 60,
                    v => v,
                };

                println!("Gate: {last_gate}");

                self.params.gate = last_gate - 1;
                self.params.changed = true;
            } else if self.current.bpm > 0 {
                self.params.auto_calibrate = false;
            }

            self.scratch.last_calibrate_time = now;
        }
    }

    fn get_frequencies(&mut self, now: usize) {
        let values_raw = self.audio_source.get_frequencies(now);

        self.freqs_raw = values_raw.clone();

        let values_vol_adjusted = match self.params.volume {
            100 => values_raw,
            adjust_percent => values_raw
                .into_iter()
                .map(|freq| {
                    let adjust_percent_float = adjust_percent as f32 / 100.0;
                    Frequency {
                        volume: freq.volume * adjust_percent_float,
                        freq: freq.freq,
                        position: freq.position,
                    }
                })
                .collect(),
        };

        // TODO: would outsource into function.

        let values = match self.params.gate {
            0 => values_vol_adjusted,
            gate_min => values_vol_adjusted
                .into_iter()
                .map(|freq| match (freq.volume * 10.0) >= gate_min as f32 {
                    true => match self.params.boost {
                        Some(b) => Frequency {
                            volume: freq.volume + ((b as f32) / 10.0),
                            freq: freq.freq,
                            position: freq.position,
                        },
                        None => freq,
                    },
                    false => Frequency {
                        volume: 0.0,
                        freq: freq.freq,
                        position: freq.position,
                    },
                })
                .collect(),
        };

        self.freqs = values;
        // self.current.initialized = true;
    }

    pub fn clear(&mut self) {
        self.freqs = vec![];
        self.scratch = CollectorScratch::new(self.scratch_params, 0);
        self.need_to_update_output_beat_trigger = [false; NUM_OUTPUTS]
    }

    //
    // DOES NOT run every ~20 ms. This runs as often as possible.
    //
    pub fn tick(&mut self, now: usize) -> anyhow::Result<()> {
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

        if self.scratch.is_on_beat {
            // println!("activate on beat flag");
            // self.scratch.is_on_beat = false;
            // NOTE: this will cause a missing update if the consumer takes too long.
            // self.need_to_update_output_beat_trigger = [true; NUM_OUTPUTS];
            self.need_to_update_output_beat_trigger.fill(true);
            self.scratch.is_on_beat = false;
        }

        self.current.time += 1;

        Ok(())
    }

    // fn tick_outputs(&mut self, freqs: &[Frequency], now: Instant) {
    //     let snapshot = self.take_snapshot();
    //
    //     for (output_spec, output) in self.outputs.iter_mut() {
    //         if output_spec.last_update.elapsed() < output_spec.update_every {
    //             continue;
    //         }
    //
    //         output_spec.last_update = now;
    //
    //         match output_spec.bins_p_column {
    //             Some(num_bins) => {
    //                 let new_column = bin_spectrum_to_u8(&freqs, num_bins);
    //                 output.current_audio_colunn = new_column;
    //             }
    //             None => {}
    //         }
    //
    //         output.snapshot = snapshot;
    //
    //         // Ensure time-critical flags are set.
    //         if self.scratch.is_on_beat_pending_updates > 0 {
    //             output.snapshot.beat_trigger = true;
    //             self.scratch.is_on_beat_pending_updates -= 1;
    //         }
    //     }
    // }

    pub fn tick_output<const OUTPUT_INDEX: usize>(&mut self) -> CollectorOutput {
        let output_spec = self.outputs[OUTPUT_INDEX];

        let freqs = match output_spec.raw {
            true => &self.freqs_raw,
            false => &self.freqs,
        };

        let current_audio_colunn = match output_spec.bins_p_column {
            Some(num_bins) => bin_spectrum_to_u8(freqs, num_bins),
            None => vec![],
        };

        let mut output = CollectorOutput {
            snapshot: self.take_snapshot(),
            current_audio_colunn,
        };

        // Ensure time-critical flags are set.
        if self.need_to_update_output_beat_trigger[OUTPUT_INDEX] {
            output.snapshot.beat_trigger = true;
            self.need_to_update_output_beat_trigger[OUTPUT_INDEX] = false;
            // println!("included beat trigger")
        }

        output
    }
}

//
// Spectrogram utils.
//

pub type AudioColumn = Vec<u8>;

/// Needs to "summarize" the entire frequency spectrum into chunks
pub fn bin_spectrum_to_u8(values: &[Frequency], mut bins: usize) -> AudioColumn {
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
    let chunks: Vec<u8> = values
        .chunks(chunk_size)
        .map(|c| {
            // let mut max = 0.0;
            // for f in c {
            //     if f.volume > max {
            //         max = f.volume;
            //     }
            // }

            // println!("MAX: {max}");

            c.iter()
                .map(|datapoint| datapoint.volume * 10.0)
                .sum::<f32>()
                / c.len() as f32
        })
        .map(|v| {
            // debug_assert!(if v > u8::MAX as f32 { panic!("V too large: {v}") } else { true });
            v as u8
        })
        .collect();

    // let data = [1, 2, 3, 4, 5, 6, 7];

    // let sums: Vec<i32> = data
    //     .chunks(3)
    //     .map(|chunk| chunk.iter().sum()) // map each chunk to its sum
    //     .collect();

    chunks
}
