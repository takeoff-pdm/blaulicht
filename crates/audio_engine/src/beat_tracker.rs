//! Real-time tempo and beat tracker working on raw PCM.
//!
//! The spectrum the collector receives is shaped for display (peak hold,
//! re-interpolated bins) and arrives whenever a capture block happens to land,
//! which blurs onset timing. This tracker instead runs its own short-time
//! Fourier transform on a fixed 10 ms hop, so every onset value sits on a
//! uniform clock with a known timestamp.
//!
//! Pipeline:
//! 1. Onset detection: log-magnitude spectral flux in a low band (kick and
//!    bass), a sweep band (where a kick's pitch sweep starts), a click band
//!    (claps, snares, hats) and the full band.
//! 2. Tempo: autocorrelation of the onset envelope over the last 8 s, scored on
//!    a BPM grid together with its two-beat harmonic, smoothed over time and
//!    switched only on sustained evidence.
//! 3. Phase: a comb over the last beats picks where beats fall. Club music
//!    often has a bass note on every off-beat and an open hat between kicks,
//!    so low-band onsets and transients are both ambiguous by half a beat.
//!    The comb instead follows onsets in the sweep band (a kick starts around
//!    150..500 Hz and falls; the bassline sits lower, hats higher), plus how
//!    much more the click band hits on even than on odd beats: a clap or snare
//!    on beats 2 and 4 shows up there, while off-beat hats hit both equally.
//! 4. Beats are emitted from the prediction, so there is no detection latency.
//!
//! With [`BeatDetection::Model`], step 1 is replaced by the learned beat model
//! (`audio-model/`, compiled in from `models/beat_model.bin`): its beat
//! activation drives both tempo and phase. It already tells beats from
//! off-beats, so the click-band asymmetry is not used.

use std::{collections::VecDeque, sync::Arc};

use blaulicht_audio_features::{BeatActivation, BeatModel, MelFrame, MelFrontend, FRAME_RATE};
use rustfft::{num_complex::Complex, Fft, FftPlanner};
use serde::{Deserialize, Serialize};

use crate::BpmDetectStatus;

/// Trained by `audio-model/train.py`, exported by `audio-model/export_model.py`.
static BEAT_MODEL: &[u8] = include_bytes!("../models/beat_model.bin");

/// How beats and tempo are detected; selected by `beat_detection` in
/// config.toml.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum BeatDetection {
    /// Spectrum-only estimator in `signals.rs`; the PCM tracker stays idle.
    Legacy,
    /// PCM tracker with hand-tuned spectral-flux onsets.
    #[default]
    Tracker,
    /// PCM tracker driven by the learned beat model.
    Model,
}

const HOP_MS: f64 = 10.0;
/// Analysis window length; long enough for ~20 Hz bins in the kick band.
const WINDOW_MS: f64 = 40.0;
const LOW_BAND_HZ: (f32, f32) = (30.0, 150.0);
const CLICK_BAND_HZ: (f32, f32) = (1_000.0, 5_000.0);
/// Where a kick's pitch sweep starts: above the bassline, below hats.
const SWEEP_BAND_HZ: (f32, f32) = (150.0, 500.0);
const FULL_BAND_MAX_HZ: f32 = 8_000.0;
/// Magnitudes are normalized to sine amplitude; this sets where log
/// compression starts.
const LOG_COMPRESSION: f32 = 1_000.0;
/// Time constant of the per-band running mean used to normalize onsets.
const NORM_ALPHA: f32 = 1.0 / 400.0;

const HISTORY_FRAMES: usize = 800;
const MIN_TEMPO_FRAMES: usize = 300;
pub const MIN_BPM: f32 = 80.0;
pub const MAX_BPM: f32 = 200.0;
const BPM_STEP: f32 = 0.25;
const TEMPO_UPDATE_FRAMES: usize = 5;
/// Per-update blend of a new salience curve into the running one.
const SALIENCE_ALPHA: f32 = 0.2;
/// Log-normal tempo prior (center, width in octaves). It breaks the tie
/// between a tempo and its half when claps or snares make the two-beat lag as
/// periodic as the beat itself (e.g. 87 vs 174 BPM), toward club tempi.
const PRIOR_CENTER_BPM: f32 = 130.0;
const PRIOR_OCTAVES: f32 = 0.5;
/// A different tempo must beat the current one by this factor...
const SWITCH_MARGIN: f32 = 1.1;
/// ...for this long before the tracker switches to it.
const SWITCH_HOLD_MS: f64 = 1_500.0;
/// While locked, the tempo follows the local peak within this relative range.
const TRACK_RANGE: f32 = 0.03;
/// Normalized autocorrelation below which the tempo is reported as weak.
const MIN_CONFIDENCE: f32 = 0.1;

/// Beats used by the phase comb, and their recency weighting.
const COMB_BEATS: usize = 8;
const COMB_DECAY: f32 = 0.85;
/// Weight of the even/odd click asymmetry relative to the sweep comb.
const CLAP_WEIGHT: f32 = 2.0;
/// Fraction of the measured phase error applied per beat.
const PHASE_GAIN: f64 = 0.3;
/// Phase errors larger than this fraction of a beat are a different phase
/// hypothesis rather than jitter.
const PHASE_JUMP_FRACTION: f64 = 0.2;
/// Beats a new phase hypothesis must persist before the tracker jumps to it.
const PHASE_JUMP_BEATS: u32 = 4;
/// Onsets that end within this time of the newest frame are not complete yet.
const COMB_GUARD_MS: f64 = 30.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TempoEstimate {
    pub bpm: f32,
    /// Normalized autocorrelation at the beat lag, 0..1.
    pub confidence: f32,
}

