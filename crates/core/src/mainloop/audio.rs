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

use crate::{
    config::Config,
    msg::{AudioDeviceT, AudioInput, DummyInput},
};
use blaulicht_audio_engine::{noise::AudioSourceNoise, AudioSource, Frequency};
use std::time::{Duration, Instant};

#[cfg(feature = "audio")]
type CaptureSource = blaulicht_audio_engine::audio_source::microphone::AudioSourceMicrophone;
#[cfg(not(feature = "audio"))]
type CaptureSource = blaulicht_audio_engine::noise::AudioSourceNoise;

const DUMMY_NOISE_SAMPLE_RATE: u32 = 48_000;
/// Roughly the frame rate of a real capture device; the mainloop ticks faster.
const DUMMY_NOISE_FRAME_INTERVAL: Duration = Duration::from_millis(10);

enum Input {
    /// A hardware device that is reopened while it is unavailable.
    Device {
        device: AudioDeviceT,
        source: Option<Box<CaptureSource>>,
        retry_at: Instant,
    },
    Silence,
    Noise {
        source: AudioSourceNoise,
        next_frame_at: Instant,
    },
}

/// Keep the lighting engine alive while an input device is unavailable.
pub struct RecoveringAudioSource {
    input: Input,
    config: Config,
    silence: Vec<Frequency>,
}

impl RecoveringAudioSource {
    pub fn new(input: AudioInput, config: Config) -> Self {
        let input = match input {
            AudioInput::Device(device) => Input::Device {
                device,
                source: None,
                retry_at: Instant::now(),
            },
            AudioInput::Dummy(DummyInput::Silence) => Input::Silence,
            AudioInput::Dummy(DummyInput::Noise) => Input::Noise {
                source: AudioSourceNoise::new(
                    DUMMY_NOISE_SAMPLE_RATE,
                    usize::MAX,
                    AUDIO_SOURCE_FREQ_BUFFER_SIZE,
                ),
                next_frame_at: Instant::now(),
            },
        };
        Self {
            input,
            config,
            silence: vec![Frequency::default(); AUDIO_SOURCE_FREQ_BUFFER_SIZE],
        }
    }
}

impl AudioSource for RecoveringAudioSource {
    fn get_freq_buffer_size(&self) -> usize {
        AUDIO_SOURCE_FREQ_BUFFER_SIZE
    }

    fn get_frequencies(&mut self, now: usize) -> (&[Frequency], bool) {
        match &mut self.input {
            Input::Device {
                device,
                source,
                retry_at,
            } => {
                if source.is_none() && Instant::now() >= *retry_at {
                    *retry_at = Instant::now() + Duration::from_secs(2);
                    match open_stream(device.clone(), self.config.clone()) {
                        Ok(opened) => *source = Some(Box::new(opened)),
                        Err(err) => {
                            tracing::warn!(target: crate::log::target::AUDIO, "Input unavailable; lighting continues: {err:#}")
                        }
                    }
                }
                match source.as_mut() {
                    Some(source) => source.get_frequencies(now),
                    None => (&self.silence, false),
                }
            }
            Input::Silence => (&self.silence, false),
            Input::Noise {
                source,
                next_frame_at,
            } => {
                let now_instant = Instant::now();
                if now_instant >= *next_frame_at {
                    *next_frame_at = now_instant + DUMMY_NOISE_FRAME_INTERVAL;
                    source.get_frequencies(now)
                } else {
                    (source.frequencies(), false)
                }
            }
        }
    }

    fn sample_rate(&self) -> Option<u32> {
        match &self.input {
            Input::Device { source, .. } => source.as_deref().and_then(AudioSource::sample_rate),
            Input::Silence => None,
            Input::Noise { source, .. } => AudioSource::sample_rate(source),
        }
    }

    fn drain_samples(&mut self, output: &mut Vec<f32>) {
        if let Input::Device {
            source: Some(source),
            ..
        } = &mut self.input
        {
            source.drain_samples(output);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collect(input: DummyInput) -> blaulicht_shared::CollectedAudioSnapshot {
        let source = RecoveringAudioSource::new(AudioInput::Dummy(input), Config::default());
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
            std::thread::sleep(DUMMY_NOISE_FRAME_INTERVAL);
        }
        collector.current.clone()
    }

    #[test]
    fn dummy_silence_provides_no_frames_without_failing_collector() {
        let current = collect(DummyInput::Silence);
        assert_eq!(
            current.source_status,
            blaulicht_shared::AudioSourceStatus::Disconnected
        );
        assert_eq!(current.volume, 0);
    }

    #[test]
    fn dummy_noise_produces_live_frames() {
        let current = collect(DummyInput::Noise);
        assert_eq!(
            current.source_status,
            blaulicht_shared::AudioSourceStatus::Active
        );
        assert!(current.volume > 0, "noise volume: {}", current.volume);
        assert!(current.bass < u8::MAX, "noise saturates bass");
    }

    #[test]
    fn dummy_names_round_trip() {
        for dummy in DummyInput::ALL {
            assert_eq!(DummyInput::from_name(dummy.name()), Some(dummy));
        }
        assert_eq!(DummyInput::from_name("default"), None);
    }
}
