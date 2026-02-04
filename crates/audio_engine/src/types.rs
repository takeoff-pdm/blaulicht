use serde::Serialize;

#[derive(Clone, Copy, Serialize, Debug)]
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
    BassDerivative(Vec<f64>),
    Volume(u8),
    BeatTrigger(bool),
}
