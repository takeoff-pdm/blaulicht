use std::collections::VecDeque;

use blaulicht_audio_engine::LoopTempoEstimator;
use blaulicht_shared::CollectedAudioSnapshot;

const BARS_PER_ANALYSIS: f32 = 2.0;
const BEATS_PER_BAR: f32 = 4.0;
const MAX_BUFFER_SECONDS: usize = 12;
const ANALYSIS_INTERVAL_MS: u64 = 1_000;
const RESULT_MAX_AGE_MS: u64 = 3_000;
const MAX_LIVE_DISAGREEMENT: f32 = 0.08;

#[derive(Clone, Copy, Debug)]
struct VerifiedTempo {
    bpm: f32,
    score: f32,
    analyzed_at_ms: u64,
}

/// Adapts the complete-loop estimator to the core's live input.
///
/// The live estimator supplies an approximate BPM, which determines the exact
/// duration of a two-bar PCM window. That makes the window circular at any
/// starting phase and lets the loop estimator independently verify its onset
/// quantization before its result is published to core consumers.
pub(crate) struct LoopTempoRuntime {
    samples: VecDeque<f32>,
    sample_rate: Option<u32>,
    last_analysis_ms: Option<u64>,
    verified: Option<VerifiedTempo>,
}

impl Default for LoopTempoRuntime {
    fn default() -> Self {
        Self {
            samples: VecDeque::new(),
            sample_rate: None,
            last_analysis_ms: None,
            verified: None,
        }
    }
}

impl LoopTempoRuntime {
    pub(crate) fn update(
        &mut self,
        now_ms: u64,
        sample_rate: Option<u32>,
        new_samples: &[f32],
        snapshot: &mut CollectedAudioSnapshot,
    ) {
        self.ingest(sample_rate, new_samples);
        let live_bpm = snapshot.bpm;

        if self.analysis_due(now_ms) {
            self.last_analysis_ms = Some(now_ms);
            self.analyze(now_ms, live_bpm);
        }

        let Some(verified) = self.verified else {
            return;
        };
        if now_ms.saturating_sub(verified.analyzed_at_ms) > RESULT_MAX_AGE_MS
            || !tempos_agree(live_bpm, verified.bpm)
        {
            self.verified = None;
            return;
        }

        snapshot.bpm = verified.bpm;
        snapshot.bpm_confidence = snapshot.bpm_confidence.max(verified.score);
        snapshot.time_between_beats_millis = (60_000.0 / verified.bpm)
            .round()
            .clamp(1.0, u16::MAX as f32) as u16;
    }

    fn ingest(&mut self, sample_rate: Option<u32>, new_samples: &[f32]) {
        if sample_rate != self.sample_rate {
            self.samples.clear();
            self.verified = None;
            self.sample_rate = sample_rate;
        }
        self.samples.extend(new_samples.iter().copied());
        let maximum = sample_rate.unwrap_or(0) as usize * MAX_BUFFER_SECONDS;
        if self.samples.len() > maximum {
            self.samples.drain(..self.samples.len() - maximum);
        }
    }

    fn analysis_due(&self, now_ms: u64) -> bool {
        self.last_analysis_ms.map_or(true, |last| {
            now_ms.saturating_sub(last) >= ANALYSIS_INTERVAL_MS
        })
    }

    fn analyze(&mut self, now_ms: u64, live_bpm: f32) {
        let Some(sample_rate) = self.sample_rate else {
            return;
        };
        if !live_bpm.is_finite() || live_bpm <= 0.0 {
            self.verified = None;
            return;
        }

        let window_seconds = BARS_PER_ANALYSIS * BEATS_PER_BAR * 60.0 / live_bpm;
        let required = (window_seconds * sample_rate as f32).round() as usize;
        if required == 0 || self.samples.len() < required {
            return;
        }

        let window: Vec<f32> = self
            .samples
            .iter()
            .skip(self.samples.len() - required)
            .copied()
            .collect();
        let result = LoopTempoEstimator::default().analyze(&window, sample_rate);
        let (Some(bpm), Some(score)) = (result.bpm, result.score) else {
            self.verified = None;
            return;
        };
        let bpm = bpm as f32;
        if tempos_agree(live_bpm, bpm) {
            self.verified = Some(VerifiedTempo {
                bpm,
                score: score as f32,
                analyzed_at_ms: now_ms,
            });
        } else {
            self.verified = None;
        }
    }
}

fn tempos_agree(live_bpm: f32, verified_bpm: f32) -> bool {
    live_bpm.is_finite()
        && live_bpm > 0.0
        && ((live_bpm - verified_bpm).abs() / live_bpm) <= MAX_LIVE_DISAGREEMENT
}

#[cfg(test)]
mod tests {
    use super::*;

    fn click_loop(bpm: f32, bars: usize, sample_rate: u32) -> Vec<f32> {
        let beats = bars as f32 * BEATS_PER_BAR;
        let mut samples = vec![0.0; (beats * 60.0 / bpm * sample_rate as f32) as usize];
        let tatum_samples = (60.0 / bpm / 4.0 * sample_rate as f32).round() as usize;
        for tatum in 0..bars * 16 {
            let start = tatum * tatum_samples;
            for offset in 0..96.min(samples.len().saturating_sub(start)) {
                let accent = if tatum % 4 == 0 { 1.0 } else { 0.4 };
                samples[start + offset] = accent * (1.0 - offset as f32 / 96.0);
            }
        }
        samples
    }

    #[test]
    fn verifies_and_publishes_live_tempo() {
        let sample_rate = 44_100;
        let samples = click_loop(120.0, 2, sample_rate);
        let mut runtime = LoopTempoRuntime::default();
        let mut snapshot = CollectedAudioSnapshot {
            bpm: 120.0,
            bpm_confidence: 0.2,
            ..CollectedAudioSnapshot::default()
        };

        runtime.update(1_000, Some(sample_rate), &samples, &mut snapshot);

        assert!((snapshot.bpm - 120.0).abs() < 0.01);
        assert!(snapshot.bpm_confidence > 0.8);
        assert_eq!(snapshot.time_between_beats_millis, 500);
    }

    #[test]
    fn does_not_override_a_disagreeing_live_tempo() {
        let sample_rate = 44_100;
        let samples = click_loop(120.0, 2, sample_rate);
        let mut runtime = LoopTempoRuntime::default();
        let mut snapshot = CollectedAudioSnapshot {
            bpm: 170.0,
            bpm_confidence: 0.2,
            ..CollectedAudioSnapshot::default()
        };

        runtime.update(1_000, Some(sample_rate), &samples, &mut snapshot);

        assert_eq!(snapshot.bpm, 170.0);
        assert_eq!(snapshot.bpm_confidence, 0.2);
    }
}
