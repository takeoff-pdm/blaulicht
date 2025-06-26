#[derive(Clone, Copy)]
#[derive(Default)]
pub struct TickInput {
    pub clock: u32,
    pub initial: bool,
    pub audio_data: CollectedAudioSnapshot,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct CollectedAudioSnapshot {
    pub time: i32,
    pub volume: u8,
    pub beat_volume: u8,
    pub bass: u8,
    pub bass_avg_short: u8,
    pub bass_avg: u8,
    pub bpm: u8,
    pub time_between_beats_millis: u16,
    pub initial: bool,
}

const TICKINPUT_WIREFORMAT_LENGTH: usize = 2;

const VALUE_CLOCK_INDEX: usize = 0;
const VALUE_INITIAL_INDEX: usize = 1;

impl TickInput {
    pub fn serialize(&self) -> [i32; TICKINPUT_WIREFORMAT_LENGTH] {
        let mut buf = [0; TICKINPUT_WIREFORMAT_LENGTH];

        buf[VALUE_CLOCK_INDEX] = self.clock as i32;
        buf[VALUE_INITIAL_INDEX] = if self.initial { 1 } else { 0 };

        buf
    }

pub fn deserialize(arr: &[u32]) -> Self {
    if arr.len() != TICKINPUT_WIREFORMAT_LENGTH {
        panic!(
            "tick array len in 'TickInput::deserialize' is not expected length: {}",
            arr.len()
        );
    }

    Self {
        clock: arr[VALUE_CLOCK_INDEX] as u32,
        initial: arr[VALUE_INITIAL_INDEX] != 0,    
        // TODO: use serde for this serialization.
        audio_data: CollectedAudioSnapshot {
            time: 0,
            volume: 0,
            beat_volume: 0,
            bass: 0,
            bass_avg_short: 0,
            bass_avg: 0,
            bpm: 0,
            time_between_beats_millis: 0,
            initial: false,
        },
    }
}
}