use crate::{AudioSource, Frequency};
use anyhow::{anyhow, Context};
// use audioviz::{
//     audio_capture::capture::Capture,
//     audio_capture::config::Config as CaptureConfig,
//     spectrum::{config::StreamConfig, stream::Stream},
// };
//     audio_capture::{capture::Capture, config::Config as CaptureConfig},
use audioviz::io::{Device, Input, InputController};
use audioviz::spectrum::{
    config::{ProcessorConfig, StreamConfig},
    stream::Stream,
};
use audioviz::utils::combine_channels;
use cpal::traits::DeviceTrait;
use std::time::{Duration, Instant};

//
// MICROPHONE SOURCE.
//

pub struct AudioSourceMicrophone {
    // pub converter: AudioConverter,
    // _capture: Capture, // Cant be dropped or the converter dies.
    input: Input,
    controller: InputController,
    stream: Stream,

    pub freq_buffer: Vec<Frequency>,
}

impl AudioSourceMicrophone {
    pub fn new(
        device: cpal::Device,
        config: StreamConfig,
        freq_buffer_size: usize,
    ) -> anyhow::Result<Self> {
        // configure FFT + gravity exactly once on the main thread
        // let mut stream_config = StreamConfig::default();
        // stream_config.channel_count = 2;
        // stream_config.fft_resolution = 1024;
        // stream_config.processor = ProcessorConfig::default();

        let stream = Stream::new(config);

        let latency = 10;
        let frames_10ms_at_48k = stream.config.processor.sampling_rate * 10 / 1000;
        let buffer_size = Some(frames_10ms_at_48k);

        // set up capture on the same thread; only the CPAL callback runs elsewhere
        let mut input = Input::with_latency(latency);
        let (_channels, _sample_rate, controller) = input
            .init(&device, buffer_size)
            .map_err(|err| anyhow::anyhow!("failed to init audio input: {:?}", err))?;

        // let (converter, _capture) = init_converter(device, config)
        //     .with_context(|| "Failed to initialize audio converter")?;

        Ok(Self {
            // converter,
            // _capture,
            freq_buffer: vec![Frequency::default(); freq_buffer_size],
            input,
            controller,
            stream,
        })
    }
}

impl AudioSource for AudioSourceMicrophone {
    fn get_frequencies(&mut self, _now: usize) -> &[Frequency] {
        let start = Instant::now();
        let mut pulled_at: Option<Instant> = None;
        let mut updated_at: Option<Instant> = None;
        let mut got_freqs_at: Option<Instant> = None;

        // loop {
        // blocks until CPAL callback pushes a block into the channel
        if let Some(block) = self.controller.try_pull_data_alternative() {
            pulled_at = Some(Instant::now());
            self.stream.push_data(block);
            self.stream.update(); // FFT + post-processing on the main thread

            updated_at = Some(Instant::now());
        } else {
            // println!("audio: reuse");
            //
            // self.stream.update(); // FFT + post-processing on the main thread
            // return &self.freq_buffer;
        }

        // if start.elapsed() > Duration::from_millis(10) {
        //     println!("CRITICAL: Xrun in audio input");
        //     break;
        // }
        // }

        let frequencies = self.stream.get_frequencies();
        got_freqs_at = Some(Instant::now());

        if frequencies.is_empty() {
            if let (Some(pulled), Some(updated), Some(got)) = (pulled_at, updated_at, got_freqs_at)
            {
                println!(
                    "get_freqs timings (ms): pull_wait={}, update={}, get_freqs={}",
                    pulled.duration_since(start).as_millis(),
                    updated.duration_since(pulled).as_millis(),
                    got.duration_since(updated).as_millis()
                );
            }
            return &self.freq_buffer;
        }

        let mut frequencies = frequencies[0].clone();

        if frequencies.is_empty() {
            return &self.freq_buffer;
        }

        // if frequencies.len() < self.freq_buffer.len() {
        //     println!(
        //         "Len mismatch: expected: {}, but got: {}",
        //         self.freq_buffer.len(),
        //         frequencies.len()
        //     );
        //     while frequencies.len() < self.freq_buffer.len() {
        //         frequencies.push(audioviz::spectrum::Frequency {
        //             volume: 0.0,
        //             freq: 0.0,
        //             position: 0.0,
        //         });
        //     }
        // }

        while frequencies.len() > self.freq_buffer.len() {
            println!(
                "Len mismatch: expected: {}, but got: {}",
                self.freq_buffer.len(),
                frequencies.len()
            );
            frequencies.pop();
        }

        // if frequencies.len() != self.freq_buffer.len() {
        //     panic!(
        //         "expected: {} got {}",
        //         self.freq_buffer.len(),
        //         frequencies.len()
        //     );
        // }

        self.freq_buffer.clear();
        self.freq_buffer
            .extend(frequencies.iter().map(|f| Frequency {
                volume: f.volume * 10.0,
                freq: f.freq,
                position: f.position,
            }));

        if let (Some(pulled), Some(updated), Some(got)) = (pulled_at, updated_at, got_freqs_at) {
            println!(
                "get_freqs timings (ms): pull_wait={}, update={}, get_freqs={}, copy={}",
                pulled.duration_since(start).as_millis(),
                updated.duration_since(pulled).as_millis(),
                got.duration_since(updated).as_millis(),
                Instant::now().duration_since(got).as_millis()
            );
        }

        println!("BUF_LEN: {}", self.freq_buffer.len());

        &self.freq_buffer
    }

    fn get_freq_buffer_size(&self) -> usize {
        self.freq_buffer.len()
    }
}

//
// Converter.
//

// pub fn init_converter(
//     device: Device,
//     config: StreamConfig,
// ) -> anyhow::Result<(AudioConverter, Capture)> {
//     // let config = StreamConfig {
//     //     // TODO: also experiment with fft resolution
//     //     // gravity: None, // OR: Some(100)
//     //     gravity: Some(100.0),
//     //     ..Default::default()
//     // };
//
//     println!("config: {config:?}");
//
//     let default_input_config = device
//         .default_input_config()
//         .context("Failed to query default input config for input device")?;
//     let device_name = device
//         .name()
//         .context("Failed to query audio input device name")?;
//
//     let audio_capture_config = CaptureConfig {
//         sample_rate: Some(default_input_config.sample_rate().0),
//         latency: None,
//         device: device_name.clone(),
//         buffer_size: CaptureConfig::default().buffer_size,
//         max_buffer_size: CaptureConfig::default().max_buffer_size,
//     };
//
//     let capture = Capture::init(audio_capture_config.clone())
//         .map_err(|err| anyhow!("Failed to initialize audio capture for {device_name}: {err:?}"))?;
//     let stream = Stream::init_with_capture(&capture, config.clone());
//     let converter = AudioConverter::from_stream(stream, config.clone());
//
//     Ok((converter, capture))
// }
