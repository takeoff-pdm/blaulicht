pub mod file;

use std::time::Instant;

use crate::AudioConverter;
use anyhow::Context;
use audioviz::{
    audio_capture::capture::Capture,
    spectrum::{config::StreamConfig, Frequency},
};
use cpal::{traits::DeviceTrait, Device};

pub trait AudioSource {
    fn get_frequencies(&mut self, now: usize) -> Vec<Frequency>;
}

//
// MICROPHONE SOURCE.
//

pub struct AudioSourceMicrophone {
    pub converter: AudioConverter,
    _capture: Capture, // Cant be dropped or the converter dies.
}

impl AudioSourceMicrophone {
    pub fn new(device: Device, config: StreamConfig) -> anyhow::Result<Self> {
        let (converter, _capture) = crate::init_converter(device, config)
            .with_context(|| "Failed to initialize audio converter")?;

        Ok(Self {
            converter,
            _capture,
        })
    }
}

impl AudioSource for AudioSourceMicrophone {
    fn get_frequencies(&mut self, _now: usize) -> Vec<Frequency> {
        self.converter.freqs()
    }
}
