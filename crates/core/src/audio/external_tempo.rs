use std::time::{Duration, Instant};

use blaulicht_shared::CollectedAudioSnapshot;

#[derive(Default)]
pub(crate) struct ExternalTempo {
    source: Option<(u8, f32, Instant)>,
    pending_beats: u64,
    beat_in_bar: Option<u8>,
    device: u8,
}

impl ExternalTempo {
    fn expire(&mut self, now: Instant) {
        if self.source.is_some_and(|(_, _, until)| now >= until) {
            *self = Self::default();
        }
    }

    pub fn update(&mut self, owner: u8, bpm: f32, beat: bool, now: Instant) {
        self.update_bar(owner, bpm, beat, None, 0, now);
    }

    pub fn update_bar(&mut self, owner: u8, bpm: f32, beat: bool, position: Option<u8>, device: u8, now: Instant) {
        self.expire(now);
        if self.source.is_some_and(|(id, _, _)| id != owner) {
            return;
        }
        if bpm == 0.0 {
            *self = Self::default();
        } else if bpm.is_finite() && (20.0..=400.0).contains(&bpm) {
            let lease = Duration::from_secs_f32((180.0 / bpm).clamp(0.25, 3.0));
            if self.device != device {
                self.pending_beats = 0;
                self.beat_in_bar = None;
            }
            self.device = device;
            if beat {
                self.beat_in_bar = position.filter(|v| (1..=4).contains(v));
            }
            self.source = Some((owner, bpm, now + lease));
            self.pending_beats += u64::from(beat);
        }
    }
}

/// Overlay only the published timing; the live analyzer continues independently
/// so disabling sync immediately recovers its current estimate.
#[derive(Default)]
pub(crate) struct TempoOverlay {
    analyzer_tempo: Option<(f32, f32, u16)>,
}

impl TempoOverlay {
    pub fn restore(&mut self, snapshot: &mut CollectedAudioSnapshot) {
        snapshot.beat_in_bar = None;
        snapshot.tempo_source = None;
        if let Some((bpm, confidence, interval)) = self.analyzer_tempo.take() {
            snapshot.bpm = bpm;
            snapshot.bpm_confidence = confidence;
            snapshot.time_between_beats_millis = interval;
        }
    }

    pub fn apply(
        &mut self,
        snapshot: &mut CollectedAudioSnapshot,
        previous_beat_id: u64,
        source: &mut ExternalTempo,
        now: Instant,
    ) {
        snapshot.beat_in_bar = None;
        snapshot.tempo_source = None;
        source.expire(now);
        let Some((owner, bpm, _)) = source.source else {
            return;
        };
        self.analyzer_tempo = Some((
            snapshot.bpm,
            snapshot.bpm_confidence,
            snapshot.time_between_beats_millis,
        ));
        snapshot.bpm = bpm;
        snapshot.bpm_confidence = 1.0;
        snapshot.time_between_beats_millis = (60_000.0 / bpm).round() as u16;
        let beats = std::mem::take(&mut source.pending_beats);
        snapshot.beat_event_id = if beats == 0 {
            previous_beat_id
        } else {
            previous_beat_id.wrapping_add(beats).max(1)
        };
        snapshot.beat_trigger = beats > 0;
        snapshot.beat_in_bar = source.beat_in_bar;
        snapshot.tempo_source = Some((owner, source.device));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_beats_replace_predictions_without_changing_audio() {
        let now = Instant::now();
        let mut source = ExternalTempo::default();
        source.update(1, 120.0, true, now);
        let mut overlay = TempoOverlay::default();
        let mut snapshot = CollectedAudioSnapshot {
            bpm: 97.0,
            bass: 83,
            onset_event_id: 4,
            beat_event_id: 8,
            ..Default::default()
        };
        overlay.apply(&mut snapshot, 7, &mut source, now);
        assert_eq!(
            (
                snapshot.bpm,
                snapshot.beat_event_id,
                snapshot.bass,
                snapshot.onset_event_id
            ),
            (120.0, 8, 83, 4)
        );
        overlay.restore(&mut snapshot);
        assert_eq!(snapshot.bpm, 97.0);
        snapshot.beat_event_id = 9; // A live analyzer prediction is suppressed.
        overlay.apply(&mut snapshot, 8, &mut source, now);
        assert_eq!(snapshot.beat_event_id, 8);
        assert!(!snapshot.beat_trigger);
        overlay.restore(&mut snapshot);
        overlay.apply(&mut snapshot, 8, &mut source, now + Duration::from_secs(2));
        assert_eq!(snapshot.bpm, 97.0);
    }

    #[test]
    fn ownership_release_expiry_and_invalid_tempo() {
        let now = Instant::now();
        let mut source = ExternalTempo::default();
        source.update(1, f32::NAN, true, now);
        assert!(source.source.is_none());
        source.update(1, 120.0, true, now);
        source.update(2, 0.0, false, now);
        assert_eq!(source.source.unwrap().0, 1);
        source.update(1, 0.0, false, now);
        assert!(source.source.is_none());
        assert_eq!(source.pending_beats, 0);
        source.update(2, 130.0, true, now);
        source.update(1, 125.0, true, now + Duration::from_secs(3));
        assert_eq!(source.source.unwrap().0, 1);
        assert_eq!(source.pending_beats, 1);
    }
}