struct Analysis {
    sample_rate: u32,
    hop: usize,
    window: Vec<f32>,
    fft: Arc<dyn Fft<f32>>,
    buffer: Vec<Complex<f32>>,
    low_bins: (usize, usize),
    click_bins: (usize, usize),
    sweep_bins: (usize, usize),
    full_bins: usize,
    /// Samples not yet consumed by a full window.
    pending: VecDeque<f32>,
    prev_log_mag: Vec<f32>,
    band_mean: [f32; 4],
}

impl Analysis {
    fn new(sample_rate: u32) -> Self {
        let hop = ((sample_rate as f64 * HOP_MS / 1000.0).round() as usize).max(1);
        let size = ((sample_rate as f64 * WINDOW_MS / 1000.0) as usize)
            .next_power_of_two()
            .max(64);
        let hann: Vec<f32> = (0..size)
            .map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / size as f32).cos())
            .collect();
        // Scale so a full-scale sine yields magnitude ~1.
        let gain = 2.0 / hann.iter().sum::<f32>();
        let window = hann.iter().map(|w| w * gain).collect();
        let bin_hz = sample_rate as f32 / size as f32;
        let bin = |hz: f32| ((hz / bin_hz).round() as usize).clamp(1, size / 2);
        Self {
            sample_rate,
            hop,
            window,
            fft: FftPlanner::new().plan_fft_forward(size),
            buffer: vec![Complex::default(); size],
            low_bins: (bin(LOW_BAND_HZ.0), bin(LOW_BAND_HZ.1)),
            click_bins: (bin(CLICK_BAND_HZ.0), bin(CLICK_BAND_HZ.1)),
            sweep_bins: (bin(SWEEP_BAND_HZ.0), bin(SWEEP_BAND_HZ.1)),
            full_bins: bin(FULL_BAND_MAX_HZ),
            pending: VecDeque::with_capacity(size * 2),
            prev_log_mag: vec![0.0; size / 2 + 1],
            band_mean: [0.0; 4],
        }
    }

    fn size(&self) -> usize {
        self.buffer.len()
    }

    /// Onset strengths (low, click, full, sweep) normalized by their running means,
    /// plus the frame's raw energy, for the next complete window.
    fn next_frame(&mut self) -> Option<([f32; 4], f32)> {
        let size = self.size();
        if self.pending.len() < size {
            return None;
        }
        let mut energy = 0.0;
        for (i, (slot, sample)) in self.buffer.iter_mut().zip(&self.pending).enumerate() {
            *slot = Complex::new(sample * self.window[i], 0.0);
            energy += sample * sample;
        }
        self.pending.drain(..self.hop);
        self.fft.process(&mut self.buffer);

        let mut flux = [0.0f32; 4];
        for bin in 1..=self.full_bins {
            let log_mag = (1.0 + LOG_COMPRESSION * self.buffer[bin].norm()).ln();
            let rise = (log_mag - self.prev_log_mag[bin]).max(0.0);
            self.prev_log_mag[bin] = log_mag;
            if (self.low_bins.0..=self.low_bins.1).contains(&bin) {
                flux[0] += rise;
            }
            if (self.click_bins.0..=self.click_bins.1).contains(&bin) {
                flux[1] += rise;
            }
            if (self.sweep_bins.0..=self.sweep_bins.1).contains(&bin) {
                flux[3] += rise;
            }
            flux[2] += rise;
        }

        let mut normalized = [0.0f32; 4];
        for band in 0..4 {
            let mean = &mut self.band_mean[band];
            *mean = if *mean <= 0.0 {
                flux[band]
            } else {
                *mean + NORM_ALPHA * (flux[band] - *mean)
            };
            normalized[band] = flux[band] / (*mean + 1e-3);
        }
        Some((normalized, energy / size as f32))
    }
}

/// One onset frame for the tracker: onset strengths (low, click, full, sweep),
/// mean signal energy, and the stream time (ms) the onsets belong to.
type OnsetFrame = ([f32; 4], f32, f64);

/// The learned beat model on the shared mel front end.
struct ModelOnsets {
    sample_rate: u32,
    frontend: MelFrontend,
    model: BeatModel,
    mels: Vec<MelFrame>,
    frames_done: u64,
    /// Newest activation, and the strongest one since the last debug
    /// snapshot (so a short beat peak is not missed between snapshots).
    last: BeatActivation,
    peak: BeatActivation,
    /// Rising edges of the activations over the model's own thresholds.
    beats: u64,
    downbeats: u64,
    above: (bool, bool),
}

impl ModelOnsets {
    fn new(sample_rate: u32) -> Result<Self, String> {
        Ok(Self {
            sample_rate,
            frontend: MelFrontend::new(sample_rate)?,
            model: BeatModel::from_bytes(BEAT_MODEL)?,
            mels: Vec::new(),
            frames_done: 0,
            last: BeatActivation::default(),
            peak: BeatActivation::default(),
            beats: 0,
            downbeats: 0,
            above: (false, false),
        })
    }

    fn push(&mut self, samples: &[f32], out: &mut Vec<OnsetFrame>) {
        let energy = samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32;
        self.mels.clear();
        self.frontend.push(samples, &mut self.mels);
        for mel in &self.mels {
            let activation = self.model.step(mel);
            let beat = activation.beat;
            self.last = activation;
            self.peak.beat = self.peak.beat.max(activation.beat);
            self.peak.downbeat = self.peak.downbeat.max(activation.downbeat);
            let info = self.model.info();
            let above = (
                activation.beat >= info.beat_threshold,
                activation.downbeat >= info.downbeat_threshold,
            );
            self.beats += u64::from(above.0 && !self.above.0);
            self.downbeats += u64::from(above.1 && !self.above.1);
            self.above = above;
            // The model marks a beat in the first frame it is audible in, so
            // the beat lies within that frame; use its middle.
            let time_ms = (self.frames_done as f64 + 0.5) * 1000.0 / FRAME_RATE as f64;
            self.frames_done += 1;
            // Beat activation feeds both the tempo (low + full) and the phase
            // comb (sweep); no click band.
            out.push(([beat, 0.0, 0.0, beat], energy, time_ms));
        }
    }
}

