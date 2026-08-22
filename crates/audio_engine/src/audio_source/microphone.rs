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
    channel_count: usize,
    sample_rate: u32,
    captured_mono: Vec<f32>,
    pub freq_buffer: Vec<Frequency>,
}

/// Map a selected cpal device to the device identifier audioviz expects.
///
/// audioviz (using `cpal::default_host()`) resolves `Device::DefaultInput` via
/// `default_input_device()` and `Device::Id(n)` via `input_devices().nth(n)`.
///
/// We prefer `DefaultInput` whenever the selected device is the host's default input: that path
/// needs no device enumeration, so it avoids probing (and briefly opening) every other input
/// device right before audioviz opens the capture stream. Enumerating eagerly here was observed
/// to leave the capture silent on PipeWire/ALSA setups. We only fall back to locating the device
/// by index when a genuinely non-default device was chosen.
fn resolve_audioviz_device(device: &cpal::Device) -> Device {
    let Ok(target_name) = device.name() else {
        return Device::DefaultInput;
    };

    let host = cpal::default_host();

    // Common case: the selected device is the default input. Use it without enumerating.
    let default_is_target = host
        .default_input_device()
        .and_then(|d| d.name().ok())
        .map(|name| name == target_name)
        .unwrap_or(false);
    if default_is_target {
        return Device::DefaultInput;
    }

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
        let (channels, sample_rate, controller) = input
            .init(&av_device, buffer_size)
            .map_err(|err| anyhow::anyhow!("failed to init audio input: {:?}", err))?;

        Ok(Self {
            freq_buffer: vec![Frequency::default(); freq_buffer_size],
            input,
            controller,
            stream,
            // `.max(1)`: a zero channel count would panic in `chunks()`.
            channel_count: (channels as usize).max(1),
            sample_rate,
            captured_mono: Vec::new(),
        })
    }
}

impl AudioSource for AudioSourceMicrophone {
    fn get_frequencies(&mut self, _now: usize) -> (&[Frequency], bool) {
        // `try_pull_data` already drains the whole capture channel into a single block (and
        // returns `Some(empty)` rather than `None` when idle), so one pull per call is enough.
        // Looping on it would spin forever on the empty-but-connected case.
        let mut have_new_block = false;
        if let Some(block) = self.controller.try_pull_data() {
            if !block.is_empty() {
                self.captured_mono.extend(
                    block
                        .chunks(self.channel_count)
                        .map(|frame| frame.iter().copied().sum::<f32>() / frame.len() as f32),
                );
                self.stream.push_data(block);
                self.stream.update();
                have_new_block = true;
            }
        }

        if !have_new_block {
            return (&self.freq_buffer, false);
        }

        let frequencies = self.stream.get_frequencies();
        if frequencies.is_empty() {
            return (&self.freq_buffer, false);
        }

        let frequencies = frequencies[0].clone();
        if frequencies.is_empty() {
            return (&self.freq_buffer, false);
        }

        // Write in place so the buffer keeps its configured size: shrinking it
        // to a short/partial frame would permanently truncate the spectrum for
        // every later frame.
        let n = frequencies.len().min(self.freq_buffer.len());
        for (slot, f) in self.freq_buffer.iter_mut().zip(frequencies.iter().take(n)) {
            *slot = Frequency {
                volume: f.volume * 10.0,
                freq: f.freq,
                position: f.position,
            };
        }
        for slot in self.freq_buffer.iter_mut().skip(n) {
            *slot = Frequency::default();
        }

        (&self.freq_buffer, true)
    }

    fn get_freq_buffer_size(&self) -> usize {
        self.freq_buffer.len()
    }

    fn sample_rate(&self) -> Option<u32> {
        Some(self.sample_rate)
    }

    fn drain_samples(&mut self, output: &mut Vec<f32>) {
        output.append(&mut self.captured_mono);
    }
}
