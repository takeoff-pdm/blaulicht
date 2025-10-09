//----- Errors

use std::fmt::Display;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    Midi(MidiError)

}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Midi(midi_error) => write!(f, "MIDI error: {}", midi_error),
        }
    }
}

impl From<MidiError> for Error {
    fn from(err: MidiError) -> Self {
        Error::Midi(err)
    }
}

#[derive(Debug)]
pub enum MidiError {
    DeviceNotFound(String),
}

impl Display for MidiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MidiError::DeviceNotFound(dev) => write!(f, "Device '{dev}' not found"),
        }
    }
}