use std::{time::Duration, u8};

use audioviz::{audio_capture::capture::Capture, spectrum::config::StreamConfig};
use audioviz::{
    audio_capture::capture::CaptureReceiver,
    spectrum::{
        stream::{Stream, StreamController},
        Frequency,
    },
};

const ROLLING_AVERAGE_LOOP_ITERATIONS: usize = 100;
const ROLLING_AVERAGE_VOLUME_SAMPLE_SIZE: usize = ROLLING_AVERAGE_LOOP_ITERATIONS / 2;

// Important.
pub const SYSTEM_MESSAGE_SPEED: Duration = Duration::from_millis(100);
pub const SIGNAL_SPEED: Duration = Duration::from_millis(50);
pub const DMX_TICK_TIME: Duration = Duration::from_millis(25);

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum AudioThreadControlSignal {
    CONTINUE,
    ABORT,
    ABORTED,
    CRASHED,
    RELOAD,
}

impl From<AudioThreadControlSignal> for u8 {
    fn from(value: AudioThreadControlSignal) -> Self {
        match value {
            AudioThreadControlSignal::CONTINUE => 0,
            AudioThreadControlSignal::ABORT => 1,
            AudioThreadControlSignal::ABORTED => 2,
            AudioThreadControlSignal::CRASHED => 3,
            AudioThreadControlSignal::RELOAD => 4,
        }
    }
}

impl From<u8> for AudioThreadControlSignal {
    fn from(value: u8) -> Self {
        match value {
            0 => AudioThreadControlSignal::CONTINUE,
            1 => AudioThreadControlSignal::ABORT,
            2 => AudioThreadControlSignal::ABORTED,
            3 => AudioThreadControlSignal::CRASHED,
            4 => AudioThreadControlSignal::RELOAD,
            _ => unreachable!("Not possible when using the conversion functions!"),
        }
    }
}

pub enum ConverterType {
    Stream(Stream),
    Capture(Capture),
}

pub struct AudioConverter {
    raw_buf: Vec<f32>,
    show_vec: Vec<f32>,
    pub raw_receiver: Option<CaptureReceiver>,
    pub stream_controller: Option<StreamController>,
    pub config: StreamConfig,
    pub resolution: usize,
}

// impl Default for AudioConfig {
//     fn default() -> Self {
//     }
// }

// impl From<AudioConfig> for StreamConfig {
//     fn from(value: AudioConfig) -> Self {
//         value.0
//     }
// }

impl AudioConverter {
    pub fn from_capture(capture: Capture, config: StreamConfig) -> Self {
        let raw_receiver = capture.get_receiver().unwrap();
        Self {
            raw_buf: Vec::new(),
            show_vec: Vec::new(),
            raw_receiver: Some(raw_receiver),
            stream_controller: None,
            config,
            resolution: 0,
        }
    }

    pub fn from_stream(stream: Stream, config: StreamConfig) -> Self {
        let stream_controller = stream.get_controller();
        Self {
            raw_buf: Vec::new(),
            show_vec: Vec::new(),
            raw_receiver: None,
            stream_controller: Some(stream_controller),
            config,
            resolution: 0,
        }
    }

    pub fn get_data(&mut self) -> Option<Vec<f32>> {
        if let Some(raw) = &self.raw_receiver {
            let mut data: Vec<f32> = match raw.receive_data() {
                Ok(d) => {
                    let mut b: Vec<f32> = Vec::new();

                    let bufs = d.chunks(1);
                    for buf in bufs {
                        let mut max: f32 = 0.0;
                        for value in buf {
                            println!("vollll: {}", self.config.processor.volume);
                            let value = value * 30.0 * self.config.processor.volume;
                            if value > max {
                                max = value
                            }
                        }
                        b.push(max)
                    }
                    b
                }
                Err(_) => Vec::new(),
            };
            self.raw_buf.append(&mut data);
            if self.raw_buf.len() >= self.resolution {
                self.show_vec = self.raw_buf[0..self.resolution].to_vec();
                self.raw_buf.drain(..);
            }
            return Some(self.show_vec.clone());
        }

        if let Some(stream) = &self.stream_controller {
            let freqs = stream.get_frequencies();

            let data: Vec<f32> = freqs.into_iter().map(|x| x.volume).collect();

            return Some(data);
        }
        None
    }

    pub fn freqs(&mut self) -> Vec<Frequency> {
        if let Some(stream) = &self.stream_controller {
            let freqs = stream.get_frequencies();
            return freqs;
        }

        panic!("broken");
    }
}
