const AUDIO_SOURCE_FREQ_BUFFER_SIZE: usize = 2048;

#[cfg(feature = "audio")]
mod audio_impl {
    use crate::{
        config::Config, mainloop::audio::AUDIO_SOURCE_FREQ_BUFFER_SIZE, msg::AudioDeviceT,
    };
    use blaulicht_audio_engine::audio_source::microphone::AudioSourceMicrophone;

    pub fn open_stream(
        device: AudioDeviceT,
        config: Config,
    ) -> anyhow::Result<AudioSourceMicrophone> {
        use anyhow::Context;

        let audio_source = AudioSourceMicrophone::new(
            device,
            config.stream.clone(),
            AUDIO_SOURCE_FREQ_BUFFER_SIZE,
        )
        .with_context(|| "Failed to initialize audio stream")?;

        Ok(audio_source)
    }
}

#[cfg(not(feature = "audio"))]
mod audio_impl {
    use crate::{
        config::Config, mainloop::audio::AUDIO_SOURCE_FREQ_BUFFER_SIZE, msg::AudioDeviceT,
    };
    use blaulicht_audio_engine::noise::AudioSourceNoise;

    pub fn open_stream(_: AudioDeviceT, _: Config) -> anyhow::Result<AudioSourceNoise> {
        let audio_source = AudioSourceNoise::new(41100, 100000, AUDIO_SOURCE_FREQ_BUFFER_SIZE);
        Ok(audio_source)
    }
}

pub use audio_impl::*;

use crate::{config::Config, msg::AudioDeviceT};
use blaulicht_audio_engine::{AudioSource, Frequency};
use std::time::{Duration, Instant};

#[cfg(feature = "audio")]
type CaptureSource = blaulicht_audio_engine::audio_source::microphone::AudioSourceMicrophone;
#[cfg(not(feature = "audio"))]
type CaptureSource = blaulicht_audio_engine::noise::AudioSourceNoise;

/// Keep the lighting engine alive while an input device is unavailable.
pub struct RecoveringAudioSource {
    source: Option<CaptureSource>,
    device: Option<AudioDeviceT>,
    config: Config,
    retry_at: Instant,
    silence: Vec<Frequency>,
}

impl RecoveringAudioSource {
    pub fn new(device: Option<AudioDeviceT>, config: Config) -> Self {
        Self {
            source: None,
            device,
            config,
            retry_at: Instant::now(),
            silence: vec![Frequency::default(); AUDIO_SOURCE_FREQ_BUFFER_SIZE],
        }
    }
}

impl AudioSource for RecoveringAudioSource {
    fn get_freq_buffer_size(&self) -> usize {
        AUDIO_SOURCE_FREQ_BUFFER_SIZE
    }

    fn get_frequencies(&mut self, now: usize) -> (&[Frequency], bool) {
        if self.source.is_none() && Instant::now() >= self.retry_at {
            self.retry_at = Instant::now() + Duration::from_secs(2);
            if let Some(device) = self.device.clone() {
                match open_stream(device, self.config.clone()) {
                    Ok(source) => self.source = Some(source),
                    Err(err) => {
                        tracing::warn!("[audio] Input unavailable; lighting continues: {err:#}")
                    }
                }
            }
        }
        match self.source.as_mut() {
            Some(source) => source.get_frequencies(now),
            None => (&self.silence, false),
        }
    }

    fn sample_rate(&self) -> Option<u32> {
        self.source.as_ref().and_then(AudioSource::sample_rate)
    }

    fn drain_samples(&mut self, output: &mut Vec<f32>) {
        if let Some(source) = self.source.as_mut() {
            source.drain_samples(output);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_device_provides_no_frames_without_failing_collector() {
        let source = RecoveringAudioSource::new(None, Config::default());
        let mut collector = blaulicht_audio_engine::SignalCollector::new(
            Default::default(),
            [blaulicht_audio_engine::CollectorOutputSpec::default()],
            Default::default(),
            source,
            0,
        )
        .unwrap();
        for now in [0, 100, 1000, 3000] {
            collector.tick(now).unwrap();
        }
        assert_eq!(
            collector.current.source_status,
            blaulicht_shared::AudioSourceStatus::Disconnected
        );
        assert_eq!(collector.current.volume, 0);
    }
}
