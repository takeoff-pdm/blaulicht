use std::{borrow::Cow, time::Duration};

use cpal::{Device, HostId};
use serde::Serialize;

#[derive(Clone, Serialize, Debug)]
pub struct BpmInfo {
    pub bpm: u8,
    pub time_between_beats_millis: u16,
}

#[derive(Clone, Serialize, Debug)]
pub enum Signal {
    Bpm(BpmInfo),
    BeatVolume(u8),
    Bass(u8),
    BassAvgShort(u8),
    BassAvg(u8),
    Volume(u8),
}

#[derive(Clone, Serialize)]
pub struct WasmControlsLog {
    pub x: u8,
    pub y: u8,
    pub value: String,
}

#[derive(Clone, Serialize)]
pub struct WasmControlsSet {
    pub x: u8,
    pub y: u8,
    pub value: bool,
}

#[derive(Clone, Serialize)]
pub struct WasmControlsConfig {
    pub x: u8,
    pub y: u8,
}

#[derive(Clone)]
pub enum SystemMessage {
    // System.
    Heartbeat(usize),
    Log(String),
    WasmLog(WasmLogBody),
    // Controls.
    WasmControlsLog(WasmControlsLog),
    WasmControlsSet(WasmControlsSet),
    WasmControlsConfig(WasmControlsConfig),
    // Performance.
    LoopSpeed(Duration),
    TickSpeed(Duration),
    // Audio.
    AudioSelected(Option<Device>),
    AudioDevicesView(Vec<(HostId, Device)>),
    // DMX.
    DMX(Box<[u8; 513]>),
}

#[derive(Clone, Serialize)]
pub struct WasmLogBody {
    pub plugin_id: u8,
    pub msg: Cow<'static, str>,
}

#[derive(Clone)]
pub enum UnifiedMessage {
    Signal(Signal),
    System(SystemMessage),
}