enum FrontEnd {
    Dsp(Analysis),
    Model(Box<ModelOnsets>),
}

impl FrontEnd {
    fn new(sample_rate: u32, use_model: bool) -> Self {
        if use_model {
            match ModelOnsets::new(sample_rate) {
                Ok(model) => {
                    log::info!("beat tracker: learned beat model at {sample_rate} Hz");
                    return Self::Model(Box::new(model));
                }
                Err(err) => {
                    log::warn!("beat tracker: beat model unavailable ({err}); using spectral flux")
                }
            }
        }
        Self::Dsp(Analysis::new(sample_rate))
    }

    fn sample_rate(&self) -> u32 {
        match self {
            Self::Dsp(analysis) => analysis.sample_rate,
            Self::Model(model) => model.sample_rate,
        }
    }

    /// Consume `samples`; `samples_total` already includes them.
    fn push(&mut self, samples: &[f32], samples_total: u64, out: &mut Vec<OnsetFrame>) {
        match self {
            Self::Dsp(analysis) => analysis.push(samples, samples_total, out),
            Self::Model(model) => model.push(samples, out),
        }
    }
}

impl Analysis {
    fn push(&mut self, samples: &[f32], samples_total: u64, out: &mut Vec<OnsetFrame>) {
        self.pending.extend(samples.iter().copied());
        let size = self.size();
        let rate = self.sample_rate as f64;
        // Stream time of the first pending sample.
        let pending_start_ms = (samples_total - self.pending.len() as u64) as f64 * 1000.0 / rate;
        let mut frame_index = 0usize;
        while let Some((onsets, energy)) = self.next_frame() {
            let center = pending_start_ms
                + (frame_index * self.hop) as f64 * 1000.0 / rate
                + size as f64 * 500.0 / rate;
            out.push((onsets, energy, center));
            frame_index += 1;
        }
    }
}

#[derive(Default)]
struct Frames {
    /// Sweep-band onsets: the beat-phase cue.
    kick: VecDeque<f32>,
    click: VecDeque<f32>,
    /// Tempo onset envelope (low + full).
    tempo: VecDeque<f32>,
    /// Stream time (ms) of the newest frame's window center.
    newest_ms: f64,
}

impl Frames {
    fn push(&mut self, onsets: [f32; 4], time_ms: f64) {
        for (history, value) in [
            (&mut self.kick, onsets[3]),
            (&mut self.click, onsets[1]),
            (&mut self.tempo, onsets[0] + onsets[2]),
        ] {
            history.push_back(value);
            if history.len() > HISTORY_FRAMES {
                history.pop_front();
            }
        }
        self.newest_ms = time_ms;
    }

    fn len(&self) -> usize {
        self.tempo.len()
    }

    /// Strongest value of `series` within one frame of stream time `t`.
    fn around(&self, series: &VecDeque<f32>, t: f64) -> Option<f32> {
        let back = (self.newest_ms - t) / HOP_MS;
        if back < 0.0 {
            return None;
        }
        let center = series.len() as isize - 1 - back.round() as isize;
        if center < 1 || center as usize + 1 >= series.len() {
            return None;
        }
        let c = center as usize;
        Some(series[c - 1].max(series[c]).max(series[c + 1]))
    }
}

struct TempoState {
    salience: Vec<f32>,
    bpm: f32,
    confidence: f32,
    challenger: Option<(f32, f64)>,
}

struct PhaseState {
    /// Stream time of the next beat to emit.
    next_beat_ms: f64,
    /// Candidate phase that disagrees with the current one, and for how many
    /// consecutive beats it has.
    challenger: Option<(f64, u32)>,
}

/// Which onset front end the PCM tracker is running.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub enum BeatFrontEnd {
    /// No PCM received yet (or legacy mode, where the tracker stays idle).
    #[default]
    Idle,
    SpectralFlux,
    Model,
}

/// The learned beat model's state, for the audio info page.
#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct BeatModelDebugData {
    /// Newest beat / downbeat activation, 0..1.
    pub beat: f32,
    pub downbeat: f32,
    /// Strongest activation since the previous snapshot, 0..1.
    pub beat_peak: f32,
    pub downbeat_peak: f32,
    /// Peak-picking thresholds the model was exported with.
    pub beat_threshold: f32,
    pub downbeat_threshold: f32,
    /// Activation crossings over the thresholds since the model started.
    pub beats: u64,
    pub downbeats: u64,
    /// Mel frames run through the model.
    pub frames: u64,
    pub sample_rate: u32,
    pub channels: u32,
    pub blocks: u32,
    pub receptive_field_frames: u32,
    pub lead_ms: f32,
}

