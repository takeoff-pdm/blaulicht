//! Scores the live beat detector against recorded tracks.
//!
//! Each file is replayed in simulated real time through the same frame
//! builder the microphone uses ([`StreamFrames`]), in fixed-size PCM blocks,
//! while the collector is ticked every millisecond like the engine mainloop.
//! The emitted beats and BPM are compared to an offline reference grid that is
//! computed from the whole track (constant-tempo music is assumed).
//!
//! ```text
//! cargo run --release -p blaulicht-audio-engine --bin beat_eval -- \
//!     --input ~/Documents/cdj2000nxs-mp3 \
//!     --phase-hints crates/audio_engine/beat_eval_phase_hints.txt
//! ```
//!
//! Columns: reference BPM, max phase drift between 30 s segments, share of
//! time the live BPM is within 2 % (acc1; acc2 also accepts half/double and
//! thirds), seconds to first lock, beat precision/recall/F1 with a 70 ms
//! tolerance after a 10 s warm-up, mean and spread of the beat offset to the
//! grid in ms, and collector CPU time as a share of real time.
//! `--detection` picks the beat detector as `beat_detection` in config.toml
//! does; `--legacy` is short for `--detection legacy`.

use std::{
    fs,
    path::{Path, PathBuf},
    time::Instant,
};

use audioviz::spectrum::config::StreamConfig;
use blaulicht_audio_engine::{
    file::AudioSourceSoundfile, stream_frames::StreamFrames, AudioSource, BeatDetection,
    BpmDetectStatus, CollectorOutputSpec, Frequency, SignalCollector,
};
use clap::Parser;
use rustfft::{num_complex::Complex, FftPlanner};

const FREQ_BUFFER_SIZE: usize = 2048;
/// Ignore the first seconds while the estimator fills its history.
const WARMUP_MS: usize = 10_000;
/// Standard beat-tracking tolerance (MIREX F-measure).
const MATCH_TOLERANCE_MS: f64 = 70.0;
const BPM_TOLERANCE: f32 = 0.02;

#[derive(Debug, Parser)]
#[command(about = "Score live beat detection against an offline reference grid")]
struct Args {
    /// Audio file or directory (recursive).
    #[arg(long)]
    input: PathBuf,
    /// PCM frames per capture block (441 = 10 ms at 44.1 kHz).
    #[arg(long, default_value_t = 441)]
    block: usize,
    /// Mainloop tick period in ms.
    #[arg(long, default_value_t = 1)]
    tick_ms: usize,
    /// Only process files whose name contains this string.
    #[arg(long)]
    filter: Option<String>,
    /// Beat detector to measure.
    #[arg(long, value_enum, default_value_t = BeatDetection::Tracker)]
    detection: BeatDetection,
    /// Same as `--detection legacy`; also withholds PCM from the collector.
    #[arg(long)]
    legacy: bool,
    /// Print the tempo-detector status histogram per file.
    #[arg(long)]
    verbose: bool,
    /// Write per-file beat/BPM traces as CSV into this directory.
    #[arg(long)]
    trace_dir: Option<PathBuf>,
    /// Render a 4 s spectrogram excerpt with reference (top, green; off-beats
    /// cyan) and live beats (bottom, red) starting at this time into
    /// `trace_dir`.
    #[arg(long)]
    plot_at_ms: Option<usize>,
    /// Length of the plotted excerpt.
    #[arg(long, default_value_t = 4_000.0)]
    plot_span_ms: f64,
    /// Hand-verified reference corrections: one file-name substring per line;
    /// the automatic reference phase of matching files is shifted by half a
    /// beat. Use it when a spectrogram shows the kicks on the off-beat ticks.
    #[arg(long)]
    phase_hints: Option<PathBuf>,
    /// Only compute (and plot) the reference; skip the live replay.
    #[arg(long)]
    reference_only: bool,
}

// ---------------------------------------------------------------------------
// Live replay
// ---------------------------------------------------------------------------

/// Plays mono PCM into [`StreamFrames`] as if a capture device delivered
/// `block`-sized buffers in real time; each poll drains every block that has
/// "arrived" by `now`, like `InputController::try_pull_data`.
struct ReplaySource {
    frames: StreamFrames,
    pcm: Vec<f32>,
    sample_rate: u32,
    block: usize,
    pushed: usize,
    provide_pcm: bool,
}

