use std::{
    time::Duration,
    u8,
};


use audioviz::{audio_capture::capture::Capture, spectrum::config::StreamConfig};
use audioviz::{
    audio_capture::capture::CaptureReceiver,
    spectrum::{
        stream::{Stream, StreamController},
        Frequency,
    },
};
use itertools::Itertools;


const ROLLING_AVERAGE_LOOP_ITERATIONS: usize = 100;
const ROLLING_AVERAGE_VOLUME_SAMPLE_SIZE: usize = ROLLING_AVERAGE_LOOP_ITERATIONS / 2;

// Important.
pub const SYSTEM_MESSAGE_SPEED: Duration = Duration::from_millis(1000);
pub const SIGNAL_SPEED: Duration = Duration::from_millis(50);
pub const DMX_TICK_TIME: Duration = Duration::from_millis(25);


#[non_exhaustive]
pub struct AudioThreadControlSignal;

impl AudioThreadControlSignal {
    pub const CONTINUE: u8 = 0;
    pub const ABORT: u8 = 1;
    pub const ABORTED: u8 = 2;
    pub const CRASHED: u8 = 3;
    pub const RELOAD: u8 = 4;
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
    pub config: AudioConfig,
    pub resolution: usize,
}
#[derive(Debug, Clone)]
pub struct AudioConfig(pub StreamConfig);

impl Default for AudioConfig {
    fn default() -> Self {
        Self( 
            audioviz::spectrum::config::StreamConfig {
                gravity: Some(100.0),
                ..Default::default()
            }
        )
    }
}

impl From<AudioConfig> for StreamConfig {
    fn from(value: AudioConfig) -> Self {
        value.0
    }
}

impl AudioConverter {
    pub fn from_capture(capture: Capture, config: AudioConfig) -> Self {
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

    pub fn from_stream(stream: Stream, config: AudioConfig) -> Self {
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
                            let value = value * 30.0 * self.config.0.processor.volume;
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