/// PCM beat tracker state, for the audio info page.
#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct BeatTrackerDebugData {
    /// Mode configured by `beat_detection` in config.toml.
    pub mode: BeatDetection,
    /// Front end actually running (the model falls back to spectral flux when
    /// it fails to load).
    pub front_end: BeatFrontEnd,
    pub manual_bpm: bool,
    /// Onset frames in the tempo history, and how many tempo needs.
    pub history_frames: u32,
    pub history_needed: u32,
    /// Tracked tempo (0 = none) and its autocorrelation confidence.
    pub tempo_bpm: f32,
    pub tempo_confidence: f32,
    /// Tempo that currently outscores the locked one (0 = none), and for how
    /// long it has (the switch happens after `switch_hold_ms`).
    pub challenger_bpm: f32,
    pub challenger_held_ms: f32,
    pub switch_hold_ms: f32,
    pub phase_locked: bool,
    /// Beats a competing phase has persisted (the jump happens at
    /// `phase_jump_beats`).
    pub phase_challenger_beats: u32,
    pub phase_jump_beats: u32,
    /// Mainloop time until the next predicted beat (negative = overdue).
    pub next_beat_in_ms: f32,
    /// Estimated `mainloop_ms - stream_ms`.
    pub clock_offset_ms: f32,
    /// Set when the learned model is running.
    pub model: Option<BeatModelDebugData>,
}

pub struct BeatTracker {
    front_end: Option<FrontEnd>,
    /// Use the learned beat model for onsets (see [`BeatDetection::Model`]).
    use_model: bool,
    frames: Frames,
    /// Total samples received; defines stream time.
    samples_total: u64,
    /// Estimated `mainloop_ms - stream_ms` for samples as they arrive.
    clock_offset_ms: Option<f64>,
    tempo: Option<TempoState>,
    phase: Option<PhaseState>,
    status: BpmDetectStatus,
    frames_since_tempo_update: usize,
    manual_bpm: Option<f32>,
    onset_flag: bool,
    onset_stats: (f32, f32),
    last_peak_frame: Option<f32>,
    last_phase_update_ms: Option<f64>,
}

impl Default for BeatTracker {
    fn default() -> Self {
        Self {
            front_end: None,
            use_model: false,
            frames: Frames::default(),
            samples_total: 0,
            clock_offset_ms: None,
            tempo: None,
            phase: None,
            status: BpmDetectStatus::default(),
            frames_since_tempo_update: 0,
            manual_bpm: None,
            onset_flag: false,
            onset_stats: (0.0, 0.0),
            last_peak_frame: None,
            last_phase_update_ms: None,
        }
    }
}

impl BeatTracker {
    pub fn reset(&mut self) {
        let (manual_bpm, use_model) = (self.manual_bpm, self.use_model);
        *self = Self::default();
        self.manual_bpm = manual_bpm;
        self.use_model = use_model;
    }

    /// Detect onsets with the learned beat model instead of spectral flux.
    /// Changing this restarts tracking.
    pub fn set_use_model(&mut self, use_model: bool) {
        if use_model != self.use_model {
            self.use_model = use_model;
            self.reset();
        }
    }

    /// Whether the learned model is producing the onsets right now (false
    /// before PCM arrives, or when it fell back to spectral flux).
    pub fn uses_model(&self) -> bool {
        matches!(self.front_end, Some(FrontEnd::Model(_)))
    }

    /// Whether PCM has been received, i.e. whether this tracker is in charge.
    pub fn is_active(&self) -> bool {
        self.front_end.is_some()
    }

    pub fn status(&self) -> BpmDetectStatus {
        self.status
    }

    /// Snapshot of the tracker's internals for the UI. Resets the model's
    /// "peak since last snapshot" activations.
    pub fn debug_data(&mut self, mode: BeatDetection, now_ms: f64) -> BeatTrackerDebugData {
        let next_beat_in_ms = self
            .next_beat_ms()
            .map_or(0.0, |next| (next - now_ms) as f32);
        let newest = self.frames.newest_ms;
        let (challenger_bpm, challenger_held_ms) = self
            .tempo
            .as_ref()
            .and_then(|tempo| tempo.challenger)
            .map_or((0.0, 0.0), |(bpm, since)| (bpm, (newest - since) as f32));
        let front_end = match &self.front_end {
            None => BeatFrontEnd::Idle,
            Some(FrontEnd::Dsp(_)) => BeatFrontEnd::SpectralFlux,
            Some(FrontEnd::Model(_)) => BeatFrontEnd::Model,
        };
        let model = match &mut self.front_end {
            Some(FrontEnd::Model(model)) => {
                let info = model.model.info();
                let receptive_field_frames = 1 + info
                    .dilations
                    .iter()
                    .map(|d| d * (info.kernel.saturating_sub(1)))
                    .sum::<usize>();
                let data = BeatModelDebugData {
                    beat: model.last.beat,
                    downbeat: model.last.downbeat,
                    beat_peak: model.peak.beat,
                    downbeat_peak: model.peak.downbeat,
                    beat_threshold: info.beat_threshold,
                    downbeat_threshold: info.downbeat_threshold,
                    beats: model.beats,
                    downbeats: model.downbeats,
                    frames: model.frames_done,
                    sample_rate: model.sample_rate,
                    channels: info.channels as u32,
                    blocks: info.dilations.len() as u32,
                    receptive_field_frames: receptive_field_frames as u32,
                    lead_ms: info.lead_ms,
                };
                model.peak = model.last;
                Some(data)
            }
            _ => None,
        };
        BeatTrackerDebugData {
            mode,
            front_end,
            manual_bpm: self.manual_bpm.is_some(),
            history_frames: self.frames.len() as u32,
            history_needed: MIN_TEMPO_FRAMES as u32,
            tempo_bpm: self.tempo.as_ref().map_or(0.0, |tempo| tempo.bpm),
            tempo_confidence: self.tempo.as_ref().map_or(0.0, |tempo| tempo.confidence),
            challenger_bpm,
            challenger_held_ms,
            switch_hold_ms: SWITCH_HOLD_MS as f32,
            phase_locked: self.phase.is_some(),
            phase_challenger_beats: self
                .phase
                .as_ref()
                .and_then(|phase| phase.challenger)
                .map_or(0, |(_, beats)| beats),
            phase_jump_beats: PHASE_JUMP_BEATS,
            next_beat_in_ms,
            clock_offset_ms: self.clock_offset_ms.unwrap_or(0.0) as f32,
            model,
        }
    }