impl AudioSource for ReplaySource {
    fn get_freq_buffer_size(&self) -> usize {
        self.frames.frequencies().len()
    }

    fn get_frequencies(&mut self, now: usize) -> (&[Frequency], bool) {
        let available = ((now as u64 * self.sample_rate as u64) / 1000) as usize;
        let available = available.min(self.pcm.len());
        let blocks = available.saturating_sub(self.pushed) / self.block;
        if blocks == 0 {
            return (self.frames.frequencies(), false);
        }
        let end = self.pushed + blocks * self.block;
        // Stereo, both channels identical.
        let interleaved: Vec<f32> = self.pcm[self.pushed..end]
            .iter()
            .flat_map(|&s| [s, s])
            .collect();
        self.pushed = end;
        let has_new = self.frames.push_block(interleaved);
        (self.frames.frequencies(), has_new)
    }

    fn sample_rate(&self) -> Option<u32> {
        Some(self.sample_rate)
    }

    fn drain_samples(&mut self, output: &mut Vec<f32>) {
        if self.provide_pcm {
            self.frames.drain_samples(output);
        } else {
            self.frames.drain_samples(&mut Vec::new());
        }
    }
}

fn engine_stream_config() -> StreamConfig {
    // Mirrors the `[stream]` section of the repository's config.toml.
    StreamConfig {
        channel_count: 2,
        fft_resolution: 2048,
        gravity: Some(100.0),
        refresh_rate: 60,
        ..StreamConfig::default()
    }
}

struct LiveRun {
    beats_ms: Vec<f64>,
    /// (time ms, bpm) sampled every 10 ms.
    bpm: Vec<(usize, f32)>,
    cpu_seconds: f64,
    /// How often each tempo-detector status was seen (sampled every 10 ms).
    statuses: std::collections::BTreeMap<String, usize>,
}

fn run_live(pcm: &[f32], sample_rate: u32, args: &Args) -> anyhow::Result<LiveRun> {
    let source = ReplaySource {
        frames: StreamFrames::new(engine_stream_config(), 2, sample_rate, FREQ_BUFFER_SIZE),
        pcm: pcm.to_vec(),
        sample_rate,
        block: args.block,
        pushed: 0,
        provide_pcm: !args.legacy,
    };
    let mut collector = SignalCollector::new(
        Default::default(),
        [CollectorOutputSpec::default()],
        Default::default(),
        source,
        0,
    )?;
    collector.set_beat_detection(if args.legacy {
        BeatDetection::Legacy
    } else {
        args.detection
    });

    let duration_ms = (pcm.len() as u64 * 1000 / sample_rate as u64) as usize;
    let mut beats_ms = Vec::new();
    let mut bpm = Vec::new();
    let mut last_beat_id = 0;
    let mut statuses = std::collections::BTreeMap::<String, usize>::new();
    let started = Instant::now();
    for now in (0..duration_ms).step_by(args.tick_ms.max(1)) {
        collector.tick(now as u64)?;
        if collector.current.beat_event_id != last_beat_id {
            last_beat_id = collector.current.beat_event_id;
            beats_ms.push(now as f64);
        }
        if now % 10 == 0 {
            bpm.push((now, collector.current.bpm));
            let status = match collector.debug_data.bpm_status {
                BpmDetectStatus::WeakPeriodicity { strength, .. } => {
                    format!("Weak({:.3})", (strength * 200.0).round() / 200.0)
                }
                BpmDetectStatus::Warmup { .. } => "Warmup".into(),
                other => format!("{other:?}"),
            };
            *statuses.entry(status).or_default() += 1;
        }
    }
    Ok(LiveRun {
        beats_ms,
        bpm,
        cpu_seconds: started.elapsed().as_secs_f64(),
        statuses,
    })
}

// ---------------------------------------------------------------------------
// Offline reference
// ---------------------------------------------------------------------------

const REF_RATE_TARGET: u32 = 11_025;
const REF_WINDOW: usize = 512;
const REF_HOP: usize = 32;
const KICK_MAX_HZ: f32 = 150.0;
const FULL_MAX_HZ: f32 = 5_000.0;

