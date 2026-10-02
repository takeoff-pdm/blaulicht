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
    /// Progress of one engine startup stage, shown in the init popup and the
    /// navbar status indicator. Host-internal, never sent to plugins.
    StartupStage {
        stage: StartupStage,
        status: StageStatus,
    },
    /// Host log line for the Logs page. Only `crate::log`'s tracing layer
    /// sends this; everything else logs through `tracing`.
    Log {
        message: String,
        level: LogLevel,
        source: Cow<'static, str>,
    },
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

/// Engine startup stages, in the order they are shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq, strum::EnumIter, strum::EnumCount)]
pub enum StartupStage {
    Config,
    Showfile,
    Audio,
    Plugins,
    DmxEngine,
    MainLoop,
}

impl StartupStage {
    pub fn label(self) -> &'static str {
        match self {
            StartupStage::Config => "Config",
            StartupStage::Showfile => "Showfile",
            StartupStage::Audio => "Audio",
            StartupStage::Plugins => "Plugins",
            StartupStage::DmxEngine => "DMX engine",
            StartupStage::MainLoop => "Main loop",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StageState {
    Pending,
    Running,
    Ok,
    Failed,
    /// Only used for [`StartupStage::MainLoop`] while the supervisor restarts
    /// a crashed engine.
    Restarting,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StageStatus {
    pub state: StageState,
    pub detail: Option<String>,
}

impl StageStatus {
    pub const PENDING: Self = Self {
        state: StageState::Pending,
        detail: None,
    };

    fn with(state: StageState, detail: impl Into<String>) -> Self {
        let detail = detail.into();
        Self {
            state,
            detail: (!detail.is_empty()).then_some(detail),
        }
    }

    pub fn running(detail: impl Into<String>) -> Self {
        Self::with(StageState::Running, detail)
    }

    pub fn ok(detail: impl Into<String>) -> Self {
        Self::with(StageState::Ok, detail)
    }

    pub fn failed(detail: impl Into<String>) -> Self {
        Self::with(StageState::Failed, detail)
    }

    pub fn restarting(detail: impl Into<String>) -> Self {
        Self::with(StageState::Restarting, detail)
    }
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