    pub fn tempo(&self) -> Option<TempoEstimate> {
        if let Some(bpm) = self.manual_bpm {
            return Some(TempoEstimate {
                bpm,
                confidence: 1.0,
            });
        }
        self.tempo.as_ref().map(|tempo| TempoEstimate {
            bpm: tempo.bpm,
            confidence: tempo.confidence,
        })
    }

    /// Pin the tempo (`Some`) or return to detection (`None`).
    pub fn set_manual_bpm(&mut self, bpm: Option<f32>) {
        self.manual_bpm = bpm.filter(|bpm| bpm.is_finite() && *bpm > 0.0);
    }

    /// Nudge the tempo toward an external estimate when it is close to the
    /// current one. The next autocorrelation update (every 50 ms) re-derives
    /// the tempo, so a far-off or stale hint cannot break a good lock.
    pub fn apply_tempo_hint(&mut self, bpm: f32) {
        if let Some(tempo) = &mut self.tempo {
            if (bpm - tempo.bpm).abs() / tempo.bpm <= TRACK_RANGE {
                tempo.bpm = bpm;
            }
        }
    }

    /// Whether a strong onset was detected since the last call.
    pub fn take_onset(&mut self) -> bool {
        std::mem::take(&mut self.onset_flag)
    }

    /// Feed newly captured mono PCM. `now_ms` is the mainloop clock at which
    /// the newest sample became available.
    pub fn push(&mut self, samples: &[f32], sample_rate: u32, now_ms: f64) {
        if samples.is_empty() || sample_rate == 0 {
            return;
        }
        if self.front_end.as_ref().map(FrontEnd::sample_rate) != Some(sample_rate) {
            self.reset();
            self.front_end = Some(FrontEnd::new(sample_rate, self.use_model));
        }

        self.samples_total += samples.len() as u64;
        let stream_end_ms = self.samples_total as f64 * 1000.0 / sample_rate as f64;
        // Track the least-delayed arrival: blocks are only ever late, never
        // early, so the minimum offset is the true one. Relax slowly so clock
        // drift is followed, and resync after a gap.
        let measured = now_ms - stream_end_ms;
        self.clock_offset_ms = Some(match self.clock_offset_ms {
            Some(offset) if (measured - offset).abs() < 250.0 => {
                if measured < offset {
                    measured
                } else {
                    offset + 0.002 * (measured - offset)
                }
            }
            _ => measured,
        });

        let mut new_frames = Vec::new();
        self.front_end
            .as_mut()
            .unwrap()
            .push(samples, self.samples_total, &mut new_frames);
        for (onsets, energy, center) in new_frames {
            self.frames.push(onsets, center);
            self.detect_onset(onsets);
            self.frames_since_tempo_update += 1;
            if self.frames_since_tempo_update >= TEMPO_UPDATE_FRAMES {
                self.frames_since_tempo_update = 0;
                self.update_tempo(energy);
            }
            self.update_phase();
        }
    }

    /// Mainloop time of the next predicted beat.
    pub fn next_beat_ms(&self) -> Option<f64> {
        let phase = self.phase.as_ref()?;
        Some(phase.next_beat_ms + self.clock_offset_ms?)
    }

    pub fn period_ms(&self) -> Option<f64> {
        self.tempo().map(|tempo| 60_000.0 / tempo.bpm as f64)
    }

    /// Returns true when a predicted beat is due at `now_ms` (at most once per
    /// beat; after a stall it skips to the most recent beat instead of
    /// replaying a backlog).
    pub fn poll_beat(&mut self, now_ms: f64) -> bool {
        let (Some(period), Some(offset)) = (self.period_ms(), self.clock_offset_ms) else {
            return false;
        };
        let Some(phase) = &mut self.phase else {
            return false;
        };
        let due = phase.next_beat_ms + offset;
        if now_ms < due {
            return false;
        }
        let skipped = ((now_ms - due) / period).floor();
        phase.next_beat_ms += (skipped + 1.0) * period;
        true
    }

    fn detect_onset(&mut self, onsets: [f32; 4]) {
        // Peak picking on the beat-phase signal against a running mean and
        // deviation, for the onset markers in the UI.
        let value = onsets[3] + onsets[1];
        let (mean, var) = &mut self.onset_stats;
        let deviation = var.sqrt();
        let is_peak =
            value > *mean + 2.0 * deviation && self.last_peak_frame.is_none_or(|last| value > last);
        let falling = self.last_peak_frame.is_some_and(|last| value < last);
        if falling {
            self.onset_flag = true;
            self.last_peak_frame = None;
        } else if is_peak {
            self.last_peak_frame = Some(value);
        }
        let alpha = 0.01;
        let diff = value - *mean;
        *mean += alpha * diff;
        *var = (1.0 - alpha) * (*var + alpha * diff * diff);
    }

