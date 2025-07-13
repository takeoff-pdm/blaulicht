use anyhow::anyhow;
use audioviz::{
    audio_capture::{capture::Capture, config::Config as CaptureConfig},
    spectrum::{config::StreamConfig, stream::Stream},
};
use blaulicht_shared::CollectedAudioSnapshot;
use cpal::{traits::DeviceTrait, Device};

use crate::{audio::defs::AudioConverter, msg::Signal};

pub struct SignalCollector {
    pub(crate) current: CollectedAudioSnapshot,
}

impl SignalCollector {
    pub fn take_snapshot(&self) -> CollectedAudioSnapshot {
        self.current
    }

    pub fn signal(&mut self, signal: Signal) {
        match signal {
            Signal::Volume(v) => {
                self.current.volume = v;
            }
            Signal::BeatVolume(v) => {
                self.current.beat_volume = v;
            }
            Signal::Bass(v) => {
                self.current.bass = v;
            }
            Signal::BassAvgShort(v) => {
                self.current.bass_avg_short = v;
            }
            Signal::BassAvg(v) => {
                self.current.bass_avg = v;
            }
            Signal::Bpm(v) => {
                self.current.bpm = v.bpm;
                self.current.time_between_beats_millis = v.time_between_beats_millis;
            }
        }
    }

    pub fn new() -> Self {
        Self {
            current: CollectedAudioSnapshot::default(),
        }
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

    let audio_capture_config = CaptureConfig {
        sample_rate: Some(device.default_input_config().unwrap().sample_rate().0),
        latency: None,
        device: device.name().unwrap(),
        buffer_size: CaptureConfig::default().buffer_size,
        max_buffer_size: CaptureConfig::default().max_buffer_size,
    };

    let capture = Capture::init(audio_capture_config.clone()).map_err(|err| anyhow!("{err:?}"))?;
    let stream = Stream::init_with_capture(&capture, config.clone());
    let converter = AudioConverter::from_stream(stream, config.clone());

    Ok((converter, capture))
}
