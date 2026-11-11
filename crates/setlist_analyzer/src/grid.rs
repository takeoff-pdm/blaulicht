//! Constant-tempo beat grid from the learned beat model.
//!
//! The model is the causal TCN the live engine runs (`audio-model/`), but
//! offline it sees the whole track, so instead of tracking beats one by one
//! the grid is fitted to all of them at once: fold the beat activation over
//! candidate tempos, take the sharpest fold, then least-squares fit the
//! activation peaks near the grid. Club tracks keep one tempo, which is what
//! DJ software assumes as well.
//!
//! The model's own downbeat output hardly separates the beats of a bar on
//! club tracks, so the downbeat comes from the sections instead
//! (`sections::downbeat_scores`).

use anyhow::anyhow;
use blaulicht_audio_features::{BeatModel, MelFrame, MelFrontend, FRAME_RATE};

static BEAT_MODEL: &[u8] = include_bytes!("../../audio_engine/models/beat_model.bin");

const FRAME_MS: f64 = 1000.0 / FRAME_RATE as f64;
/// The activation peaks this long after the kick's attack. Measured against
/// `beat_eval`'s attack-timed reference grids on the cdj2000nxs set (median
/// of 16 tracks, spread 20..28 ms); lights should hit with the attack.
const ATTACK_CALIBRATION_MS: f64 = -24.0;
const FOLD_BINS: usize = 48;

pub struct Activations {
    pub mels: Vec<MelFrame>,
    pub beat: Vec<f32>,
    /// The model fires this long before the beat.
    pub lead_ms: f64,
}

impl Activations {
    /// Track time of a (fractional) frame's beat. A beat at `t` first shows
    /// in frame `floor(t * FRAME_RATE)`, so a frame stands for the middle of
    /// its 10 ms, plus the model's lead.
    pub fn frame_to_ms(&self, frame: f64) -> f64 {
        (frame + 0.5) * FRAME_MS + self.lead_ms
    }

    pub fn ms_to_frame(&self, ms: f64) -> f64 {
        (ms - self.lead_ms) / FRAME_MS - 0.5
    }
}

pub fn activations(samples: &[f32], sample_rate: u32) -> anyhow::Result<Activations> {
    let mut frontend = MelFrontend::new(sample_rate).map_err(|e| anyhow!(e))?;
    let mut model = BeatModel::from_bytes(BEAT_MODEL).map_err(|e| anyhow!(e))?;
    let mut mels = Vec::with_capacity(samples.len() / frontend.config().hop + 1);
    frontend.push(samples, &mut mels);
    let beat = mels.iter().map(|m| model.step(m).beat).collect();
    Ok(Activations {
        mels,
        beat,
        lead_ms: model.info().lead_ms as f64,
    })
}

#[derive(Clone, Debug)]
pub struct Grid {
    pub bpm: f64,
    /// Grid beat 0, in `[0, period)`.
    pub first_beat_ms: f64,
    /// Activation peaks the fit used, and their spread around the grid.
    pub fitted_beats: usize,
    pub residual_ms: f64,
}

impl Grid {
    pub fn period_ms(&self) -> f64 {
        60_000.0 / self.bpm
    }

    pub fn beat_ms(&self, beat: i64) -> f64 {
        self.first_beat_ms + beat as f64 * self.period_ms()
    }

    /// Moves the grid by half a beat, for tracks where the fit locked onto
    /// the off-beat.
    pub fn shift_half_beat(&mut self) {
        let period = self.period_ms();
        self.first_beat_ms = (self.first_beat_ms + period / 2.0) % period;
    }
}

/// Folds `act` with `period` frames; returns the peak's sharpness (peak over
/// mean) and its phase in frames.
fn fold(act: &[f32], period: f64) -> (f64, f64) {
    let mut hist = [0.0f64; FOLD_BINS];
    for (i, a) in act.iter().enumerate() {
        let phase = (i as f64 % period) / period;
        hist[((phase * FOLD_BINS as f64) as usize).min(FOLD_BINS - 1)] += *a as f64;
    }
    let at = |i: isize| hist[i.rem_euclid(FOLD_BINS as isize) as usize];
    let smoothed: Vec<f64> = (0..FOLD_BINS as isize)
        .map(|i| 0.25 * at(i - 1) + 0.5 * at(i) + 0.25 * at(i + 1))
        .collect();
    let (best, &peak) = smoothed
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(b.1))
        .unwrap();
    let mean = smoothed.iter().sum::<f64>() / FOLD_BINS as f64;
    let (ym, yp) = (
        smoothed[(best + FOLD_BINS - 1) % FOLD_BINS],
        smoothed[(best + 1) % FOLD_BINS],
    );
    let denom = ym - 2.0 * peak + yp;
    let delta = if denom.abs() > 1e-12 {
        (0.5 * (ym - yp) / denom).clamp(-0.5, 0.5)
    } else {
        0.0
    };
    let phase = (best as f64 + 0.5 + delta) / FOLD_BINS as f64 * period;
    (peak / mean.max(1e-12), phase)
}

