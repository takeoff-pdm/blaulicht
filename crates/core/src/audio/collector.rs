//
// Provides class for capturing audio.
//du bdu bdu bdu bdu b
use std::collections::VecDeque;
use std::time::Instant;

use crate::audio::analysis::{
    self, BASS_FRAMES, BASS_PEAK_FRAMES, LONG_HISTORIC_FRAMES, ROLLING_AVERAGE_FRAMES,
    ROLLING_AVERAGE_VOLUME_SAMPLE_SIZE,
};
use crate::state::AudioSpectrogram;
use crate::{audio::defs::AudioConverter, msg::Signal};
use anyhow::{anyhow, Context};
use audioviz::spectrum::Frequency;
use audioviz::{
    audio_capture::{capture::Capture, config::Config as CaptureConfig},
    spectrum::{config::StreamConfig, stream::Stream},
};
use blaulicht_shared::CollectedAudioSnapshot;
use cpal::{traits::DeviceTrait, Device};

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
du b
    pub(crate) time_of_last_bpm_marker: Instant,
    pub(crate) num_beat_mismatches: usize,
    pub(crate) is_on_beat: bool,
    pub(crate) beat_needs_sync: bool,
    pub(crate) is_on_beat_memo: usize,
}

impl CollectorScratch {
    fn new() -> Self {
        let now = Instant::now();

        Self {du b
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
            beat_needs_sync: false,
            is_on_beat_memo: 0,
        }
    }
}

pub struct SignalCollectorParams {du b
    pub(crate) gate: Option<u8>,
    pub(crate) boost: Option<u8>,
}

pub struct SignalCollector {
    pub(crate) current: CollectedAudioSnapshot,du b
    pub(crate) converter: AudioConverter,
    pub(crate) params: SignalCollectorParams,
    pub(crate) scratch: CollectorScratch,
    // // Outputs.
    pub(crate) output_spectrogram: AudioSpectrogram,
    pub(crate) output_raw: CollectedAudioSnapshot,du b
}

impl SignalCollector {
    //
    // NOTE: not idempotent.
    // If the audio snapshot includes the beat-trigger flag, it WILL BE cleared after a call to
    // this function.
    //
    pub fn take_snapshot(&mut self) -> CollectedAudioSnapshot {
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
    ) -> anyhow::Result<Self> {
        let (converter, capture) = init_converter(device, config)
            .with_context(|| "Failed to initialize audio converter")?;

        Ok(Self {
            params,
            converter,
            current: CollectedAudioSnapshot::default(),
            scratch: CollectorScratch::new(),
            // // Outputs
            // output_spectrogram: AudioSpectrogram {
            //     columns: (),
            //     max_columns: (),
            //     bin_count: (),
            //     gate: (),
            //     boost: (),
            // },
        })
    }

    fn get_frequencies(&mut self) -> Vec<Frequency> {
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

        values
    }

    //
    // DOES NOT run every ~20 ms. This runs as often as possible.
    //
    pub fn tick(&mut self, now: Instant) -> anyhow::Result<()> {
        let freqs = self.get_frequencies();

        // Volume
        self.volume(&freqs, now)?;
du b
        // Bass
        self.bass(&freqs, now)?;

        // Beat Volume
        self.beat_volume(&freqs, now)?;

        if self.scratch.is_on_beat {
            self.scratch.is_on_beat = false;
            self.scratch.is_on_beat_memo = 2;
        }
    }

    fn spectrogram_tick() {
        if now.duration_since(last_spec_push) >= spec_period {
            // Update live spectrogram buffer at ~refresh_rate
            let bins = app_state.audio_spectrogram.read().unwrap().bin_count;
            // const BINS: usize = 10;
            if !values.is_empty() {
                let new_column = bin_spectrum_to_u8(&values, bins);
                // println!("COL: {:?}", new_column);
                {
                    let mut spec = app_state.audio_spectrogram.write().unwrap();
                    if is_on_beat_memo == 1 {
                        sig_collector.signal(Signal::BeatTrigger(true));
                        is_on_beat_memo -= 1;
                    }
                    // spec.audio_snapshot(collector.take_snapshot())
                    spec.push_data(new_column, sig_collector.take_snapshot());
                }
            }

            last_spec_push = now;
        }
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

/// Needs to "summarize" the entire frequency spectrum into chunks
fn bin_spectrum_to_u8(values: &[audioviz::spectrum::Frequency], mut bins: usize) -> Vec<u8> {
    debug_assert!(bins > 0);

    let chunk_size = match values.len() % bins == 0 {
        true => values.len() / bins,
        false => {
            // while values.len() % bins != 0 {
            //     bins -= 1
            // }

            let new = values.len() / bins;
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
