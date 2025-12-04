use std::{borrow::Cow, time::Duration};

use blaulicht_audio_engine::Signal;
use blaulicht_shared::LogLevel;
use cpal::{Device, HostId};
use serde::{Deserialize, Serialize};

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

#[derive(Clone)]
pub enum SystemMessage {
    // System.
    Heartbeat(usize),
    Log(String, LogLevel),
    WasmLog(WasmLogBody),
    // Performance.
    LoopSpeed(Duration),
    TickSpeed(Duration),
    // Audio.
    AudioSelected(Option<Device>),
    AudioDevicesView(Vec<(HostId, Device)>),
    // DMX.
    DMX(Box<[u8; 513]>),
    // Plugin state.
    SavePluginState {
        plugin_name: String,
        state_data: String,
    },
}

#[derive(Clone, Serialize, Debug)]
pub struct WasmLogBody {
    pub plugin_id: u8,
    pub msg: Cow<'static, str>,
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

#[derive(Clone)]
pub enum FromFrontend {
    Reload,
    SelectInputDevice(Option<Device>),
}