struct Reference {
    bpm: f64,
    /// All grid beats in ms over the whole track.
    beats_ms: Vec<f64>,
    /// Whether a kick is audible at that grid beat.
    active: Vec<bool>,
    /// Max deviation of 30 s segment phases from the global phase (ms).
    phase_drift_ms: f64,
    /// Kick evidence at the chosen half-beat over the other half; near 1 the
    /// phase is ambiguous and worth checking on a spectrogram.
    phase_margin: f64,
}

struct Odf {
    kick: Vec<f32>,
    full: Vec<f32>,
    /// Rise of log sub-bass (30..120 Hz) band energy.
    sub_rise: Vec<f32>,
    /// Log-energy rise of the high-passed signal in short (~12 ms) windows:
    /// the attack of kicks and claps with little timing smear.
    attack: Vec<f32>,
    /// Spectral flux in 150..500 Hz: where a kick's pitch sweep starts. Off-beat
    /// bass sits lower and hats/claps higher.
    sweep: Vec<f32>,
    frame_ms: f64,
    /// Time of frame 0 in ms.
    offset_ms: f64,
}

fn reference_odf(pcm: &[f32], sample_rate: u32) -> Odf {
    let factor = (sample_rate as f32 / REF_RATE_TARGET as f32)
        .round()
        .max(1.0) as usize;
    let rate = sample_rate as f32 / factor as f32;
    let x: Vec<f32> = pcm
        .chunks_exact(factor)
        .map(|c| c.iter().sum::<f32>() / factor as f32)
        .collect();

    let window: Vec<f32> = (0..REF_WINDOW)
        .map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / REF_WINDOW as f32).cos())
        .collect();
    let fft = FftPlanner::<f32>::new().plan_fft_forward(REF_WINDOW);
    let bin_hz = rate / REF_WINDOW as f32;
    let kick_bins = (KICK_MAX_HZ / bin_hz).ceil() as usize;
    let full_bins = (FULL_MAX_HZ / bin_hz).ceil() as usize;

    let frames = x.len().saturating_sub(REF_WINDOW) / REF_HOP;
    let mut prev = vec![0.0f32; full_bins];
    let mut kick = Vec::with_capacity(frames);
    let mut full = Vec::with_capacity(frames);
    let mut sub_energy = Vec::with_capacity(frames);
    let mut sweep = Vec::with_capacity(frames);
    let sweep_lo = (150.0 / bin_hz).round() as usize;
    let sweep_hi = (500.0 / bin_hz).round() as usize;
    let sub_lo = (30.0 / bin_hz).floor().max(1.0) as usize;
    let sub_hi = (120.0 / bin_hz).ceil() as usize;
    let mut buf = vec![Complex::default(); REF_WINDOW];
    for f in 0..frames {
        let start = f * REF_HOP;
        for i in 0..REF_WINDOW {
            buf[i] = Complex::new(x[start + i] * window[i], 0.0);
        }
        fft.process(&mut buf);
        let mut k = 0.0;
        let mut a = 0.0;
        let mut e = 0.0;
        let mut w = 0.0;
        for b in 1..full_bins {
            let mag = (1.0 + 100.0 * buf[b].norm()).ln();
            let d = (mag - prev[b]).max(0.0);
            prev[b] = mag;
            if b < kick_bins {
                k += d;
            }
            if (sweep_lo..=sweep_hi).contains(&b) {
                w += d;
            }
            if (sub_lo..=sub_hi).contains(&b) {
                e += buf[b].norm_sqr();
            }
            a += d;
        }
        kick.push(k);
        full.push(a);
        sweep.push(w);
        sub_energy.push((e + 1e-6).ln());
    }

    let frame_ms = REF_HOP as f64 * 1000.0 / rate as f64;

    // Attack envelope, frame-aligned with the STFT frames (same centers).
    const ATTACK_WINDOW: usize = 128;
    let highpassed: Vec<f32> = (0..x.len())
        .map(|i| if i == 0 { 0.0 } else { x[i] - x[i - 1] })
        .collect();
    let attack_energy: Vec<f32> = (0..frames)
        .map(|f| {
            let center = f * REF_HOP + REF_WINDOW / 2;
            let lo = center.saturating_sub(ATTACK_WINDOW / 2);
            let hi = (lo + ATTACK_WINDOW).min(highpassed.len());
            let e: f32 = highpassed[lo..hi].iter().map(|v| v * v).sum();
            (e / ATTACK_WINDOW as f32 + 1e-9).ln()
        })
        .collect();
    let mut attack: Vec<f32> = (0..frames)
        .map(|f| {
            if f < 2 {
                0.0
            } else {
                (attack_energy[f] - attack_energy[f - 2]).max(0.0)
            }
        })
        .collect();

    let detrend = |v: &mut Vec<f32>| {
        let radius = (250.0 / frame_ms) as usize;
        let mut prefix = vec![0.0f64; v.len() + 1];
        for (i, x) in v.iter().enumerate() {
            prefix[i + 1] = prefix[i] + *x as f64;
        }
        let out: Vec<f32> = (0..v.len())
            .map(|i| {
                let lo = i.saturating_sub(radius);
                let hi = (i + radius + 1).min(v.len());
                let mean = (prefix[hi] - prefix[lo]) / (hi - lo) as f64;
                (v[i] - mean as f32).max(0.0)
            })
            .collect();
        *v = out;
    };
    // Sub-bass energy rise over ~12 ms.
    let lag = (12.0 / frame_ms).round().max(1.0) as usize;
    let mut sub_rise: Vec<f32> = (0..sub_energy.len())
        .map(|i| {
            if i < lag {
                0.0
            } else {
                (sub_energy[i] - sub_energy[i - lag]).max(0.0)
            }
        })
        .collect();
    detrend(&mut kick);
    detrend(&mut full);
    detrend(&mut sub_rise);
    detrend(&mut attack);
    detrend(&mut sweep);

    Odf {
        attack,
        sweep,
        kick,
        full,
        sub_rise,
        frame_ms,
        offset_ms: REF_WINDOW as f64 / 2.0 * 1000.0 / rate as f64,
    }
}

