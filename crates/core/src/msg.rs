use blaulicht_audio_engine::Signal;
use blaulicht_shared::{LogLevel, PluginStateLocation};
use serde::{Deserialize, Serialize};
use std::{borrow::Cow, time::Duration};

#[cfg(feature = "audio")]
use cpal::{Device, HostId};

#[derive(Clone, Serialize, Debug)]
pub struct WasmControlsLog {
    pub x: u8,
    pub y: u8,
    pub value: String,
}

#[derive(Clone, Serialize, Debug)]
pub struct WasmControlsSet {
    pub x: u8,
    pub y: u8,
    pub value: bool,
}

#[derive(Clone, Serialize, Debug)]
pub struct WasmControlsConfig {
    pub x: u8,
    pub y: u8,
}

#[derive(Clone, Copy, Default)]
pub struct DmxTickSpeeds {
    pub dmx_engine: Duration,
    pub dmx_write: Duration,
}

#[derive(Clone, Default)]
pub struct TickSpeeds {
    pub loop_total: Duration,
    pub plugins: Duration,
    pub audio_processing: Duration,
    pub dmx: DmxTickSpeeds,
}

#[derive(Clone)]
pub enum SystemMessage {
    // System.
    Heartbeat(usize),
    EngineInitializationComplete,
    Log(String, LogLevel),
    WasmLog(WasmLogBody),

    TickSpeeds(TickSpeeds),

    // // Performance.
    // LoopSpeed(Duration),
    // TickSpeed(Duration),
    // Audio.
    AudioSelected(Option<AudioDeviceT>),
    AudioDevicesView(Vec<(AudioHostT, AudioDeviceT)>),
    // DMX.
    DMX(Box<[u8; 513]>),
    // Plugin state.
    SavePluginState {
        plugin_name: String,
        state_data: String,
        location: PluginStateLocation,
    },
    PluginAlert {
        plugin_id: u8,
        label: String,
        duration_ms: u32,
    },
}

/// Tracing target for events that must only reach the terminal / file
/// subscriber. The UI log layer skips events with this target because the
/// same information is already delivered to the log window through a
/// dedicated [`SystemMessage`] (e.g. [`SystemMessage::WasmLog`]).
pub const TERMINAL_ONLY_LOG_TARGET: &str = "blaulicht::terminal_only";

#[derive(Clone, Serialize, Debug)]
pub struct WasmLogBody {
    pub plugin_id: u8,
    pub msg: Cow<'static, str>,
    pub additional: Option<String>,
    pub level: LogLevel,
}

#[derive(Clone)]
pub enum UnifiedMessage {
    Signal(Signal),
    System(SystemMessage),
}

#[derive(Deserialize, Clone, Debug)]
pub struct MidiEvent {
    pub device: u8,
    pub status: u8,
    pub data0: u8,
    pub data1: u8,
}

#[cfg(feature = "audio")]
pub type AudioHostT = HostId;

#[cfg(not(feature = "audio"))]
pub type AudioHostT = ();

#[cfg(feature = "audio")]
pub type AudioDeviceT = Device;

#[cfg(not(feature = "audio"))]
pub type AudioDeviceT = MockAudioDevice;

#[derive(Clone, Copy, Default)]
pub struct MockAudioDevice {}

#[cfg(not(feature = "audio"))]
impl MockAudioDevice {
    pub fn name(&self) -> Result<String, std::convert::Infallible> {
        Ok("dummy-name".to_string())
    }
}

#[derive(Clone)]
pub enum FromFrontend {
    Reload,
    SelectInputDevice(Option<AudioDeviceT>),
}
