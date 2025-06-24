pub mod defs;

pub use defs::{SIGNAL_SPEED, SYSTEM_MESSAGE_SPEED};

pub mod stream;
pub mod utils;
pub use stream::run;
mod analysis;
