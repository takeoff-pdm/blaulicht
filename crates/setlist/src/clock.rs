//! Following a track on the engine's beat clock, without deck data.
//!
//! The operator tells the clock where the track is (sync on its first
//! downbeat, on a section start, or a click on the timeline); from then on it
//! runs at the live tempo and pulls itself onto each beat the engine reports.
//! The beats can come from the audio analyzer or, with "audio sync" on in the
//! Pro DJ Link plugin, from the CDJs, so the clock follows tempo changes and
//! survives breakdowns without a kick.

use crate::map::{TrackMap, BEATS_PER_BAR};

/// Share of the phase error removed per beat event; the rest is jitter.
const PULL: f64 = 0.5;
/// Beat events further off than this are off-beats or noise, not ours.
const MAX_PHASE_ERROR: f64 = 0.3;
/// Live tempo is used only this close to the map's (DJ pitch range).
const MAX_TEMPO_RATIO_ERROR: f64 = 0.12;
const MIN_BPM_CONFIDENCE: f32 = 0.3;

/// One engine tick's audio data, as far as the clock cares.
#[derive(Clone, Copy, Debug, Default)]
pub struct ClockInput {
    pub now_ms: u64,
    /// Monotonic; a change means a beat was just heard.
    pub beat_event_id: u64,
    pub bpm: f32,
    pub bpm_confidence: f32,
    /// Position of that beat in its bar (1..=4), from the tempo source.
    pub beat_in_bar: Option<u8>,
}

#[derive(Debug, Default)]
pub struct BeatClock {
    position: f64,
    at_ms: Option<u64>,
    running: bool,
    /// Tempo in use; 0 until a track sets it.
    bpm: f64,
    last_event_id: u64,
    last_event_ms: Option<u64>,
    /// Beat events that matched the clock since the last sync or jump.
    matched: u32,
    /// Correct the bar phase from `beat_in_bar` (trust the tempo source's
    /// downbeats over the analyzer's).
    pub follow_bar_phase: bool,
}

impl BeatClock {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn position(&self) -> f64 {
        self.position
    }

    pub fn running(&self) -> bool {
        self.running
    }

    pub fn bpm(&self) -> f64 {
        self.bpm
    }

    /// Beat events matched since the last sync; 0 means free-running.
    pub fn matched(&self) -> u32 {
        self.matched
    }

    /// A new track: stopped at its start, at its own tempo.
    pub fn load(&mut self, map: &TrackMap) {
        self.position = 0.0;
        self.running = false;
        self.bpm = map.bpm;
        self.matched = 0;
    }

    pub fn stop(&mut self) {
        self.running = false;
    }

    pub fn start(&mut self, now_ms: u64) {
        self.at_ms = Some(now_ms);
        self.running = true;
    }

    /// The operator says grid beat `beat` is the beat they just pressed on.
    /// A press in the first half of a beat means the beat just heard, one in
    /// the second half the one about to come.
    pub fn sync(&mut self, now_ms: u64, beat: f64) {
        let period_ms = 60_000.0 / self.bpm.max(1.0);
        let since = self
            .last_event_ms
            .map(|t| now_ms.saturating_sub(t) as f64)
            .filter(|ms| *ms < 2.0 * period_ms)
            .unwrap_or(0.0);
        let phase = (since / period_ms).fract();
        self.position = if phase < 0.5 {
            beat + phase
        } else {
            beat - (1.0 - phase)
        };
        self.at_ms = Some(now_ms);
        self.running = true;
        self.matched = 0;
    }

    pub fn nudge(&mut self, beats: f64) {
        self.position += beats;
        self.matched = 0;
    }

    pub fn tick(&mut self, input: &ClockInput, map: &TrackMap) {
        self.update_tempo(input, map);
        if let (true, Some(at)) = (self.running, self.at_ms) {
            let dt = input.now_ms.saturating_sub(at) as f64;
            self.position += dt * self.bpm / 60_000.0;
        }
        self.at_ms = Some(input.now_ms);

        if input.beat_event_id == self.last_event_id || input.beat_event_id == 0 {
            return;
        }
        self.last_event_id = input.beat_event_id;
        self.last_event_ms = Some(input.now_ms);
        if !self.running {
            return;
        }
        let error = self.position - self.position.round();
        if error.abs() < MAX_PHASE_ERROR {
            self.position -= error * PULL;
            self.matched = self.matched.saturating_add(1);
        }
        if let (true, Some(in_bar)) = (self.follow_bar_phase, input.beat_in_bar) {
            let beat = self.position.round() as i64;
            let ours = (beat - map.downbeat as i64).rem_euclid(BEATS_PER_BAR as i64);
            let theirs = (in_bar as i64 - 1).rem_euclid(BEATS_PER_BAR as i64);
            // Shortest way round: -1, 0, +1 or +2 beats.
            let shift = (theirs - ours + 1).rem_euclid(4) - 1;
            self.position += shift as f64;
        }
    }

