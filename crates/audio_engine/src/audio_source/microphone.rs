use crate::{AudioConverter, AudioSource, Frequency};
use anyhow::{anyhow, Context};
use audioviz::{
    audio_capture::capture::Capture,
    audio_capture::config::Config as CaptureConfig,
    spectrum::{config::StreamConfig, stream::Stream},
};
//     audio_capture::{capture::Capture, config::Config as CaptureConfig},
use cpal::{traits::DeviceTrait, Device};

//
// MICROPHONE SOURCE.
//

pub struct AudioSourceMicrophone {
    pub converter: AudioConverter,
    _capture: Capture, // Cant be dropped or the converter dies.
}

impl AudioSourceMicrophone {
    pub fn new(device: Device, config: StreamConfig) -> anyhow::Result<Self> {
        let (converter, _capture) = init_converter(device, config)
            .with_context(|| "Failed to initialize audio converter")?;

        Ok(Self {
            converter,
            _capture,
        })
    }
}

impl AudioSource for AudioSourceMicrophone {
    fn get_frequencies(&mut self, _now: usize) -> Vec<Frequency> {
        self.converter
            .freqs()
            .iter()
            .map(|f| Frequency::from(f))
            .collect()
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
