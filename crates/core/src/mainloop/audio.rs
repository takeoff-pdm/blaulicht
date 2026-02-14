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