    fn update_tempo(&mut self, input: &ClockInput, map: &TrackMap) {
        if self.bpm <= 0.0 {
            self.bpm = map.bpm;
        }
        if input.bpm <= 0.0 || input.bpm_confidence < MIN_BPM_CONFIDENCE {
            return;
        }
        // Fold half- and double-time estimates onto the map's tempo.
        let live = input.bpm as f64;
        let folded = [live, live * 2.0, live / 2.0]
            .into_iter()
            .min_by(|a, b| (a / map.bpm - 1.0).abs().total_cmp(&(b / map.bpm - 1.0).abs()))
            .unwrap_or(live);
        if (folded / map.bpm - 1.0).abs() <= MAX_TEMPO_RATIO_ERROR {
            self.bpm = folded;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::tests::test_map;
    use crate::map::SectionKind::Intro;

    struct Sim {
        clock: BeatClock,
        map: TrackMap,
        now: u64,
        event: u64,
    }

    impl Sim {
        fn new() -> Self {
            let map = test_map(&[(Intro, 64)]);
            let mut clock = BeatClock::new();
            clock.load(&map);
            Self {
                clock,
                map,
                now: 1_000,
                event: 0,
            }
        }

        /// Ticks every 12 ms for `ms`; a beat event every `period` ms from
        /// `first_beat` on (`None` for no beats).
        fn run(&mut self, ms: u64, beats: Option<(f64, f64)>, bpm: f32) {
            let end = self.now + ms;
            while self.now < end {
                self.now += 12;
                if let Some((first, period)) = beats {
                    let t = self.now as f64 - first;
                    if t >= 0.0 && (t % period) < 12.0 {
                        self.event += 1;
                    }
                }
                let input = ClockInput {
                    now_ms: self.now,
                    beat_event_id: self.event,
                    bpm,
                    bpm_confidence: 0.9,
                    beat_in_bar: None,
                };
                self.clock.tick(&input, &self.map);
            }
        }
    }

    #[test]
    fn free_runs_at_the_map_tempo() {
        let mut sim = Sim::new();
        sim.clock.sync(sim.now, 1.0);
        sim.run(60_000, None, 0.0);
        let expected = 1.0 + 128.0;
        assert!((sim.clock.position() - expected).abs() < 0.05, "{}", sim.clock.position());
    }

    #[test]
    fn follows_a_pitched_deck_through_its_beats() {
        let mut sim = Sim::new();
        // The DJ plays the 128 BPM track at 131 BPM; the analyzer agrees.
        let period = 60_000.0 / 131.0;
        let first = sim.now as f64 + 5.0;
        sim.run(10, Some((first, period)), 131.0);
        sim.clock.sync(sim.now, 1.0);
        sim.run(30_000, Some((first, period)), 131.0);
        let beats_heard = ((sim.now as f64 - first) / period).floor();
        let expected = 1.0 + beats_heard + ((sim.now as f64 - first) % period) / period;
        assert!(
            (sim.clock.position() - expected).abs() < 0.08,
            "{} vs {expected}",
            sim.clock.position()
        );
        assert!(sim.clock.matched() > 50);
    }

    #[test]
    fn pulls_a_wrong_tempo_onto_the_beats() {
        let mut sim = Sim::new();
        let period = 60_000.0 / 128.0;
        let first = sim.now as f64;
        sim.clock.sync(sim.now, 0.0);
        // No usable live tempo (confidence too low is modeled by 0 BPM), and
        // the clock was synced a bit late: beat events pull it back in.
        sim.clock.nudge(0.2);
        sim.run(10_000, Some((first, period)), 0.0);
        let phase = sim.clock.position() - sim.clock.position().round();
        let expected_phase = ((sim.now as f64 - first) % period) / period;
        let expected_phase = if expected_phase > 0.5 {
            expected_phase - 1.0
        } else {
            expected_phase
        };
        assert!((phase - expected_phase).abs() < 0.05, "{phase} vs {expected_phase}");
    }

    #[test]
    fn half_time_estimates_are_folded() {
        let mut sim = Sim::new();
        sim.clock.sync(sim.now, 0.0);
        sim.run(1_000, None, 65.0);
        assert_eq!(sim.clock.bpm(), 130.0);
        // Way off: ignored, keep the last good tempo.
        sim.run(1_000, None, 100.0);
        assert_eq!(sim.clock.bpm(), 130.0);
    }

    #[test]
    fn sync_snaps_to_the_nearer_beat() {
        let mut sim = Sim::new();
        let period = 60_000.0 / 128.0;
        sim.run(500, Some((sim.now as f64 + 12.0, period)), 128.0);
        let last_beat = sim.clock.last_event_ms.unwrap();
        // Pressed 100 ms after a beat: that beat was beat 9.
        let mut c = std::mem::take(&mut sim.clock);
        c.sync(last_beat + 100, 9.0);
        assert!((c.position() - (9.0 + 100.0 / period)).abs() < 0.03);
        // Pressed 100 ms before the next one: that one is beat 9.
        c.sync(last_beat + (period as u64 - 100), 9.0);
        assert!((c.position() - (9.0 - 100.0 / period)).abs() < 0.03);
    }

    #[test]
    fn bar_phase_follows_the_tempo_source() {
        let mut sim = Sim::new();
        sim.clock.follow_bar_phase = true;
        sim.clock.sync(sim.now, 2.0);
        // The map's downbeat is beat 1, so the clock thinks beat 2 is the
        // 2nd beat of the bar; the CDJ says it is the 4th.
        sim.event += 1;
        let input = ClockInput {
            now_ms: sim.now,
            beat_event_id: sim.event,
            bpm: 128.0,
            bpm_confidence: 1.0,
            beat_in_bar: Some(4),
        };
        sim.clock.tick(&input, &sim.map);
        assert!((sim.clock.position() - 4.0).abs() < 0.01, "{}", sim.clock.position());
    }
}