/// Fold `odf` with `period_frames` into a phase histogram; returns the
/// (smoothed) peak height and its phase in frames.
fn fold(odf: &[f32], period_frames: f64, range: std::ops::Range<usize>) -> (f64, f64) {
    let bins = period_frames.round().max(4.0) as usize;
    let mut hist = vec![0.0f64; bins];
    for i in range {
        let phase = (i as f64 % period_frames) / period_frames;
        hist[((phase * bins as f64) as usize).min(bins - 1)] += odf[i] as f64;
    }
    let smoothed: Vec<f64> = (0..bins)
        .map(|i| 0.25 * hist[(i + bins - 1) % bins] + 0.5 * hist[i] + 0.25 * hist[(i + 1) % bins])
        .collect();
    let (best, &height) = smoothed
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(b.1))
        .unwrap();
    let ym = smoothed[(best + bins - 1) % bins];
    let yp = smoothed[(best + 1) % bins];
    let denom = ym - 2.0 * height + yp;
    let delta = if denom.abs() > 1e-12 {
        (0.5 * (ym - yp) / denom).clamp(-0.5, 0.5)
    } else {
        0.0
    };
    let phase = ((best as f64 + 0.5 + delta) / bins as f64) * period_frames;
    (height, phase)
}

fn compute_reference(pcm: &[f32], sample_rate: u32, flip_phase: bool) -> Reference {
    let odf = reference_odf(pcm, sample_rate);
    let n = odf.kick.len();
    let combined: Vec<f32> = {
        let norm = |v: &[f32]| v.iter().map(|x| *x as f64).sum::<f64>().max(1e-9) / v.len() as f64;
        let (nk, nf) = (norm(&odf.kick), norm(&odf.full));
        odf.kick
            .iter()
            .zip(&odf.full)
            .map(|(k, f)| (*k as f64 / nk + *f as f64 / nf) as f32)
            .collect()
    };

    // Coarse: fold-peak sharpness over 100..200 BPM in 0.1 BPM steps. Music
    // in the corpus is 4/4 club music, so the tactus lives in this range.
    let to_period = |bpm: f64| 60_000.0 / bpm / odf.frame_ms;
    let score = |bpm: f64| {
        let period = to_period(bpm);
        let (height, _) = fold(&combined, period, 0..n);
        let mean = combined.iter().map(|x| *x as f64).sum::<f64>() / period.round();
        height / mean.max(1e-9)
    };
    let mut best = (0.0, f64::MIN);
    let mut bpm = 100.0;
    while bpm <= 200.0 {
        let s = score(bpm);
        if s > best.1 {
            best = (bpm, s);
        }
        bpm += 0.1;
    }
    // Fine: 0.002 BPM around the coarse peak, phase from the kick band.
    let mut fine = (best.0, f64::MIN);
    let mut bpm = best.0 - 0.15;
    while bpm <= best.0 + 0.15 {
        let (height, _) = fold(&odf.kick, to_period(bpm), 0..n);
        if height > fine.1 {
            fine = (bpm, height);
        }
        bpm += 0.002;
    }
    let ref_bpm = fine.0;
    let period = to_period(ref_bpm);
    // Phase. Club tracks often put a bass note on every off-beat and an open
    // hat between kicks, so neither low-band onsets nor transients alone say
    // where the beat is. A kick is a pitch sweep that starts around
    // 150..500 Hz together with a broadband attack: off-beat bass has the
    // former without the latter, hats the reverse. Pick the half-beat where
    // both coincide, then time it with the short-window attack envelope. This
    // still misses some tracks; verify on spectrograms (`--plot-at-ms`) and
    // correct with `--phase-hints`.
    let histogram = |v: &[f32]| -> Vec<f64> {
        let bins = period.round() as usize;
        let mut hist = vec![0.0f64; bins];
        for (i, x) in v.iter().enumerate() {
            let ph = (i as f64).rem_euclid(period) / period;
            hist[((ph * bins as f64) as usize).min(bins - 1)] += *x as f64;
        }
        hist
    };
    let local_max = |hist: &[f64], center: f64, radius: f64| -> (f64, f64) {
        let bins = hist.len() as isize;
        let at = |i: isize| hist[i.rem_euclid(bins) as usize];
        let best = ((center - radius).round() as isize..=(center + radius).round() as isize)
            .max_by(|a, b| at(*a).total_cmp(&at(*b)))
            .unwrap();
        ((best as f64 + 0.5).rem_euclid(period), at(best))
    };
    let mean_of = |v: &[f32]| v.iter().map(|x| *x as f64).sum::<f64>() / v.len().max(1) as f64;
    let (attack_mean, sweep_mean) = (mean_of(&odf.attack), mean_of(&odf.sweep));
    let kick_evidence: Vec<f32> = (0..n)
        .map(|i| {
            let lo = i.saturating_sub(1);
            let hi = (i + 2).min(n);
            let sweep = odf.sweep[lo..hi].iter().copied().fold(0.0f32, f32::max);
            ((odf.attack[i] as f64 / attack_mean.max(1e-9)) * (sweep as f64 / sweep_mean.max(1e-9)))
                as f32
        })
        .collect();
    let (_, kick_phase) = fold(&kick_evidence, period, 0..n);
    let evidence_hist = histogram(&kick_evidence);
    let (_, peak) = local_max(&evidence_hist, kick_phase, 1.0);
    let (_, other) = local_max(&evidence_hist, kick_phase + period / 2.0, period / 8.0);
    let slot = if flip_phase {
        kick_phase + period / 2.0
    } else {
        kick_phase
    };
    let (phase, _) = local_max(&histogram(&odf.attack), slot, period / 16.0);

    // Drift check: per 30 s segment, the low-band phase vs the global one.
    let low: Vec<f32> = odf
        .kick
        .iter()
        .zip(&odf.sub_rise)
        .map(|(k, s)| k + s)
        .collect();
    let (_, low_phase) = fold(&low, period, 0..n);
    let seg = (30_000.0 / odf.frame_ms) as usize;
    let mut drift: f64 = 0.0;
    let mut start = 0;
    while start + seg <= n {
        let (h, p) = fold(&low, period, start..start + seg);
        let mean = low[start..start + seg]
            .iter()
            .map(|x| *x as f64)
            .sum::<f64>()
            / period.round();
        // Only segments with a clear beat; breakdowns fold to noise.
        if h > 3.0 * mean {
            let mut d = p - low_phase;
            d -= (d / period).round() * period;
            drift = drift.max(d.abs() * odf.frame_ms);
        }
        start += seg;
    }

    let mut beats_ms = Vec::new();
    let mut strengths = Vec::new();
    let tol = (20.0 / odf.frame_ms).round() as isize;
    let mut f = phase;
    while f < n as f64 {
        beats_ms.push(odf.offset_ms + f * odf.frame_ms);
        let c = f.round() as isize;
        let s = ((c - tol).max(0)..(c + tol + 1).min(n as isize))
            .map(|i| odf.kick[i as usize])
            .fold(0.0f32, f32::max);
        strengths.push(s);
        f += period;
    }
    let mut sorted = strengths.clone();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let p90 = sorted[(sorted.len() as f32 * 0.9) as usize];
    let active = strengths.iter().map(|s| *s > 0.25 * p90).collect();

    Reference {
        bpm: ref_bpm,
        beats_ms,
        active,
        phase_drift_ms: drift,
        phase_margin: peak / other.max(1e-9),
    }
}

