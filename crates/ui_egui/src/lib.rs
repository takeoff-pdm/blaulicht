//! The native egui frontend of Blaulicht (the desk): every page, the plugin
//! UI renderer, external screens and the 3D visualizer. It reaches the engine
//! only through an `EngineClient` ([`client::AppClient`]: a `LocalClient` in
//! the desk binary, a `RemoteClient` in the web frontend, `crates/web`).
#![warn(clippy::all, rust_2018_idioms)]
// clippy.toml bans std::time's clocks (they panic in a browser). On native
// `web_time` re-exports them, so the ban can only be checked in a wasm32
// build.
#![cfg_attr(
    not(target_arch = "wasm32"),
    allow(clippy::disallowed_methods, clippy::disallowed_types)
)]

pub mod app;
pub mod cli;
pub mod client;
// `eframe::run_native` / `NativeOptions` don't exist on wasm32 (048).
#[cfg(not(target_arch = "wasm32"))]
pub mod desk;
pub mod layout;
#[cfg(feature = "visualizer")]
pub mod stage_assets;
