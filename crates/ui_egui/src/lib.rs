//! The native egui frontend of Blaulicht (the desk): every page, the plugin
//! UI renderer, external screens and the 3D visualizer. It reaches the engine
//! only through an `EngineClient` (`blaulicht_engine::client::LocalClient`
//! in the desk binary).
#![warn(clippy::all, rust_2018_idioms)]

pub mod app;
pub mod cli;
pub mod desk;
pub mod stage_assets;
