use std::{any::Any, time::Instant};

use crate::{AudioConverter, AudioSource, Frequency};
use anyhow::{anyhow, Context};
use audioviz::{
    audio_capture::{capture::Capture, config::Config as CaptureConfig},
    io::Input,
    spectrum::{
        config::{ProcessorConfig, StreamConfig},
        stream::Stream,
    },
};
//     audio_capture::{capture::Capture, config::Config as CaptureConfig},
use cpal::{traits::DeviceTrait, Device};

//
// MICROPHONE SOURCE.
//

pub struct AudioSourceMicrophone {
    pub converter: AudioConverter,
    _capture: Capture, // Cant be dropped or the converter dies.
    pub freq_buffer: Vec<Frequency>,
}

impl AudioSourceMicrophone {
    pub fn new(
        device: Device,
        config: StreamConfig,
        freq_buffer_size: usize,
    ) -> anyhow::Result<Self> {
        // let (converter, _capture) = init_converter(device, config)
        //     .with_context(|| "Failed to initialize audio converter")?;

        // configure FFT + gravity exactly once on the main thread
        let mut stream_config = StreamConfig::default();
        stream_config.channel_count = 2;
        stream_config.fft_resolution = 1024;
        stream_config.processor = ProcessorConfig::default();

        let mut stream = Stream::new(stream_config);

        // set up capture on the same thread; only the CPAL callback runs elsewhere
        let mut input = Input::with_latency(10);

        // TODO: downgrade cpal
        let (_channels, _sample_rate, controller) = input
            .init(&audioviz::io::Device::Cpal(device), None)
            .map_err(|err| anyhow::anyhow!("failed to init audio input: {:?}", err))?;

        // let start = Instant::now();

        // loop {
        // blocks until CPAL callback pushes a block into the channel
        if let Some(block) = controller.pull_data() {
            stream.push_data(block);
            stream.update(); // FFT + post-processing on the main thread

            for (channel_idx, freqs) in stream.get_frequencies().into_iter().enumerate() {
                println!("channel {} peaks:", channel_idx);
                for f in freqs.iter().take(2) {
                    println!("  {:>7.1} Hz -> {:>5.3}", f.freq, f.volume);
                }
            }
        }

        // if start.elapsed() > Duration::from_secs(10) {
        //     break;
        // }
        // }

        // Ok(Self {
        //     converter,
        //     _capture,
        //     freq_buffer: vec![Frequency::default(); freq_buffer_size],
        // })
        Ok(())
    }
}

impl AudioSource for AudioSourceMicrophone {
    fn get_frequencies(&mut self, _now: usize) -> &[Frequency] {
        for (i, v) in self.converter.freqs().iter().enumerate() {
            self.freq_buffer[i] = Frequency::from(v);
        }

        &self.freq_buffer
    }

    fn get_freq_buffer_size(&self) -> usize {
        self.freq_buffer.len()
    }
}

//
// Converter.
//

pub fn init_converter(
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

    let default_input_config = device
        .default_input_config()
        .context("Failed to query default input config for input device")?;
    let device_name = device
        .name()
        .context("Failed to query audio input device name")?;

    let audio_capture_config = CaptureConfig {
        sample_rate: Some(default_input_config.sample_rate().0),
        latency: None,
        device: device_name.clone(),
        buffer_size: CaptureConfig::default().buffer_size,
        max_buffer_size: CaptureConfig::default().max_buffer_size,
    };

    let capture = Capture::init(audio_capture_config.clone())
        .map_err(|err| anyhow!("Failed to initialize audio capture for {device_name}: {err:?}"))?;
    let stream = Stream::init_with_capture(&capture, config.clone());
    let converter = AudioConverter::from_stream(stream, config.clone());

    Ok((converter, capture))
}
