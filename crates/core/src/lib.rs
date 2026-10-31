#![warn(clippy::all, rust_2018_idioms)]
#![feature(never_type)]

pub mod app;
pub mod audio;
pub mod cli;
pub mod client;
pub mod command;
pub mod config;
pub mod dmx;
pub mod event;
pub mod frontend_ui;
pub mod hub;
pub mod log;
pub mod mainloop;
pub mod msg;
pub mod persistence;
pub mod plugin;
pub mod routes;
pub mod stage;
pub mod stage_assets;
pub mod state;
pub mod ui_ops;
pub mod util;
pub mod utils;
