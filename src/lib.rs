#![warn(clippy::all, rust_2018_idioms)]

pub mod app;
pub mod audio;
pub mod config;
pub mod dmx;
pub mod routes;
pub mod utils;
pub mod wasm;
pub mod midi;

pub struct DmxData {
    channels: [u8; 512],
}

pub enum ToFrontent {
    Dmx(DmxData),
    Bpm(u16),
}
