use crate::{AudioSource, Frequency};
use audioviz::io::{Device, Input, InputController};
use audioviz::spectrum::{config::StreamConfig, stream::Stream};
use cpal::traits::{DeviceTrait, HostTrait};

//
// MICROPHONE SOURCE.
//

pub struct AudioSourceMicrophone {
    input: Input,
    controller: InputController,
    stream: Stream,
    pub freq_buffer: Vec<Frequency>,
}

/// Map a selected cpal device to the device identifier audioviz expects.
///
/// audioviz resolves `Device::Id(n)` as `cpal::default_host().input_devices().nth(n)`, so we
/// locate the requested device by name within that same enumeration. If it can't be matched
/// (e.g. it lives on a non-default host, or has no name) we fall back to the default input.
fn resolve_audioviz_device(device: &cpal::Device) -> Device {
    let Ok(target_name) = device.name() else {
        return Device::DefaultInput;
    };

    let host = cpal::default_host();
    let index = host.input_devices().ok().and_then(|mut devices| {
        devices.position(|candidate| {
            candidate
                .name()
                .map(|name| name == target_name)
                .unwrap_or(false)
        })
    });

    match index {
        Some(index) => Device::Id(index),
        None => Device::DefaultInput,
    }
}

impl AudioSourceMicrophone {
    pub fn new(
        device: cpal::Device,
        config: StreamConfig,
        freq_buffer_size: usize,
    ) -> anyhow::Result<Self> {
        let stream = Stream::new(config);

        let latency = 10;
        let frames_10ms_at_48k = stream.config.processor.sampling_rate * latency / 1000;
        let buffer_size = Some(frames_10ms_at_48k);

        let av_device = resolve_audioviz_device(&device);

        // set up capture on the same thread; only the CPAL callback runs elsewhere
        let mut input = Input::new();
        let (_channels, _sample_rate, controller) = input
            .init(&av_device, buffer_size)
            .map_err(|err| anyhow::anyhow!("failed to init audio input: {:?}", err))?;

        Ok(Self {
            freq_buffer: vec![Frequency::default(); freq_buffer_size],
            input,
            controller,
            stream,
        })
    }
}

impl AudioSource for AudioSourceMicrophone {
    fn get_frequencies(&mut self, _now: usize) -> &[Frequency] {
        // Drain every block the CPAL callback has queued since the last call, running the
        // FFT/post-processing once per block. Draining avoids a growing backlog (and latency)
        // now that this is polled on a fixed cadence rather than every loop iteration.
        while let Some(block) = self.controller.try_pull_data() {
            if !block.is_empty() {
                self.stream.push_data(block);
                self.stream.update(); // FFT + post-processing on the main thread
            }
        }

        let frequencies = self.stream.get_frequencies();

        // Note: use last available freqs (like interpolation but worse)
        if frequencies.is_empty() {
            return &self.freq_buffer;
        }

        let mut frequencies = frequencies[0].clone();

        if frequencies.is_empty() {
            return &self.freq_buffer;
        }

        while frequencies.len() > self.freq_buffer.len() {
            frequencies.pop();
        }

        self.freq_buffer.clear();
        self.freq_buffer
            .extend(frequencies.iter().map(|f| Frequency {
                volume: f.volume * 10.0,
                freq: f.freq,
                position: f.position,
            }));

        &self.freq_buffer
    }

    fn get_freq_buffer_size(&self) -> usize {
        self.freq_buffer.len()
    }
}
