use crate::Frequency;

#[cfg(feature = "file")]
pub mod file;

#[cfg(feature = "stream_in")]
pub mod microphone;

#[cfg(feature = "noise")]
pub mod noise;

pub trait AudioSource {
    fn get_freq_buffer_size(&self) -> usize;
    /// Returns the current frequency buffer along with a flag indicating whether
    /// the buffer was refreshed since the previous call.
    ///
    /// The bool lets callers gate expensive per-frame analysis to actual audio
    /// updates instead of running it at the mainloop tick rate.
    fn get_frequencies(&mut self, now: usize) -> (&[Frequency], bool);

    /// Sample rate of raw mono PCM made available through [`Self::drain_samples`].
    fn sample_rate(&self) -> Option<u32> {
        None
    }

    /// Move newly captured mono PCM into `output` for secondary analyzers.
    fn drain_samples(&mut self, _output: &mut Vec<f32>) {}
}
