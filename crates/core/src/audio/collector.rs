//
// Provides class for capturing audio.
//
use crate::audio::analysis::{
    BASS_FRAMES, BASS_PEAK_FRAMES, LONG_HISTORIC_FRAMES, ROLLING_AVERAGE_FRAMES,
    ROLLING_AVERAGE_VOLUME_SAMPLE_SIZE,
};
use crate::{audio::defs::AudioConverter, msg::Signal};
use anyhow::{anyhow, Context};
use audioviz::spectrum::Frequency;
use audioviz::{
    audio_capture::{capture::Capture, config::Config as CaptureConfig},
    spectrum::{config::StreamConfig, stream::Stream},
};
use blaulicht_shared::CollectedAudioSnapshot;
use cpal::{traits::DeviceTrait, Device};
use std::collections::VecDeque;
use std::time::{Duration, Instant};

// Exists for unifying the output used for the main engine (DMX + plugins) and the spectrogram.
// The problem is: both run at different refresh rates.
// Could just use the faster refresh rate for both; but this is more general.
#[derive(Debug, Clone, Copy)]
pub struct CollectorOutputSpec {
    // update_every: Duration,
    // last_update: Instant,
    pub(crate) bins_p_column: Option<usize>, // If None, no columns will be included
}

impl Default for CollectorOutputSpec {
    fn default() -> Self {
        Self {
            // update_every: Default::default(),
            // last_update: Instant::now(),
            bins_p_column: Default::default(),
        }
    }
}

#[derive(Default, Debug, Clone)]
pub struct CollectorOutput {
    pub(crate) snapshot: CollectedAudioSnapshot,
    pub(crate) current_audio_colunn: AudioColumn,
}

pub struct CollectorScratch {
    // Volume.
    pub(crate) time_of_last_volume_publish: Instant,
    pub(crate) volume_samples: VecDeque<usize>,

    // Beat
    pub(crate) time_of_last_beat_publish: Instant,
    pub(crate) last_index: usize,
    // rolling_average_frames = 100;
    // let long_historic_frames = rolling_average_frames * 1000;
    pub(crate) long_historic: VecDeque<usize>,
    pub(crate) historic: VecDeque<usize>,

    pub(crate) bass_samples: VecDeque<u8>,
    pub(crate) bass_peaks: VecDeque<Instant>,
    pub(crate) time_of_last_bpm_marker: Instant,
    pub(crate) num_beat_mismatches: usize,
    pub(crate) beat_needs_sync: bool,

    pub(crate) is_on_beat: bool,
}

impl CollectorScratch {
    fn new() -> Self {
        let now = Instant::now();

        Self {
            time_of_last_volume_publish: now,
            volume_samples: VecDeque::with_capacity(ROLLING_AVERAGE_VOLUME_SAMPLE_SIZE),
            time_of_last_beat_publish: now,
            last_index: 0,
            long_historic: VecDeque::with_capacity(LONG_HISTORIC_FRAMES),
            historic: VecDeque::with_capacity(ROLLING_AVERAGE_FRAMES),
            bass_samples: VecDeque::with_capacity(BASS_FRAMES),
            bass_peaks: VecDeque::with_capacity(BASS_PEAK_FRAMES),
            time_of_last_bpm_marker: now,
            num_beat_mismatches: 0,
            is_on_beat: false,
            beat_needs_sync: true,
        }
    }
}

#[derive(Default, Clone, Copy)]
pub struct SignalCollectorParams {
    pub(crate) gate: Option<u8>,
    pub(crate) boost: Option<u8>,
}

pub struct SignalCollector<const NUM_OUTPUTS: usize> {
    pub(crate) freqs: Vec<Frequency>,
    pub(crate) current: CollectedAudioSnapshot,
    pub(crate) converter: AudioConverter,
    capture: Capture,
    pub(crate) params: SignalCollectorParams,
    pub(crate) scratch: CollectorScratch,
    pub(crate) outputs: [CollectorOutputSpec; NUM_OUTPUTS],
    pub(crate) need_to_update_output_beat_trigger: [bool; NUM_OUTPUTS],
}

impl<const NUM_OUTPUTS: usize> SignalCollector<NUM_OUTPUTS> {
    //
    // NOTE: not idempotent.
    // If the audio snapshot includes the beat-trigger flag, it WILL BE cleared after a call to
    // this function.
    //
    pub fn take_snapshot(&self) -> CollectedAudioSnapshot {
        self.current
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
        device: Device,
        config: StreamConfig,
        params: SignalCollectorParams,
        outputs: [CollectorOutputSpec; NUM_OUTPUTS],
    ) -> anyhow::Result<Self> {
        let (converter, capture) = init_converter(device, config)
            .with_context(|| "Failed to initialize audio converter")?;

        Ok(Self {
            params,
            freqs: vec![],
            converter,
            capture,
            current: CollectedAudioSnapshot::default(),
            scratch: CollectorScratch::new(),
            outputs,
            need_to_update_output_beat_trigger: [true; NUM_OUTPUTS],
        })
    }

    fn get_frequencies(&mut self) {
        let values = {
            let values_raw = self.converter.freqs();

            match self.params.gate {
                Some(gate_min) => values_raw
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
                None => values_raw,
            }
        };

        self.freqs = values;
    }

    //
    // DOES NOT run every ~20 ms. This runs as often as possible.
    //
    pub fn tick(&mut self, now: Instant) -> anyhow::Result<()> {
        self.get_frequencies();

        // Volume
        self.volume()?;
        // Bass
        self.bass(now)?;

        // Beat Volume
        self.beat_volume()?;

        if self.scratch.is_on_beat {
            println!("activate on beat flag");
            // self.scratch.is_on_beat = false;
            // NOTE: this will cause a missing update if the consumer takes too long.
            // self.need_to_update_output_beat_trigger = [true; NUM_OUTPUTS];
            self.need_to_update_output_beat_trigger.fill(true);
            self.scratch.is_on_beat = false;
        }

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

        let current_audio_colunn = match output_spec.bins_p_column {
            Some(num_bins) => bin_spectrum_to_u8(&self.freqs, num_bins),
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
            println!("included beat trigger")
        }

        output
    }
}

//
// Converter.
//

fn init_converter(
    device: Device,
    config: StreamConfig,
) -> anyhow::Result<(AudioConverter, Capture)> {
    // let config = StreamConfig {
    //     // TODO: also experiment with fft resolution
    //     // gravity: None, // OR: Some(100)
    //     gravity: Some(100.0),
    //     ..Default::default()
    // };

    println!("config: {config:?}");

    let audio_capture_config = CaptureConfig {
        sample_rate: Some(device.default_input_config().unwrap().sample_rate().0),
        latency: None,
        device: device.name().unwrap(),
        buffer_size: CaptureConfig::default().buffer_size,
        max_buffer_size: CaptureConfig::default().max_buffer_size,
    };

    let capture = Capture::init(audio_capture_config.clone()).map_err(|err| anyhow!("{err:?}"))?;
    let stream = Stream::init_with_capture(&capture, config.clone());
    let converter = AudioConverter::from_stream(stream, config.clone());

    Ok((converter, capture))
}

//
// Spectrogram utils.
//

pub type AudioColumn = Vec<u8>;

/// Needs to "summarize" the entire frequency spectrum into chunks
fn bin_spectrum_to_u8(values: &[audioviz::spectrum::Frequency], mut bins: usize) -> AudioColumn {
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
