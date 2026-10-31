use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    thread,
};

use blaulicht_audio_engine::{
    LoopTempoDebugData, LoopTempoEstimator, LoopTempoStatus, SignalDebugData,
};
use crossbeam_channel::{Receiver, Sender, TrySendError};

const BARS_PER_WINDOW: f32 = 2.0;
const BEATS_PER_BAR: f32 = 4.0;
const MIN_BPM: f32 = 80.0;
const MAX_BPM: f32 = 200.0;
const COARSE_STEP_BPM: usize = 2;
const REFINE_RADIUS_BPM: f32 = 2.0;
const REFINE_STEP_BPM: f32 = 0.25;
const MAX_BUFFER_SECONDS: usize = 7;
const ANALYSIS_INTERVAL_MS: u64 = 2_000;
const STABLE_TEMPO_MAX_AGE_MS: u64 = 8_000;
const MAX_TEMPO_DISAGREEMENT: f32 = 0.08;
const ANALYSIS_SAMPLE_RATE: u32 = 8_000;
const CONSENSUS_TOLERANCE: f32 = 0.02;
const HOLD_DEADBAND: f32 = 0.005;
const STABLE_UPDATE_ALPHA: f32 = 0.15;
const INITIAL_CONFIRMATIONS: u8 = 2;
const CHANGE_CONFIRMATIONS: u8 = 3;

#[derive(Default)]
struct CaptureBuffer {
    now_ms: u64,
    sample_rate: u32,
    samples: VecDeque<f32>,
    last_analysis_ms: Option<u64>,
}

#[derive(Clone, Copy, Debug)]
struct LoopTempoResult {
    status: LoopTempoStatus,
    candidate_bpm: f32,
    accepted_bpm: f32,
    score: f32,
    threshold: f32,
    analyzed_at_ms: u64,
    buffered_seconds: f32,
}

