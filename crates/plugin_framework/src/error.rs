//----- Errors

use std::fmt::Display;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    Connection(IoError),
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Connection(midi_error) => write!(f, "IO error: {}", midi_error),
        }
    }
}

impl From<IoError> for Error {
    fn from(err: IoError) -> Self {
        Error::Connection(err)
    }
}

#[derive(Debug)]
pub enum IoError {
    MidiDeviceNotFound(String),
    SerialDeviceNotFound(String),
}

impl Display for IoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IoError::MidiDeviceNotFound(dev) => write!(f, "MIDI device '{dev}' not found"),
            IoError::SerialDeviceNotFound(dev) => write!(f, "SERIAL device '{dev}' not found"),
        }
    }
}
