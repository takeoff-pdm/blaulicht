use blaulicht_shared::{AudioSourceStatus, CollectedAudioSnapshot};

#[derive(Default)]
pub struct BeatClock {
    pub step: f64,
    pub loops: u64,
    pub running: bool,
    pub estimated: bool,
    last_clock: Option<u32>,
    last_event: u64,
    last_source: Option<(u8, u8)>,
    since_beat_ms: f64,
    travelled: f64,
}

impl BeatClock {
    pub fn reset(&mut self) {
        self.running = false;
        self.step = 0.0;
        self.loops = 0;
        self.travelled = 0.0;
        self.estimated = false;
    }

    /// Returns true only when entering estimated bar alignment (log once).
    pub fn update(&mut self, now: u32, audio: &CollectedAudioSnapshot) -> bool {
        let dt = self.last_clock.replace(now).map(|last| now.wrapping_sub(last)).unwrap_or(0) as f64;
        let fresh = audio.beat_trigger && audio.beat_event_id != 0 && audio.beat_event_id != self.last_event;
        self.last_event = audio.beat_event_id;
        if self.last_source != audio.tempo_source {
            self.reset();
            self.last_source = audio.tempo_source;
        }
        let valid_source = audio.tempo_source.is_some()
            || matches!(audio.source_status, AudioSourceStatus::Active);
        let valid_tempo = audio.bpm.is_finite() && (20.0..=400.0).contains(&audio.bpm);
        if !valid_source || !valid_tempo || dt > 3_000.0 {
            self.reset();
            return false;
        }
        let period = 60_000.0 / f64::from(audio.bpm);
        self.since_beat_ms += dt;
        if !fresh && self.since_beat_ms > 3.0 * period {
            self.reset();
            return false;
        }
        let position = audio.beat_in_bar.filter(|p| (1..=4).contains(p));
        let mut warning = false;
        if fresh {
            self.since_beat_ms = 0.0;
            if !self.running {
                self.step = f64::from(position.unwrap_or(1) - 1) * 4.0;
                self.running = true;
                self.estimated = position.is_none();
                return self.estimated;
            }
        }
        if !self.running {
            return false;
        }
        let advance = dt / period * 4.0;
        let mut next = self.step + advance;
        let mut anchored = false;
        if fresh {
            if position.is_none() && !self.estimated {
                self.estimated = true;
                warning = true;
            }
            let target = position.map(|p| f64::from(p - 1) * 4.0)
                .unwrap_or_else(|| (next / 4.0).round() * 4.0);
            let modulus = if position.is_some() { 16.0 } else { 4.0 };
            let error = (target - next + modulus / 2.0).rem_euclid(modulus) - modulus / 2.0;
            if position.is_some() && (self.estimated || error.abs() > 2.0) {
                // Authoritative bar acquisition/source seek, not a completed loop.
                next = target;
                anchored = true;
                self.travelled = 0.0;
                self.loops = 0;
            } else {
                // Bound corrections so jitter never moves the playhead backwards.
                next += error.clamp(-advance * 0.25, advance * 0.25);
            }
            if position.is_some() {
                self.estimated = false;
            }
        }
        if !anchored {
            self.travelled += (next - self.step).max(0.0);
        }
        self.loops = (self.travelled / 16.0).floor() as u64;
        self.step = next.rem_euclid(16.0);
        warning
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn audio(id: u64, position: Option<u8>) -> CollectedAudioSnapshot {
        CollectedAudioSnapshot { bpm: 120.0, source_status: AudioSourceStatus::Active,
            beat_trigger: true, beat_event_id: id, beat_in_bar: position, ..Default::default() }
    }
    #[test]
    fn alignment_deduplication_and_sixteenths() {
        let mut clock = BeatClock::default();
        assert!(!clock.update(0, &audio(1, Some(3))));
        assert_eq!(clock.step, 8.0);
        clock.update(125, &audio(1, Some(3)));
        assert_eq!(clock.step, 9.0);
        clock.update(500, &audio(2, Some(4)));
        assert_eq!(clock.step, 12.0);
        clock.update(1_000, &audio(3, Some(1)));
        assert_eq!(clock.step, 0.0);
    }
    #[test]
    fn fallback_warns_once_and_adopts_bar_then_waits_after_dropout() {
        let mut clock = BeatClock::default();
        assert!(clock.update(0, &audio(1, None)));
        assert!(!clock.update(500, &audio(2, None)));
        clock.update(1_000, &audio(3, Some(4)));
        assert_eq!(clock.step, 12.0);
        assert!(!clock.estimated);
        let mut lost = audio(3, None);
        lost.bpm = 0.0;
        clock.update(1_100, &lost);
        assert!(!clock.running);
        clock.update(1_200, &audio(3, Some(4)));
        assert!(!clock.running);
        clock.update(1_500, &audio(4, Some(1)));
        assert!(clock.running);
        assert_eq!(clock.step, 0.0);
    }
    #[test]
    fn clock_wrap_source_switch_and_tempo_change() {
        let mut clock = BeatClock::default();
        clock.update(u32::MAX - 99, &audio(1, Some(1)));
        clock.update(25, &audio(1, Some(1)));
        assert_eq!(clock.step, 1.0);
        let mut faster = audio(2, Some(2));
        faster.bpm = 240.0;
        faster.tempo_source = Some((2, 1));
        faster.source_status = AudioSourceStatus::Disconnected;
        clock.update(150, &faster);
        assert_eq!(clock.step, 4.0);
        clock.update(275, &faster);
        assert_eq!(clock.step, 6.0);
        clock.update(1_100, &faster);
        assert!(!clock.running);
    }
}