impl LoopTempoResult {
    fn debug(self, now_ms: u64, fused: bool) -> LoopTempoDebugData {
        LoopTempoDebugData {
            status: self.status,
            candidate_bpm: self.candidate_bpm,
            accepted_bpm: self.accepted_bpm,
            score: self.score,
            threshold: self.threshold,
            age_ms: now_ms
                .saturating_sub(self.analyzed_at_ms)
                .min(u32::MAX as u64) as u32,
            buffered_seconds: self.buffered_seconds,
            fused,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct FusedTempo {
    pub bpm: f32,
    pub confidence: f32,
}

#[derive(Clone, Copy, Debug)]
struct StableTempo {
    bpm: f32,
    score: f32,
    confirmed_at_ms: u64,
}

#[derive(Clone, Copy, Debug)]
struct TempoChallenger {
    bpm: f32,
    score: f32,
    confirmations: u8,
    observed_at_ms: u64,
}

#[derive(Default)]
struct TempoStabilizer {
    stable: Option<StableTempo>,
    challenger: Option<TempoChallenger>,
}

impl TempoStabilizer {
    fn observe(&mut self, result: LoopTempoResult) {
        if result.status != LoopTempoStatus::Accepted || result.accepted_bpm <= 0.0 {
            return;
        }

        if let Some(stable) = &mut self.stable {
            let observed = align_tempo(stable.bpm, result.accepted_bpm);
            let difference = relative_difference(stable.bpm, observed);
            if difference <= CONSENSUS_TOLERANCE {
                if difference > HOLD_DEADBAND {
                    stable.bpm += (observed - stable.bpm) * STABLE_UPDATE_ALPHA;
                }
                stable.score += (result.score - stable.score) * STABLE_UPDATE_ALPHA;
                stable.confirmed_at_ms = result.analyzed_at_ms;
                self.challenger = None;
                return;
            }
            self.observe_challenger(observed, result, CHANGE_CONFIRMATIONS);
        } else {
            self.observe_challenger(result.accepted_bpm, result, INITIAL_CONFIRMATIONS);
        }
    }

    fn observe_challenger(
        &mut self,
        bpm: f32,
        result: LoopTempoResult,
        required_confirmations: u8,
    ) {
        let challenger = self.challenger.get_or_insert(TempoChallenger {
            bpm,
            score: result.score,
            confirmations: 0,
            observed_at_ms: result.analyzed_at_ms,
        });
        if challenger.confirmations > 0
            && relative_difference(challenger.bpm, bpm) > CONSENSUS_TOLERANCE
        {
            *challenger = TempoChallenger {
                bpm,
                score: result.score,
                confirmations: 1,
                observed_at_ms: result.analyzed_at_ms,
            };
        } else {
            let count = challenger.confirmations as f32;
            challenger.bpm = (challenger.bpm * count + bpm) / (count + 1.0);
            challenger.score = (challenger.score * count + result.score) / (count + 1.0);
            challenger.confirmations = challenger.confirmations.saturating_add(1);
            challenger.observed_at_ms = result.analyzed_at_ms;
        }

        if challenger.confirmations >= required_confirmations {
            self.stable = Some(StableTempo {
                bpm: challenger.bpm,
                score: challenger.score,
                confirmed_at_ms: challenger.observed_at_ms,
            });
            self.challenger = None;
        }
    }

    fn active(&self, now_ms: u64) -> Option<StableTempo> {
        self.stable.filter(|stable| {
            now_ms.saturating_sub(stable.confirmed_at_ms) <= STABLE_TEMPO_MAX_AGE_MS
        })
    }
}

/// Owns the non-real-time side of core audio analysis.
///
/// PCM is sent to a background worker. The worker independently searches
/// candidate two-bar window durations, runs the complete-loop estimator for
/// each candidate, then refines around the best score. The audio thread only
/// polls completed results and applies the explicit fusion policy below.
pub(crate) struct LoopTempoRuntime {
    capture: Arc<Mutex<CaptureBuffer>>,
    wake_worker: Sender<()>,
    output: Receiver<LoopTempoResult>,
    latest: Option<LoopTempoResult>,
    stabilizer: TempoStabilizer,
}

impl Default for LoopTempoRuntime {
    fn default() -> Self {
        let capture = Arc::new(Mutex::new(CaptureBuffer::default()));
        let (wake_tx, wake_rx) = crossbeam_channel::bounded(1);
        // At most one status/result pair can be waiting. If core stops polling,
        // backpressure pauses analysis instead of accumulating stale results.
        let (output_tx, output_rx) = crossbeam_channel::bounded(2);
        let worker_capture = Arc::clone(&capture);
        thread::Builder::new()
            .name("loop-tempo-analysis".to_string())
            .spawn(move || worker(worker_capture, wake_rx, output_tx))
            .expect("failed to spawn loop tempo analysis worker");
        Self {
            capture,
            wake_worker: wake_tx,
            output: output_rx,
            latest: None,
            stabilizer: TempoStabilizer::default(),
        }
    }
}

impl LoopTempoRuntime {
    pub(crate) fn update(
        &mut self,
        now_ms: u64,
        sample_rate: Option<u32>,
        samples: Vec<f32>,
        live_bpm: f32,
        live_confidence: f32,
        debug: &mut SignalDebugData,
    ) -> Option<FusedTempo> {
        if let Some(sample_rate) = sample_rate.filter(|_| !samples.is_empty()) {
            {
                let mut capture = self.capture.lock().unwrap();
                if capture.sample_rate != sample_rate {
                    capture.samples.clear();
                    capture.last_analysis_ms = None;
                    capture.sample_rate = sample_rate;
                }
                capture.now_ms = now_ms;
                capture.samples.extend(samples);
                let maximum = sample_rate as usize * MAX_BUFFER_SECONDS;
                if capture.samples.len() > maximum {
                    let excess = capture.samples.len() - maximum;
                    capture.samples.drain(..excess);
                }
            }
            match self.wake_worker.try_send(()) {
                Ok(()) | Err(TrySendError::Full(())) => {}
                Err(TrySendError::Disconnected(())) => {
                    self.latest = Some(LoopTempoResult {
                        status: LoopTempoStatus::WorkerStopped,
                        candidate_bpm: 0.0,
                        accepted_bpm: 0.0,
                        score: 0.0,
                        threshold: 0.0,
                        analyzed_at_ms: now_ms,
                        buffered_seconds: 0.0,
                    });
                }
            }
        }

        for result in self.output.try_iter() {
            self.stabilizer.observe(result);
            self.latest = Some(result);
        }

        let stable = self.stabilizer.active(now_ms);
        let fused = stable.and_then(|tempo| fuse_tempos(live_bpm, live_confidence, tempo));

        let mut loop_debug = self
            .latest
            .map(|result| result.debug(now_ms, fused.is_some()))
            .unwrap_or_default();
        loop_debug.accepted_bpm = stable.map_or(0.0, |tempo| tempo.bpm);
        debug.loop_tempo = loop_debug;
        fused
    }
}

fn worker(
    capture: Arc<Mutex<CaptureBuffer>>,
    wake_worker: Receiver<()>,
    output: Sender<LoopTempoResult>,
) {
    while wake_worker.recv().is_ok() {
        let (now_ms, sample_rate, samples) = {
            let mut capture = capture.lock().unwrap();
            let due = capture.last_analysis_ms.map_or(true, |last| {
                capture.now_ms.saturating_sub(last) >= ANALYSIS_INTERVAL_MS
            });
            if !due || capture.sample_rate == 0 {
                continue;
            }
            capture.last_analysis_ms = Some(capture.now_ms);
            (
                capture.now_ms,
                capture.sample_rate,
                capture.samples.iter().copied().collect::<Vec<_>>(),
            )
        };

        let buffered_seconds = samples.len() as f32 / sample_rate as f32;
        let required_seconds = window_seconds(MIN_BPM);
        if buffered_seconds < required_seconds {
            let _ = output.send(LoopTempoResult {
                status: LoopTempoStatus::WarmingUp,
                candidate_bpm: 0.0,
                accepted_bpm: 0.0,
                score: 0.0,
                threshold: 0.0,
                analyzed_at_ms: now_ms,
                buffered_seconds,
            });
            continue;
        }

        let _ = output.send(LoopTempoResult {
            status: LoopTempoStatus::Analyzing,
            candidate_bpm: 0.0,
            accepted_bpm: 0.0,
            score: 0.0,
            threshold: 0.0,
            analyzed_at_ms: now_ms,
            buffered_seconds,
        });
        let result = analyze_tempo_grid(&samples, sample_rate, now_ms);
        if output.send(result).is_err() {
            break;
        }
    }
}

fn analyze_tempo_grid(samples: &[f32], sample_rate: u32, now_ms: u64) -> LoopTempoResult {
    let (analysis_samples, analysis_rate) = downsample_for_analysis(samples, sample_rate);
    let estimator = LoopTempoEstimator::default();
    let mut best = None;

    for candidate in (MIN_BPM as usize..=MAX_BPM as usize).step_by(COARSE_STEP_BPM) {
        consider_candidate(
            &estimator,
            &analysis_samples,
            analysis_rate,
            candidate as f32,
            &mut best,
        );
    }

    if let Some(coarse) = best {
        let mut candidate = (coarse.window_bpm - REFINE_RADIUS_BPM).max(MIN_BPM);
        let end = (coarse.window_bpm + REFINE_RADIUS_BPM).min(MAX_BPM);
        while candidate <= end {
            consider_candidate(
                &estimator,
                &analysis_samples,
                analysis_rate,
                candidate,
                &mut best,
            );
            candidate += REFINE_STEP_BPM;
        }
    }

    let buffered_seconds = samples.len() as f32 / sample_rate as f32;
    match best {
        Some(best) => LoopTempoResult {
            status: if best.accepted_bpm > 0.0 {
                LoopTempoStatus::Accepted
            } else {
                LoopTempoStatus::Rejected
            },
            candidate_bpm: best.candidate_bpm,
            accepted_bpm: best.accepted_bpm,
            score: best.score,
            threshold: best.threshold,
            analyzed_at_ms: now_ms,
            buffered_seconds,
        },
        None => LoopTempoResult {
            status: LoopTempoStatus::Rejected,
            candidate_bpm: 0.0,
            accepted_bpm: 0.0,
            score: 0.0,
            threshold: 0.0,
            analyzed_at_ms: now_ms,
            buffered_seconds,
        },
    }
}

fn downsample_for_analysis(samples: &[f32], sample_rate: u32) -> (Vec<f32>, u32) {
    let factor = sample_rate.div_ceil(ANALYSIS_SAMPLE_RATE).max(1);
    let rate = sample_rate / factor;
    let sample_count = samples.len() / factor as usize;
    let samples = samples
        .iter()
        .step_by(factor as usize)
        .take(sample_count)
        .copied()
        .collect();
    (samples, rate)
}

#[derive(Clone, Copy)]
struct CandidateResult {
    window_bpm: f32,
    candidate_bpm: f32,
    accepted_bpm: f32,
    score: f32,
    selection_score: f32,
    threshold: f32,
}

fn consider_candidate(
    estimator: &LoopTempoEstimator,
    samples: &[f32],
    sample_rate: u32,
    window_bpm: f32,
    best: &mut Option<CandidateResult>,
) {
    let required = (window_seconds(window_bpm) * sample_rate as f32).round() as usize;
    if required == 0 || samples.len() < required {
        return;
    }
    let analysis = estimator.analyze(&samples[samples.len() - required..], sample_rate);
    let Some(score) = analysis.score.map(|score| score as f32) else {
        return;
    };
    let result = CandidateResult {
        window_bpm,
        candidate_bpm: analysis.candidate_bpm.unwrap_or(0.0) as f32,
        accepted_bpm: analysis.bpm.unwrap_or(0.0) as f32,
        score,
        selection_score: score
            - tempo_prior_penalty(analysis.candidate_bpm.unwrap_or(120.0) as f32),
        threshold: analysis.threshold as f32,
    };
    if best.map_or(true, |current| {
        result.selection_score > current.selection_score
    }) {
        *best = Some(result);
    }
}

fn tempo_prior_penalty(bpm: f32) -> f32 {
    // Resolve subdivision/octave hypotheses without overpowering a materially
    // better rhythmic fit. This is the same 120-BPM center used by the live
    // estimator and the reference algorithm's tempo prior.
    0.2 * (bpm / 120.0).ln().powi(2)
}

fn window_seconds(bpm: f32) -> f32 {
    BARS_PER_WINDOW * BEATS_PER_BAR * 60.0 / bpm
}

fn relative_difference(reference: f32, candidate: f32) -> f32 {
    (reference - candidate).abs() / reference.max(f32::EPSILON)
}

fn align_tempo(reference: f32, candidate: f32) -> f32 {
    [candidate / 2.0, candidate, candidate * 2.0]
        .into_iter()
        .min_by(|left, right| {
            (left - reference)
                .abs()
                .total_cmp(&(right - reference).abs())
        })
        .unwrap_or(candidate)
}

fn fuse_tempos(live_bpm: f32, live_confidence: f32, stable: StableTempo) -> Option<FusedTempo> {
    let loop_bpm = stable.bpm;
    if loop_bpm <= 0.0 || !loop_bpm.is_finite() {
        return None;
    }
    if live_bpm <= 0.0 || !live_bpm.is_finite() {
        return Some(FusedTempo {
            bpm: loop_bpm,
            confidence: stable.score.clamp(0.0, 1.0),
        });
    }

    let aligned_loop_bpm = align_tempo(live_bpm, loop_bpm);
    let disagreement = relative_difference(live_bpm, aligned_loop_bpm);
    if disagreement > MAX_TEMPO_DISAGREEMENT {
        return None;
    }

    let live_weight = live_confidence.clamp(0.0, 1.0).max(0.05);
    let loop_weight = stable.score.clamp(0.0, 1.0).max(0.05);
    let agreement = 1.0 - disagreement / MAX_TEMPO_DISAGREEMENT;
    Some(FusedTempo {
        // Once independently confirmed, the loop estimate is the tempo anchor.
        // The live estimator continues to provide onset/phase information.
        bpm: aligned_loop_bpm,
        confidence: ((live_weight + loop_weight) * 0.5 * agreement).clamp(0.0, 1.0),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn accepted_result(bpm: f32, score: f32, analyzed_at_ms: u64) -> LoopTempoResult {
        LoopTempoResult {
            status: LoopTempoStatus::Accepted,
            candidate_bpm: bpm,
            accepted_bpm: bpm,
            score,
            threshold: 0.8,
            analyzed_at_ms,
            buffered_seconds: 6.0,
        }
    }

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
    fn independent_grid_finds_click_tempo() {
        let sample_rate = 12_000;
        let samples = click_loop(126.0, 4, sample_rate);
        let result = analyze_tempo_grid(&samples, sample_rate, 10_000);
        assert_eq!(result.status, LoopTempoStatus::Accepted);
        assert!((result.accepted_bpm - 126.0).abs() < 1.0, "{result:?}");
    }

    #[test]
    fn fusion_combines_agreeing_independent_results() {
        let stable = StableTempo {
            bpm: 121.0,
            score: 0.9,
            confirmed_at_ms: 0,
        };
        let fused = fuse_tempos(120.0, 0.7, stable).unwrap();
        assert_eq!(fused.bpm, 121.0);
        assert!(fused.confidence > 0.7);
    }

    #[test]
    fn fusion_rejects_disagreement() {
        let stable = StableTempo {
            bpm: 150.0,
            score: 0.95,
            confirmed_at_ms: 0,
        };
        assert_eq!(fuse_tempos(120.0, 0.8, stable), None);
    }

    #[test]
    fn half_and_double_time_predictions_keep_the_established_tempo() {
        let mut stabilizer = TempoStabilizer::default();
        stabilizer.observe(accepted_result(120.0, 0.9, 1_000));
        stabilizer.observe(accepted_result(120.0, 0.9, 3_000));
        assert_eq!(stabilizer.active(3_000).unwrap().bpm, 120.0);

        stabilizer.observe(accepted_result(60.0, 0.9, 5_000));
        assert_eq!(stabilizer.active(5_000).unwrap().bpm, 120.0);

        stabilizer.observe(accepted_result(240.0, 0.9, 7_000));
        assert_eq!(stabilizer.active(7_000).unwrap().bpm, 120.0);
    }

    #[test]
    fn a_single_different_prediction_does_not_move_the_lock() {
        let mut stabilizer = TempoStabilizer::default();
        stabilizer.observe(accepted_result(120.0, 0.9, 1_000));
        stabilizer.observe(accepted_result(120.0, 0.9, 3_000));
        stabilizer.observe(accepted_result(132.0, 0.95, 5_000));

        assert_eq!(stabilizer.active(5_000).unwrap().bpm, 120.0);
    }
}