    fn update_tempo(&mut self, frame_energy: f32) {
        let n = self.frames.len();
        if frame_energy <= 1e-10 && self.tempo.is_none() {
            self.status = BpmDetectStatus::NoEnergy;
            return;
        }
        if n < MIN_TEMPO_FRAMES {
            self.status = BpmDetectStatus::Warmup {
                have: n,
                need: MIN_TEMPO_FRAMES,
            };
            return;
        }

        let history = self.frames.tempo.make_contiguous();
        let mean = history.iter().sum::<f32>() / n as f32;
        let variance = history.iter().map(|v| (v - mean) * (v - mean)).sum::<f32>() / n as f32;
        if !variance.is_finite() || variance <= 1e-9 {
            self.status = BpmDetectStatus::FlatOnset;
            return;
        }

        // Normalized autocorrelation up to twice the slowest beat lag.
        let max_lag = ((60_000.0 / MIN_BPM as f64 / HOP_MS) * 2.0).ceil() as usize + 1;
        let max_lag = max_lag.min(n - 1);
        let mut acf = vec![0.0f32; max_lag + 1];
        for (lag, slot) in acf.iter_mut().enumerate().skip(1) {
            let pairs = n - lag;
            let mut sum = 0.0;
            for i in 0..pairs {
                sum += (history[i] - mean) * (history[i + lag] - mean);
            }
            *slot = sum / (pairs as f32 * variance);
        }
        let acf_at = |lag: f32| -> f32 {
            let i = lag.floor() as usize;
            if i + 1 >= acf.len() {
                return 0.0;
            }
            let t = lag - i as f32;
            acf[i] * (1.0 - t) + acf[i + 1] * t
        };

        let steps = ((MAX_BPM - MIN_BPM) / BPM_STEP).round() as usize + 1;
        let bpm_at = |i: usize| MIN_BPM + i as f32 * BPM_STEP;
        let fresh: Vec<f32> = (0..steps)
            .map(|i| {
                let bpm = bpm_at(i);
                let lag = 60_000.0 / bpm / HOP_MS as f32;
                let score = acf_at(lag) + 0.5 * acf_at(2.0 * lag);
                let octaves = (bpm / PRIOR_CENTER_BPM).log2() / PRIOR_OCTAVES;
                score * (-0.5 * octaves * octaves).exp()
            })
            .collect();

        let now = self.frames.newest_ms;
        let state = self.tempo.get_or_insert_with(|| TempoState {
            salience: fresh.clone(),
            bpm: 0.0,
            confidence: 0.0,
            challenger: None,
        });
        for (smoothed, value) in state.salience.iter_mut().zip(&fresh) {
            *smoothed += SALIENCE_ALPHA * (value - *smoothed);
        }

        let salience = &state.salience;
        let peak_in = |lo: usize, hi: usize| -> usize {
            (lo..=hi)
                .max_by(|a, b| salience[*a].total_cmp(&salience[*b]))
                .unwrap_or(lo)
        };
        let refine = |i: usize| -> f32 {
            if i == 0 || i + 1 >= salience.len() {
                return bpm_at(i);
            }
            let (ym, y0, yp) = (salience[i - 1], salience[i], salience[i + 1]);
            let denom = ym - 2.0 * y0 + yp;
            let delta = if denom.abs() > 1e-9 {
                (0.5 * (ym - yp) / denom).clamp(-0.5, 0.5)
            } else {
                0.0
            };
            bpm_at(i) + delta * BPM_STEP
        };
        let index_of =
            |bpm: f32| (((bpm - MIN_BPM) / BPM_STEP).round().max(0.0) as usize).min(steps - 1);

        let global = peak_in(0, steps - 1);
        let chosen = if state.bpm <= 0.0 {
            global
        } else {
            let lo = index_of(state.bpm * (1.0 - TRACK_RANGE));
            let hi = index_of(state.bpm * (1.0 + TRACK_RANGE));
            let local = peak_in(lo, hi);
            if (lo..=hi).contains(&global)
                || salience[global] <= SWITCH_MARGIN * salience[local].max(0.0)
            {
                state.challenger = None;
                local
            } else {
                let candidate = bpm_at(global);
                match state.challenger {
                    Some((bpm, since))
                        if (bpm - candidate).abs() / candidate <= TRACK_RANGE
                            && now - since >= SWITCH_HOLD_MS =>
                    {
                        state.challenger = None;
                        global
                    }
                    Some((bpm, _)) if (bpm - candidate).abs() / candidate <= TRACK_RANGE => local,
                    _ => {
                        state.challenger = Some((candidate, now));
                        local
                    }
                }
            }
        };

        let confidence = acf_at(60_000.0 / bpm_at(chosen) / HOP_MS as f32).clamp(0.0, 1.0);
        if confidence < MIN_CONFIDENCE && state.bpm <= 0.0 {
            self.status = BpmDetectStatus::WeakPeriodicity {
                strength: confidence,
                threshold: MIN_CONFIDENCE,
            };
            self.tempo = None;
            return;
        }
        let new_bpm = refine(chosen);
        if (new_bpm - state.bpm).abs() / new_bpm.max(1.0) > TRACK_RANGE {
            // A tempo switch invalidates the phase lock.
            self.phase = None;
        }
        state.bpm = new_bpm;
        state.confidence = confidence;
        self.status = if confidence < MIN_CONFIDENCE {
            BpmDetectStatus::WeakPeriodicity {
                strength: confidence,
                threshold: MIN_CONFIDENCE,
            }
        } else {
            BpmDetectStatus::Detecting
        };
    }

    /// Score how well beats placed `back_ms` before the newest frame (and
    /// every period before that) line up with kick onsets and with a two-beat
    /// clap pattern.
    fn comb_score(&self, back_ms: f64, period: f64) -> Option<f32> {
        let newest = self.frames.newest_ms;
        let mut kick = 0.0;
        let mut clap = [0.0f32; 2];
        let mut weight = 1.0;
        let mut used = 0;
        for k in 0..COMB_BEATS {
            let t = newest - back_ms - k as f64 * period;
            let (Some(onset), Some(c)) = (
                self.frames.around(&self.frames.kick, t),
                self.frames.around(&self.frames.click, t),
            ) else {
                break;
            };
            kick += weight * onset;
            clap[k % 2] += weight * c;
            weight *= COMB_DECAY;
            used += 1;
        }
        (used >= 2).then(|| kick + CLAP_WEIGHT * (clap[0] - clap[1]).abs())
    }