fn plot(
    pcm: &[f32],
    sample_rate: u32,
    reference: &Reference,
    live: &LiveRun,
    start_ms: usize,
    span_ms: f64,
    path: &Path,
) -> anyhow::Result<()> {
    const WIDTH: u32 = 1_000;
    const HEIGHT: u32 = 300;
    const WIN: usize = 1024;
    let col_ms = span_ms / WIDTH as f64;
    let width = WIDTH;
    let fft = FftPlanner::<f32>::new().plan_fft_forward(WIN);
    let bin_hz = sample_rate as f64 / WIN as f64;
    // Log frequency axis 30 Hz .. 10 kHz.
    let row_hz = |row: u32| 30.0 * (10_000.0f64 / 30.0).powf(1.0 - row as f64 / HEIGHT as f64);
    let mut img = image::RgbImage::new(width, HEIGHT + 40);
    let mut buf = vec![Complex::default(); WIN];
    for x in 0..width {
        let t_ms = start_ms as f64 + x as f64 * col_ms;
        let center = (t_ms / 1000.0 * sample_rate as f64) as isize;
        for i in 0..WIN {
            let idx = center - WIN as isize + i as isize;
            let w = 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / WIN as f32).cos();
            let v = if idx >= 0 && (idx as usize) < pcm.len() {
                pcm[idx as usize]
            } else {
                0.0
            };
            buf[i] = Complex::new(v * w, 0.0);
        }
        fft.process(&mut buf);
        for y in 0..HEIGHT {
            let bin = (row_hz(y) / bin_hz).round() as usize;
            let db = 20.0 * (buf[bin.min(WIN / 2)].norm() as f64 + 1e-6).log10();
            let v = ((db + 10.0) / 60.0).clamp(0.0, 1.0);
            let c = (v * 255.0) as u8;
            img.put_pixel(x, y, image::Rgb([c, c, c]));
        }
    }
    let mut mark = |t: f64, y0: u32, y1: u32, color: [u8; 3]| {
        let x = ((t - start_ms as f64) / col_ms).round();
        for dx in [-1.0, 0.0, 1.0] {
            let x = x + dx;
            if x >= 0.0 && (x as u32) < width {
                for y in y0..y1 {
                    img.put_pixel(x as u32, y, image::Rgb(color));
                }
            }
        }
    };
    let period = 60_000.0 / reference.bpm;
    for t in &reference.beats_ms {
        mark(*t, HEIGHT, HEIGHT + 14, [0, 255, 0]);
        mark(*t + period / 2.0, HEIGHT, HEIGHT + 6, [0, 200, 255]);
    }
    for t in &live.beats_ms {
        mark(*t, HEIGHT + 20, HEIGHT + 40, [255, 40, 40]);
    }
    let window = |v: &[f64]| -> Vec<String> {
        v.iter()
            .filter(|t| **t >= start_ms as f64 && **t < start_ms as f64 + span_ms)
            .map(|t| format!("{:.0}", (*t - start_ms as f64) / col_ms))
            .collect()
    };
    println!("  plot px: ref beats {:?}", window(&reference.beats_ms));
    println!("  plot px: live beats {:?}", window(&live.beats_ms));
    img.save(path)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Scoring
