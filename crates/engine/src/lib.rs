//! The Blaulicht engine: DMX output, the main loop and its supervisor, the
//! wasmtime plugin host, the audio bridge, the event bus and system hub,
//! show state, config and persistence, and the `EngineClient` that frontends
//! use. No UI toolkit: frontends (the egui desk in `blaulicht-ui-egui`, later
//! remote ones) attach through a client.
#![warn(clippy::all, rust_2018_idioms)]
#![feature(never_type)]

pub mod audio;
pub mod cli;
pub mod client;
pub mod command;
pub mod config;
pub mod dmx;
pub mod event;
pub mod frontend_ui;
pub mod hub;
pub mod lifecycle;
pub mod log;
pub mod mainloop;
pub mod msg;
pub mod persistence;
pub mod plugin;
pub mod stage;
pub mod stage_assets;
pub mod state;
pub mod ui_ops;
pub mod util;
pub mod utils;

pub use lifecycle::{Engine, EngineHandle, EngineOptions};