    fn update_phase(&mut self) {
        let Some(period) = self.period_ms() else {
            self.phase = None;
            return;
        };
        if self.frames.len() < MIN_TEMPO_FRAMES {
            return;
        }

        // Re-estimate once per beat; the comb sees every phase at once, so
        // this needs no alignment with the predicted beats.
        let newest = self.frames.newest_ms;
        if self.phase.is_some()
            && self
                .last_phase_update_ms
                .is_some_and(|last| newest - last < period)
        {
            return;
        }
        self.last_phase_update_ms = Some(newest);

        // Candidate "most recent beat" times, one per frame across a period.
        let steps = (period / HOP_MS).ceil() as usize;
        let mut scores = Vec::with_capacity(steps);
        for step in 0..steps {
            let back = COMB_GUARD_MS + step as f64 * HOP_MS;
            if let Some(score) = self.comb_score(back, period) {
                scores.push((back, score));
            }
        }
        let Some(&(best_back, best_score)) = scores.iter().max_by(|a, b| a.1.total_cmp(&b.1))
        else {
            return;
        };
        let mean = scores.iter().map(|s| s.1).sum::<f32>() / scores.len() as f32;
        if best_score <= mean * 1.2 {
            // No clear pulse to lock to; keep predicting.
            return;
        }
        // Sub-frame refinement.
        let refined_back = {
            let i = scores.iter().position(|s| s.0 == best_back).unwrap();
            if i > 0 && i + 1 < scores.len() {
                let (ym, y0, yp) = (scores[i - 1].1, scores[i].1, scores[i + 1].1);
                let denom = ym - 2.0 * y0 + yp;
                if denom.abs() > 1e-9 {
                    best_back + (0.5 * (ym - yp) / denom).clamp(-0.5, 0.5) as f64 * HOP_MS
                } else {
                    best_back
                }
            } else {
                best_back
            }
        };
        let observed_beat = newest - refined_back;

        match &mut self.phase {
            None => {
                let ahead = ((newest - observed_beat) / period).floor() + 1.0;
                self.phase = Some(PhaseState {
                    next_beat_ms: observed_beat + ahead * period,
                    challenger: None,
                });
            }
            Some(phase) => {
                let predicted = phase.next_beat_ms - period;
                let mut error = observed_beat - predicted;
                error -= (error / period).round() * period;
                if error.abs() <= PHASE_JUMP_FRACTION * period {
                    phase.next_beat_ms += PHASE_GAIN * error;
                    phase.challenger = None;
                } else {
                    let count = match phase.challenger {
                        Some((previous, count))
                            if {
                                let mut d = error - previous;
                                d -= (d / period).round() * period;
                                d.abs() <= PHASE_JUMP_FRACTION * period
                            } =>
                        {
                            count + 1
                        }
                        _ => 1,
                    };
                    if count >= PHASE_JUMP_BEATS {
                        phase.next_beat_ms += error;
                        phase.challenger = None;
                    } else {
                        phase.challenger = Some((error, count));
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 44_100;

    /// Kick (pitch sweep from ~400 Hz down to 50 Hz, with a click) on every
    /// beat, a bass note with harmonics on every off-beat, a clap on beats 2
    /// and 4, starting at `first_beat_s`.
    fn club_loop(bpm: f32, seconds: f32, first_beat_s: f32, offbeat_bass: bool) -> Vec<f32> {
        let mut out = vec![0.0f32; (seconds * RATE as f32) as usize];
        let period = 60.0 / bpm;
        let mut beat = 0usize;
        let mut add = |start: f32, len: f32, f: &dyn Fn(f32) -> f32| {
            let s = (start * RATE as f32) as usize;
            let n = (len * RATE as f32) as usize;
            for i in 0..n {
                if let Some(slot) = out.get_mut(s + i) {
                    *slot += f(i as f32 / RATE as f32);
                }
            }
        };
        let mut seed = 1u32;
        let mut noise = move || {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (seed >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0
        };
        let clap_noise: Vec<f32> = (0..(0.05 * RATE as f32) as usize)
            .map(|_| noise())
            .collect();
        loop {
            let t = first_beat_s + beat as f32 * period;
            if t >= seconds {
                break;
            }
            add(t, 0.18, &|x| {
                // Integrated phase of an exponential sweep 400 Hz -> 50 Hz.
                let phase = 50.0 * x + 350.0 / 30.0 * (1.0 - (-x * 30.0).exp());
                0.6 * (-x * 12.0).exp() * (2.0 * std::f32::consts::PI * phase).sin()
            });
            add(t, 0.004, &|x| {
                0.3 * (2.0 * std::f32::consts::PI * 2_500.0 * x).sin()
            });
            if beat % 2 == 1 {
                add(t, 0.05, &|x| {
                    let i = ((x * RATE as f32) as usize).min(clap_noise.len() - 1);
                    0.25 * (-x * 40.0).exp() * clap_noise[i]
                });
            }
            if offbeat_bass {
                add(t + period / 2.0, period * 0.4, &|x| {
                    let w = 2.0 * std::f32::consts::PI * 65.0 * x;
                    let tone = w.sin() + 0.4 * (2.0 * w).sin() + 0.3 * (3.0 * w).sin();
                    0.5 * (-x * 6.0).exp() * tone
                });
            }
            beat += 1;
        }
        out
    }

    /// Feed `pcm` in 10 ms blocks with a 1 ms tick; returns emitted beat times.
    fn run(tracker: &mut BeatTracker, pcm: &[f32], rate: u32) -> Vec<f64> {
        let block = (rate / 100) as usize;
        let mut pushed = 0;
        let mut beats = Vec::new();
        let duration_ms = pcm.len() as u64 * 1000 / rate as u64;
        for now in 0..duration_ms {
            let available = (now * rate as u64 / 1000) as usize;
            while pushed + block <= available {
                tracker.push(&pcm[pushed..pushed + block], rate, now as f64);
                pushed += block;
            }
            if tracker.poll_beat(now as f64) {
                beats.push(now as f64);
            }
        }
        beats
    }

    fn phase_errors(beats: &[f64], first_beat_ms: f64, period: f64, after_ms: f64) -> Vec<f64> {
        beats
            .iter()
            .filter(|t| **t >= after_ms)
            .map(|t| {
                let mut d = t - first_beat_ms;
                d -= (d / period).round() * period;
                d
            })
            .collect()
    }

    #[test]
    fn locks_tempo_and_kick_phase_despite_offbeat_bass() {
        for bpm in [128.0f32, 145.0, 174.0] {
            let pcm = club_loop(bpm, 30.0, 0.25, true);
            let mut tracker = BeatTracker::default();
            let beats = run(&mut tracker, &pcm, RATE);
            let tempo = tracker.tempo().expect("tempo");
            assert!(
                (tempo.bpm - bpm).abs() < 0.5,
                "{bpm}: detected {}",
                tempo.bpm
            );
            let period = 60_000.0 / bpm as f64;
            let errors = phase_errors(&beats, 250.0, period, 15_000.0);
            assert!(errors.len() > 20, "{bpm}: only {} beats", errors.len());
            let worst = errors.iter().fold(0.0f64, |m, e| m.max(e.abs()));
            assert!(
                worst < 25.0,
                "{bpm}: worst phase error {worst:.1} ms ({errors:?})"
            );
        }
    }

    #[test]
    fn works_at_48k() {
        let bpm = 140.0;
        let pcm44 = club_loop(bpm, 25.0, 0.1, false);
        // Naive resample is fine for a synthetic test signal.
        let pcm: Vec<f32> = (0..(pcm44.len() as f64 * 48_000.0 / RATE as f64) as usize)
            .map(|i| pcm44[((i as f64 * RATE as f64 / 48_000.0) as usize).min(pcm44.len() - 1)])
            .collect();
        let mut tracker = BeatTracker::default();
        let beats = run(&mut tracker, &pcm, 48_000);
        assert!((tracker.tempo().unwrap().bpm - bpm).abs() < 0.5);
        let errors = phase_errors(&beats, 100.0, 60_000.0 / bpm as f64, 12_000.0);
        assert!(errors.iter().all(|e| e.abs() < 25.0), "{errors:?}");
    }

    #[test]
    fn model_locks_tempo_and_kick_phase() {
        for bpm in [128.0f32, 145.0] {
            let pcm = club_loop(bpm, 30.0, 0.25, true);
            let mut tracker = BeatTracker::default();
            tracker.set_use_model(true);
            let beats = run(&mut tracker, &pcm, RATE);
            assert!(tracker.uses_model());
            let tempo = tracker.tempo().expect("tempo");
            assert!(
                (tempo.bpm - bpm).abs() < 0.5,
                "{bpm}: detected {}",
                tempo.bpm
            );
            let period = 60_000.0 / bpm as f64;
            let errors = phase_errors(&beats, 250.0, period, 15_000.0);
            assert!(errors.len() > 20, "{bpm}: only {} beats", errors.len());
            // The model peaks ~30 ms late on this synthetic kick, unlike on
            // real music (-1.4 ms mean offset on the validation tracks), so
            // check for a steady lock rather than the exact phase.
            let (lo, hi) = errors
                .iter()
                .fold((f64::MAX, f64::MIN), |(lo, hi), e| (lo.min(*e), hi.max(*e)));
            assert!(hi - lo < 10.0, "{bpm}: phase wanders {lo:.1}..{hi:.1} ms");
            assert!(
                lo > -40.0 && hi < 40.0,
                "{bpm}: phase off by {lo:.1}..{hi:.1} ms"
            );
        }
    }

    #[test]
    fn model_frames_use_the_tracker_clock() {
        assert_eq!(1000.0 / FRAME_RATE as f64, HOP_MS);
        BeatModel::from_bytes(BEAT_MODEL).expect("compiled-in model loads");
    }

    #[test]
    fn model_falls_back_to_flux_at_unsupported_rates() {
        let mut tracker = BeatTracker::default();
        tracker.set_use_model(true);
        tracker.push(&[0.1; 2_205], 22_050, 0.0);
        assert!(tracker.is_active());
        assert!(!tracker.uses_model());
    }

    #[test]
    fn silence_produces_no_tempo_or_beats() {
        let mut tracker = BeatTracker::default();
        let beats = run(&mut tracker, &vec![0.0; RATE as usize * 10], RATE);
        assert!(beats.is_empty());
        assert!(tracker.tempo().is_none());
    }

    #[test]
    fn manual_bpm_pins_tempo() {
        let mut tracker = BeatTracker::default();
        tracker.set_manual_bpm(Some(150.0));
        let pcm = club_loop(140.0, 10.0, 0.0, false);
        run(&mut tracker, &pcm, RATE);
        assert_eq!(tracker.tempo().unwrap().bpm, 150.0);
    }

    #[test]
    fn stalled_poll_emits_one_beat_not_a_backlog() {
        let pcm = club_loop(120.0, 20.0, 0.0, false);
        let mut tracker = BeatTracker::default();
        run(&mut tracker, &pcm, RATE);
        let next = tracker.next_beat_ms().unwrap();
        assert!(tracker.poll_beat(next + 5_000.0));
        assert!(!tracker.poll_beat(next + 5_001.0));
    }
}
