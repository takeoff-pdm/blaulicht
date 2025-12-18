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
// pub const SIGNAL_SPEED: Duration = Duration::from_millis(50);
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
