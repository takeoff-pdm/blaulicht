pub const VAR: usize = 42;

pub mod audio_source;

pub mod collector;

#[cfg(feature = "sources")]
pub mod converter;

pub mod signals;
pub mod spectrogram;
pub mod types;

pub use audio_source::*;
pub use collector::*;

#[cfg(feature = "sources")]
pub use converter::*;

pub use signals::*;
pub use spectrogram::*;
pub use types::*;