// ---------------------------------------------------------------------------

#[derive(Default, Clone, Copy)]
struct Score {
    bpm_acc1: f64,
    bpm_acc2: f64,
    first_lock_s: Option<f64>,
    precision: f64,
    recall: f64,
    f1: f64,
    offset_mean_ms: f64,
    offset_std_ms: f64,
    cpu_ratio: f64,
}

fn score(reference: &Reference, live: &LiveRun, duration_ms: usize) -> Score {
    let rb = reference.bpm as f32;
    let within = |bpm: f32, target: f32| (bpm - target).abs() / target <= BPM_TOLERANCE;

    // BPM accuracy over active-beat regions after warm-up.
    let active_at = |t: usize| {
        let period = 60_000.0 / reference.bpm;
        let idx = ((t as f64 - reference.beats_ms[0]) / period).round();
        idx >= 0.0 && (idx as usize) < reference.active.len() && reference.active[idx as usize]
    };
    let mut total = 0usize;
    let mut acc1 = 0usize;
    let mut acc2 = 0usize;
    let mut first_lock = None;
    for &(t, bpm) in &live.bpm {
        if first_lock.is_none() && within(bpm, rb) {
            first_lock = Some(t as f64 / 1000.0);
        }
        if t < WARMUP_MS || !active_at(t) {
            continue;
        }
        total += 1;
        if within(bpm, rb) {
            acc1 += 1;
        }
        if [0.5, 1.0, 2.0, 1.0 / 3.0, 3.0, 2.0 / 3.0, 1.5]
            .iter()
            .any(|f| within(bpm, rb * f))
        {
            acc2 += 1;
        }
    }

    // Beat F-measure after warm-up. Precision counts any grid beat as a hit;
    // recall only asks for beats where a kick is audible.
    let grid: Vec<(f64, bool)> = reference
        .beats_ms
        .iter()
        .copied()
        .zip(reference.active.iter().copied())
        .filter(|(t, _)| *t >= WARMUP_MS as f64 && *t < duration_ms as f64)
        .collect();
    let fired: Vec<f64> = live
        .beats_ms
        .iter()
        .copied()
        .filter(|t| *t >= WARMUP_MS as f64)
        .collect();
    let mut used = vec![false; grid.len()];
    let mut offsets = Vec::new();
    let mut j = 0usize;
    for &t in &fired {
        while j < grid.len() && grid[j].0 < t - MATCH_TOLERANCE_MS {
            j += 1;
        }
        let mut best: Option<usize> = None;
        let mut k = j;
        while k < grid.len() && grid[k].0 <= t + MATCH_TOLERANCE_MS {
            if !used[k] && best.is_none_or(|b| (grid[k].0 - t).abs() < (grid[b].0 - t).abs()) {
                best = Some(k);
            }
            k += 1;
        }
        if let Some(b) = best {
            used[b] = true;
            offsets.push(t - grid[b].0);
        }
    }
    let active_total = grid.iter().filter(|g| g.1).count();
    let active_hit = grid.iter().zip(&used).filter(|(g, u)| g.1 && **u).count();
    let precision = offsets.len() as f64 / fired.len().max(1) as f64;
    let recall = active_hit as f64 / active_total.max(1) as f64;
    let f1 = if precision + recall > 0.0 {
        2.0 * precision * recall / (precision + recall)
    } else {
        0.0
    };
    let mean = offsets.iter().sum::<f64>() / offsets.len().max(1) as f64;
    let std = (offsets.iter().map(|o| (o - mean).powi(2)).sum::<f64>()
        / offsets.len().max(1) as f64)
        .sqrt();

    Score {
        bpm_acc1: acc1 as f64 / total.max(1) as f64,
        bpm_acc2: acc2 as f64 / total.max(1) as f64,
        first_lock_s: first_lock,
        precision,
        recall,
        f1,
        offset_mean_ms: mean,
        offset_std_ms: std,
        cpu_ratio: live.cpu_seconds / (duration_ms as f64 / 1000.0),
    }
}

