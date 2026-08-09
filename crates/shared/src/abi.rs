use crate::ControlEventCollection;
use bincode::{config, Decode, Encode};
use serde::{Deserialize, Serialize};
use std::fmt::Display;
use strum::EnumIter;

/// Binary protocol version used for host-to-plugin snapshots.
pub const PLUGIN_ABI_VERSION: u32 = 2;

#[derive(Clone, Encode, Decode, Default)]
pub struct TickInput {
    pub id: u8,
    pub clock: u32,
    pub initial: bool,
    pub audio_data: CollectedAudioSnapshot,
    pub events: ControlEventCollection,
}

/// State of the audio source as observed by the analysis worker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Encode, Decode)]
pub enum AudioSourceStatus {
    Active,
    NoFrame,
    Disconnected,
    Ended,
    Error,
}

impl Default for AudioSourceStatus {
    fn default() -> Self {
        Self::NoFrame
    }
}

/// Macro musical section of the currently playing track, classified by the
/// audio engine. Unlike the per-frame `beat_trigger`/`beat_active`, this is a
/// sustained state describing whether the track is dropping vs. quiet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Encode, Decode)]
pub enum SectionState {
    /// No / low sustained bass (quiet section). Default until enough audio seen.
    #[default]
    Breakdown,
    /// Transient state entered on "no bass -> sudden bass"; held ~5s.
    Drop,
    /// Sustained driving beat after the `Drop` window expires.
    ActiveBeat,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, Encode, Decode)]
pub struct CollectedAudioSnapshot {
    pub time: u64,
    pub volume: u8,
    pub beat_volume: u8,
    pub bass: u8,
    // pub bass_avg_short: u8,
    pub bass_avg: u8,
    pub bass_avg_short: u8,
    pub bpm: f32,
    pub time_between_beats_millis: u16,
    pub initial: bool,
    pub beat_trigger: bool,
    pub actual_onset_peak: bool,
    pub section_state: SectionState,
    /// Normalized (0..1) confidence of the BPM/periodicity estimate.
    pub bpm_confidence: f32,
    /// Monotonic event identity. Zero means that no event has been observed.
    #[serde(default)]
    pub beat_event_id: u64,
    /// Monotonic event identity. Zero means that no event has been observed.
    #[serde(default)]
    pub onset_event_id: u64,
    /// Age of the source frame used for the continuous values.
    #[serde(default)]
    pub frame_age_ms: u32,
    #[serde(default)]
    pub source_status: AudioSourceStatus,
}

#[derive(Clone, Debug, PartialEq, EnumIter, Serialize)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Err,
}

impl Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl From<&str> for LogLevel {
    fn from(value: &str) -> Self {
        match value {
            "Debug" => Self::Debug,
            "Info" => Self::Info,
            "Warn" => Self::Warn,
            "Err" => Self::Err,
            _ => unreachable!("Parse error in log level conversion"),
        }
    }
}

impl TryFrom<i32> for LogLevel {
    type Error = ();

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        Ok(match value {
            0 => Self::Debug,
            1 => Self::Info,
            2 => Self::Warn,
            3 => Self::Err,
            _ => return Err(()),
        })
    }
}

impl From<LogLevel> for i32 {
    fn from(value: LogLevel) -> Self {
        match value {
            LogLevel::Debug => 0,
            LogLevel::Info => 1,
            LogLevel::Warn => 2,
            LogLevel::Err => 3,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PluginStateLocation {
    Showfile = 0,
    Global = 1,
}

impl TryFrom<u8> for PluginStateLocation {
    type Error = ();

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Ok(match value {
            0 => Self::Showfile,
            1 => Self::Global,
            _ => return Err(()),
        })
    }
}

impl From<PluginStateLocation> for u8 {
    fn from(value: PluginStateLocation) -> Self {
        value as u8
    }
}

// const TICKINPUT_WIREFORMAT_LENGTH: usize = 2;

// const VALUE_CLOCK_INDEX: usize = 0;
// const VALUE_INITIAL_INDEX: usize = 1;

impl TickInput {
    pub fn serialize(&self) -> Vec<u8> {
        bincode::encode_to_vec(self, config::standard()).unwrap()
    }

    pub fn deserialize(buf: &[u8]) -> Self {
        let res = bincode::decode_from_slice(buf, config::standard());

        let data = match res {
            Ok((data, _)) => data,
            Err(err) => {
                // Panic handler is usually not registered yet.
                panic!("Deserialize error: {}", err);
            }
        };

        data
    }
}

// impl TickInput {
//     pub fn serialize(&self) -> [i32; TICKINPUT_WIREFORMAT_LENGTH] {
//         let mut buf = [0; TICKINPUT_WIREFORMAT_LENGTH];

//         buf[VALUE_CLOCK_INDEX] = self.clock as i32;
//         buf[VALUE_INITIAL_INDEX] = if self.initial { 1 } else { 0 };

//         buf
//     }

// pub fn deserialize(arr: &[u32]) -> Self {
//     if arr.len() != TICKINPUT_WIREFORMAT_LENGTH {
//         panic!(
//             "tick array len in 'TickInput::deserialize' is not expected length: {}",
//             arr.len()
//         );
//     }

//     Self {
//         clock: arr[VALUE_CLOCK_INDEX] as u32,
//         initial: arr[VALUE_INITIAL_INDEX] != 0,
//         // TODO: use serde for this serialization.
//         audio_data: CollectedAudioSnapshot {
//             time: 0,
//             volume: 0,
//             beat_volume: 0,
//             bass: 0,
//             bass_avg_short: 0,
//             bass_avg: 0,
//             bpm: 0,
//             time_between_beats_millis: 0,
//             initial: false,
//         },
//     }
// }
// }

//
//
//