/// Parabolic peak position of `v` around index `i`.
fn refine_peak(v: &[f32], i: usize) -> f64 {
    if i == 0 || i + 1 >= v.len() {
        return i as f64;
    }
    let (ym, y, yp) = (v[i - 1] as f64, v[i] as f64, v[i + 1] as f64);
    let denom = ym - 2.0 * y + yp;
    if denom.abs() < 1e-12 {
        return i as f64;
    }
    i as f64 + (0.5 * (ym - yp) / denom).clamp(-0.5, 0.5)
}

fn argmax(v: &[f32], lo: usize, hi: usize) -> Option<usize> {
    (lo..hi.min(v.len())).max_by(|a, b| v[*a].total_cmp(&v[*b]))
}

/// Least-squares line `y = a + b x`.
fn line_fit(points: &[(f64, f64)]) -> Option<(f64, f64)> {
    let n = points.len() as f64;
    if n < 2.0 {
        return None;
    }
    let (sx, sy) = points
        .iter()
        .fold((0.0, 0.0), |(sx, sy), (x, y)| (sx + x, sy + y));
    let (mx, my) = (sx / n, sy / n);
    let (sxx, sxy) = points.iter().fold((0.0, 0.0), |(sxx, sxy), (x, y)| {
        (sxx + (x - mx) * (x - mx), sxy + (x - mx) * (y - my))
    });
    if sxx <= 0.0 {
        return None;
    }
    let b = sxy / sxx;
    Some((my - b * mx, b))
}

pub fn fit_grid(act: &Activations, min_bpm: f64, max_bpm: f64) -> Option<Grid> {
    let n = act.beat.len();
    if n < 10 * FRAME_RATE as usize {
        return None;
    }
    let to_period = |bpm: f64| 60.0 * FRAME_RATE as f64 / bpm;

    // Coarse then fine tempo search on fold sharpness.
    let search = |from: f64, to: f64, step: f64| {
        let mut best = (from, f64::MIN, 0.0);
        let mut bpm = from;
        while bpm <= to {
            let (score, phase) = fold(&act.beat, to_period(bpm));
            if score > best.1 {
                best = (bpm, score, phase);
            }
            bpm += step;
        }
        best
    };
    let (coarse, _, _) = search(min_bpm, max_bpm, 0.05);
    let (bpm, _, phase) = search(coarse - 0.06, coarse + 0.06, 0.002);
    let mut period = to_period(bpm);
    let mut offset = phase;

    // Refine on the activation peaks near the grid. Weak peaks (breakdowns)
    // are left out; outliers are dropped and the line refitted.
    let window = (0.2 * period).round() as isize;
    let mut peaks: Vec<(f64, f64, f32)> = Vec::new();
    let mut k = 0i64;
    loop {
        let center = offset + k as f64 * period;
        if center >= n as f64 {
            break;
        }
        let c = center.round() as isize;
        let lo = (c - window).max(0) as usize;
        let hi = (c + window + 1).max(0) as usize;
        if let Some(i) = argmax(&act.beat, lo, hi) {
            peaks.push((k as f64, refine_peak(&act.beat, i), act.beat[i]));
        }
        k += 1;
    }
    let mut heights: Vec<f32> = peaks.iter().map(|p| p.2).collect();
    heights.sort_by(|a, b| a.total_cmp(b));
    let p90 = heights.get(heights.len() * 9 / 10).copied().unwrap_or(0.0);
    let mut points: Vec<(f64, f64)> = peaks
        .iter()
        .filter(|p| p.2 >= 0.4 * p90)
        .map(|p| (p.0, p.1))
        .collect();
    let mut residual = f64::NAN;
    for limit in [4.0, 2.5, 1.5] {
        let Some((a, b)) = line_fit(&points) else { break };
        let kept: Vec<(f64, f64)> = points
            .iter()
            .copied()
            .filter(|(x, y)| (y - (a + b * x)).abs() <= limit)
            .collect();
        if kept.len() < 16 {
            break;
        }
        (offset, period) = (a, b);
        points = kept;
        let ss: f64 = points.iter().map(|(x, y)| (y - (a + b * x)).powi(2)).sum();
        residual = (ss / points.len() as f64).sqrt() * FRAME_MS;
    }
    let bpm = 60.0 * FRAME_RATE as f64 / period;
    let period_ms = period * FRAME_MS;

    // Grid beat 0 is the first grid beat at or after the track start.
    let t0 = act.frame_to_ms(offset) + ATTACK_CALIBRATION_MS;
    let shift = (-t0 / period_ms).ceil();
    let first_beat_ms = t0 + shift * period_ms;

    Some(Grid {
        bpm,
        first_beat_ms,
        fitted_beats: points.len(),
        residual_ms: residual,
    })
}
