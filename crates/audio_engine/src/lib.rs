pub const VAR: usize = 42;

pub mod audio_source;

pub mod collector;

#[cfg(feature = "stream_in")]
pub mod converter;

#[cfg(feature = "stream_in")]
pub use converter::*;

pub mod signals;
pub mod spectrogram;
pub mod types;

pub use audio_source::*;
pub use collector::*;

pub use signals::*;
pub use spectrogram::*;
pub use types::*;
