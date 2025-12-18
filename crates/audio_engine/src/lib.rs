pub const VAR: usize = 42;

pub mod audio_source;
pub mod collector;
pub mod converter;
pub mod signals;
pub mod spectrogram;
pub mod types;

pub use audio_source::*;
pub use collector::*;
pub use converter::*;
pub use signals::*;
pub use spectrogram::*;
pub use types::*;
