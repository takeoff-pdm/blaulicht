use crate::Frequency;

#[cfg(feature = "file")]
pub mod file;

#[cfg(feature = "stream_in")]
pub mod microphone;

#[cfg(feature = "noise")]
pub mod noise;

pub trait AudioSource {
    fn get_frequencies(&mut self, now: usize) -> Vec<Frequency>;
}
