//! Turns per-tick audio snapshots into beat/BPM updates for forgeDMX.

use blaulicht_shared::{AudioSourceStatus, CollectedAudioSnapshot};

pub const BPM_MIN: f32 = 20.0;
pub const BPM_MAX: f32 = 300.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Beat {
    /// Position in the bar, 1..=4.
    pub position: u8,
    /// True when the position was counted locally (source has no bar info).
    pub estimated: bool,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Update {
    pub beat: Option<Beat>,
    /// Valid BPM to report (None when the source is inactive or out of range).
    pub bpm: Option<f32>,
}

#[derive(Default)]
pub struct BeatTracker {
    last_event: u64,
    last_source: Option<(u8, u8)>,
    position: u8,
}

pub fn valid_bpm(audio: &CollectedAudioSnapshot) -> Option<f32> {
    let active = audio.tempo_source.is_some()
        || audio.manual_tempo
        || matches!(audio.source_status, AudioSourceStatus::Active);
    (active && audio.bpm.is_finite() && (BPM_MIN..=BPM_MAX).contains(&audio.bpm))
        .then_some(audio.bpm)
}

impl BeatTracker {
    pub fn update(&mut self, audio: &CollectedAudioSnapshot) -> Update {
        if self.last_source != audio.tempo_source {
            self.last_source = audio.tempo_source;
            self.position = 0;
        }
        let bpm = valid_bpm(audio);
        let fresh = audio.beat_event_id != 0 && audio.beat_event_id != self.last_event;
        self.last_event = audio.beat_event_id;
        if !fresh || bpm.is_none() {
            return Update { beat: None, bpm };
        }
        let beat = match audio.beat_in_bar.filter(|p| (1..=4).contains(p)) {
            Some(p) => Beat { position: p, estimated: false },
            None => Beat { position: self.position % 4 + 1, estimated: true },
        };
        self.position = beat.position;
        Update { beat: Some(beat), bpm }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(id: u64, bpm: f32, pos: Option<u8>) -> CollectedAudioSnapshot {
        CollectedAudioSnapshot {
            bpm,
            beat_trigger: id != 0,
            beat_event_id: id,
            beat_in_bar: pos,
            source_status: AudioSourceStatus::Active,
            ..Default::default()
        }
    }

    #[test]
    fn detects_beats_by_event_id() {
        let mut t = BeatTracker::default();
        assert!(t.update(&snap(1, 128.0, None)).beat.is_some());
        assert!(t.update(&snap(1, 128.0, None)).beat.is_none());
        assert!(t.update(&snap(2, 128.0, None)).beat.is_some());
    }

    #[test]
    fn estimated_position_wraps() {
        let mut t = BeatTracker::default();
        let got: Vec<u8> = (1..=6)
            .map(|i| t.update(&snap(i, 120.0, None)).beat.unwrap().position)
            .collect();
        assert_eq!(got, vec![1, 2, 3, 4, 1, 2]);
    }

    #[test]
    fn source_position_wins_and_estimate_continues_from_it() {
        let mut t = BeatTracker::default();
        let b = t.update(&snap(1, 120.0, Some(3))).beat.unwrap();
        assert_eq!(b, Beat { position: 3, estimated: false });
        let b = t.update(&snap(2, 120.0, None)).beat.unwrap();
        assert_eq!(b, Beat { position: 4, estimated: true });
    }

    #[test]
    fn suppresses_out_of_range_or_inactive() {
        let mut t = BeatTracker::default();
        let u = t.update(&snap(1, 400.0, None));
        assert_eq!(u, Update::default());
        let mut s = snap(2, 128.0, None);
        s.source_status = AudioSourceStatus::Disconnected;
        assert_eq!(t.update(&s), Update::default());
    }
}
