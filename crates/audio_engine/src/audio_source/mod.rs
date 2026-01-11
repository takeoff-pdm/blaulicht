use crate::Frequency;

#[cfg(feature = "sources")]
pub mod file;

#[cfg(feature = "sources")]
pub mod microphone;

pub mod noise;

pub trait AudioSource {
    fn get_frequencies(&mut self, now: usize) -> Vec<Frequency>;
}