// ---------------------------------------------------------------------------

fn collect_files(path: &Path, files: &mut Vec<PathBuf>) -> anyhow::Result<()> {
    if path.is_file() {
        files.push(path.to_path_buf());
        return Ok(());
    }
    for entry in fs::read_dir(path)? {
        let p = entry?.path();
        if p.is_dir() {
            collect_files(&p, files)?;
        } else if p
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "mp3" | "flac" | "wav"))
        {
            files.push(p);
        }
    }
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let mut files = Vec::new();
    collect_files(&args.input, &mut files)?;
    files.sort();
    if let Some(filter) = &args.filter {
        files.retain(|f| f.to_string_lossy().contains(filter.as_str()));
    }
    if let Some(dir) = &args.trace_dir {
        fs::create_dir_all(dir)?;
    }
    let hints: Vec<String> = match &args.phase_hints {
        Some(path) => fs::read_to_string(path)?
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .map(String::from)
            .collect(),
        None => Vec::new(),
    };

    println!(
        "{:<44} {:>7} {:>6} {:>6} {:>6} {:>6} {:>6} {:>6} {:>6} {:>7} {:>6} {:>6}",
        "file", "ref", "drift", "acc1", "acc2", "lock", "P", "R", "F1", "off", "std", "cpu%"
    );
    let mut scores = Vec::new();
    for file in &files {
        let source = match AudioSourceSoundfile::new(&file.to_string_lossy(), FREQ_BUFFER_SIZE) {
            Ok(s) => s,
            Err(err) => {
                eprintln!("FAILED {}: {err}", file.display());
                continue;
            }
        };
        let pcm = source.samples().to_vec();
        let rate = source.sample_rate();
        let duration_ms = (pcm.len() as u64 * 1000 / rate as u64) as usize;
        let flip = hints
            .iter()
            .any(|h| file.to_string_lossy().contains(h.as_str()));
        let reference = compute_reference(&pcm, rate, flip);
        let live = if args.reference_only {
            LiveRun {
                beats_ms: Vec::new(),
                bpm: Vec::new(),
                cpu_seconds: 0.0,
                statuses: Default::default(),
            }
        } else {
            run_live(&pcm, rate, &args)?
        };
        let s = score(&reference, &live, duration_ms);

        let name: String = file
            .file_stem()
            .map(|s| s.to_string_lossy().chars().take(44).collect())
            .unwrap_or_default();
        println!(
            "{:<44} {:>7.2} {:>6.1} {:>6.2} {:>6.2} {:>6} {:>6.2} {:>6.2} {:>6.2} {:>7.1} {:>6.1} {:>6.2}",
            name,
            reference.bpm,
            reference.phase_drift_ms,
            s.bpm_acc1,
            s.bpm_acc2,
            s.first_lock_s
                .map(|t| format!("{t:.1}"))
                .unwrap_or_else(|| "-".into()),
            s.precision,
            s.recall,
            s.f1,
            s.offset_mean_ms,
            s.offset_std_ms,
            s.cpu_ratio * 100.0,
        );

        if args.verbose {
            println!("  reference phase margin: {:.2}", reference.phase_margin);
            let total: usize = live.statuses.values().sum();
            let mut items: Vec<_> = live.statuses.iter().collect();
            items.sort_by(|a, b| b.1.cmp(a.1));
            let text: Vec<String> = items
                .iter()
                .take(8)
                .map(|(k, v)| format!("{k}:{:.0}%", **v as f64 * 100.0 / total.max(1) as f64))
                .collect();
            println!("  status: {}", text.join(" "));
        }
        if let (Some(dir), Some(at)) = (&args.trace_dir, args.plot_at_ms) {
            let path = dir.join(format!("{name}.png"));
            plot(&pcm, rate, &reference, &live, at, args.plot_span_ms, &path)?;
        }
        if let Some(dir) = &args.trace_dir {
            let mut out = String::from("kind,time_ms,value\n");
            for (t, a) in reference.beats_ms.iter().zip(&reference.active) {
                out += &format!("ref,{t:.1},{}\n", u8::from(*a));
            }
            for t in &live.beats_ms {
                out += &format!("beat,{t:.1},1\n");
            }
            for (t, b) in &live.bpm {
                if t % 100 == 0 {
                    out += &format!("bpm,{t},{b:.2}\n");
                }
            }
            fs::write(dir.join(format!("{name}.csv")), out)?;
        }
        scores.push(s);
    }

    if !scores.is_empty() {
        let n = scores.len() as f64;
        let avg = |f: fn(&Score) -> f64| scores.iter().map(f).sum::<f64>() / n;
        println!(
            "{:<44} {:>7} {:>6} {:>6.2} {:>6.2} {:>6} {:>6.2} {:>6.2} {:>6.2} {:>7.1} {:>6.1} {:>6.2}",
            format!("MEAN over {} files", scores.len()),
            "",
            "",
            avg(|s| s.bpm_acc1),
            avg(|s| s.bpm_acc2),
            "",
            avg(|s| s.precision),
            avg(|s| s.recall),
            avg(|s| s.f1),
            avg(|s| s.offset_mean_ms),
            avg(|s| s.offset_std_ms),
            avg(|s| s.cpu_ratio) * 100.0,
        );
    }
    Ok(())
}
