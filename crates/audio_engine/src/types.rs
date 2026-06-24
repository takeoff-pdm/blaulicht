use serde::Serialize;

use blaulicht_shared::SectionState;

use crate::SignalDebugData;

#[derive(Clone, Copy, Serialize, Debug)]
pub struct BpmInfo {
    pub bpm: f32,
    pub time_between_beats_millis: u16,
}

#[derive(Clone, Serialize, Debug)]
pub enum Signal {
    Bpm(BpmInfo),
    BeatVolume(u8),
    Bass(u8),
    // BassAvgShort(u8),
    BassAvg(u8),
    DebugData(SignalDebugData),
    Volume(u8),
    BeatTrigger(bool),
    Section(SectionState),
}
