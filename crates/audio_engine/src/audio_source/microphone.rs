use crate::{AudioSource, Frequency};
use audioviz::io::{Device, Input, InputController};
use audioviz::spectrum::config::StreamConfig;

use super::stream_frames::StreamFrames;
use cpal::traits::{DeviceTrait, HostTrait};

//
// MICROPHONE SOURCE.
//

pub struct AudioSourceMicrophone {
    input: Input,
    controller: InputController,
    sample_rate: u32,
    frames: StreamFrames,
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
        let latency = 10;
        let frames_10ms_at_48k = config.processor.sampling_rate * latency / 1000;
        let buffer_size = Some(frames_10ms_at_48k);

        let av_device = resolve_audioviz_device(&device);

        // set up capture on the same thread; only the CPAL callback runs elsewhere
        let mut input = Input::new();
        let (channels, sample_rate, controller) = match input.init(&av_device, buffer_size) {
            Ok(handles) => handles,
            Err(err) => {
                // Some backends (e.g. the PipeWire ALSA plugin) reject the
                // explicit low-latency buffer size with EINVAL. Retry with the
                // device default rather than failing outright.
                log::warn!(
                    "audio input rejected buffer size {buffer_size:?} ({err:?}); retrying with device default"
                );
                input = Input::new();
                input
                    .init(&av_device, None)
                    .map_err(|err| anyhow::anyhow!("failed to init audio input: {:?}", err))?
            }
        };

        Ok(Self {
            input,
            controller,
            sample_rate,
            frames: StreamFrames::new(config, channels as usize, sample_rate, freq_buffer_size),
        })
    }
}

impl AudioSource for AudioSourceMicrophone {
    fn get_frequencies(&mut self, _now: usize) -> (&[Frequency], bool) {
        // `try_pull_data` already drains the whole capture channel into a single block (and
        // returns `Some(empty)` rather than `None` when idle), so one pull per call is enough.
        // Looping on it would spin forever on the empty-but-connected case.
        let has_new = self
            .controller
            .try_pull_data()
            .is_some_and(|block| self.frames.push_block(block));
        (self.frames.frequencies(), has_new)
    }

    fn get_freq_buffer_size(&self) -> usize {
        self.frames.frequencies().len()
    }

    fn sample_rate(&self) -> Option<u32> {
        Some(self.sample_rate)
    }

    fn drain_samples(&mut self, output: &mut Vec<f32>) {
        self.frames.drain_samples(output);
    }
}
