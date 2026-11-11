//! Downbeat, bar features and song sections from the beat grid.
//!
//! Club tracks change on the downbeat: the kick drops out for a breakdown,
//! comes back for the drop, a crash marks the next phrase. So the downbeat
//! is the beat phase where the per-beat features change most, and sections
//! come from labeling each bar as quiet (no kick), groove (kick) or peak
//! (kick and full energy), smoothed with a Viterbi pass that prefers to
//! switch on phrase boundaries.

use blaulicht_audio_features::{MelFrontend, N_MELS};
use blaulicht_setlist::{BarFeatures, Section, SectionKind, BEATS_PER_BAR};

use crate::grid::{Activations, Grid};

const BPB: usize = BEATS_PER_BAR as usize;
/// A build is at most this many bars before its drop.
const BUILD_BARS: usize = 8;
/// Shorter runs are fills or misdetections, not sections.
const MIN_RUN_BARS: usize = 4;
/// A shorter loud run is a louder phrase, not a drop worth a blackout.
const MIN_DROP_BARS: usize = 8;
/// On the cdj2000nxs set, the change scores alone put 12 of 18 downbeats on
/// the music start (all 18 start on grid beat 0) with margins of 1.03..1.5;
/// with this prior 16, and the other two win by more than 1.2 on changes.
const MUSIC_START_PRIOR: f64 = 1.2;

/// Per grid beat, from the mel frames between it and the next beat.
pub struct BeatFeatures {
    pub loud: Vec<f32>,
    pub low: Vec<f32>,
    pub high: Vec<f32>,
    /// Low-band onset strength at the beat: clear for a kick on its own,
    /// small under a sustained sub-bass.
    pub onset: Vec<f32>,
    /// Beat model activation at the beat. It keeps firing through breakdowns
    /// (about 1.0 on most bars of a club track), so it only marks where the
    /// music starts.
    pub pulse: Vec<f32>,
}

fn bands(lo: f32, hi: f32) -> Vec<usize> {
    let centers = MelFrontend::band_centers_hz();
    (0..N_MELS)
        .filter(|m| centers[*m] >= lo && centers[*m] < hi)
        .collect()
}

pub fn beat_features(act: &Activations, grid: &Grid, beats: usize) -> BeatFeatures {
    let n = act.mels.len();
    let (low_bands, high_bands) = (bands(0.0, 150.0), bands(2_000.0, 20_000.0));
    let mean = |k: usize, b: &[usize]| {
        b.iter().map(|m| act.mels[k][*m]).sum::<f32>() / b.len() as f32
    };
    let low_flux = |k: usize| {
        if k == 0 {
            return 0.0;
        }
        low_bands
            .iter()
            .map(|m| (act.mels[k][*m] - act.mels[k - 1][*m]).max(0.0))
            .sum::<f32>()
            / low_bands.len() as f32
    };
    let frame = |beat: usize| {
        let frame = act.ms_to_frame(grid.beat_ms(beat as i64)).round().max(0.0) as usize;
        frame.min(n)
    };

    let mut f = BeatFeatures {
        loud: Vec::with_capacity(beats),
        low: Vec::with_capacity(beats),
        high: Vec::with_capacity(beats),
        onset: Vec::with_capacity(beats),
        pulse: Vec::with_capacity(beats),
    };
    for beat in 0..beats {
        let (lo, hi) = (frame(beat), frame(beat + 1).max(frame(beat) + 1).min(n));
        let span = (lo..hi).filter(|k| *k < n);
        let count = span.clone().count().max(1) as f32;
        let loud = |k: usize| act.mels[k].iter().sum::<f32>() / N_MELS as f32;
        f.loud.push(span.clone().map(loud).sum::<f32>() / count);
        f.low.push(span.clone().map(|k| mean(k, &low_bands)).sum::<f32>() / count);
        f.high.push(span.map(|k| mean(k, &high_bands)).sum::<f32>() / count);
        let attack = lo.saturating_sub(1)..(lo + 4).min(n);
        f.onset.push(attack.map(low_flux).fold(0.0, f32::max));
        // The activation peaks a few frames after the attack.
        let around = lo.saturating_sub(1)..(lo + 6).min(n);
        f.pulse.push(around.map(|k| act.beat[k]).fold(0.0, f32::max));
    }
    f
}

fn percentile(v: &[f32], q: f32) -> f32 {
    if v.is_empty() {
        return 0.0;
    }
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.total_cmp(b));
    s[((s.len() - 1) as f32 * q).round() as usize]
}

/// Scales `v` so its 95th percentile maps to 1 and its 5th to 0, but over at
/// least `floor`: a feature that hardly changes over the track (kick and bass
/// in every bar) stays near 1 instead of having its noise stretched to 0..1.
fn normalize(v: &[f32], floor: f32) -> Vec<f32> {
    let hi = percentile(v, 0.95);
    let lo = percentile(v, 0.05).min(hi - floor);
    v.iter().map(|x| ((x - lo) / (hi - lo)).clamp(0.0, 1.0)).collect()
}

/// Minimum spread of the log-mel levels (natural log units; 0.5 is about
/// 4 dB) and of the low-band onsets before they count as changing.
const LEVEL_FLOOR: f32 = 0.5;
const ONSET_FLOOR: f32 = 0.1;

/// Mean change of the features from the 4 beats before each beat to the 4
/// beats from it on, summed per beat phase. Returns the sums.
pub fn downbeat_scores(f: &BeatFeatures) -> [f64; BPB] {
    const W: usize = 4;
    let feats = [
        normalize(&f.loud, LEVEL_FLOOR),
        normalize(&f.low, LEVEL_FLOOR),
        normalize(&f.high, LEVEL_FLOOR),
        normalize(&f.onset, ONSET_FLOOR),
    ];
    let n = f.loud.len();
    let mut scores = [0.0f64; BPB];
    for b in W..n.saturating_sub(W) {
        let change: f64 = feats
            .iter()
            .map(|v| {
                let before: f32 = v[b - W..b].iter().sum::<f32>() / W as f32;
                let after: f32 = v[b..b + W].iter().sum::<f32>() / W as f32;
                (after - before).abs() as f64
            })
            .sum();
        // Squared: a drop or breakdown says more than a fill.
        scores[b % BPB] += change * change;
    }
    scores
}

/// The first beat the model hears, which in a DJ edit is nearly always bar 1.
pub fn music_start(f: &BeatFeatures) -> usize {
    f.pulse.iter().position(|p| *p > 0.3).unwrap_or(0)
}

/// Picks the downbeat phase from the change scores, with the music start's
/// phase favored by [`MUSIC_START_PRIOR`]; returns it and its margin over the
/// runner-up. A track with a pickup still wins on clear changes.
pub fn pick_downbeat(scores: &[f64; BPB], music_start: usize) -> (u32, f32) {
    let mut weighted = *scores;
    weighted[music_start % BPB] *= MUSIC_START_PRIOR;
    let mut order: Vec<usize> = (0..BPB).collect();
    order.sort_by(|a, b| weighted[*b].total_cmp(&weighted[*a]));
    let margin = weighted[order[0]] / weighted[order[1]].max(1e-12);
    (order[0] as u32, margin as f32)
}

/// Raw per-bar means; bar `n` covers beats `downbeat + 4n ..+4`.
pub struct Bars {
    pub energy: Vec<f32>,
    pub low: Vec<f32>,
    pub high: Vec<f32>,
    pub onset: Vec<f32>,
}

pub fn bars(f: &BeatFeatures, downbeat: u32) -> Bars {
    let n = f.loud.len();
    let start = downbeat as usize;
    let count = n.saturating_sub(start) / BPB;
    let per_bar = |v: &[f32]| -> Vec<f32> {
        (0..count)
            .map(|bar| {
                let b = start + bar * BPB;
                v[b..b + BPB].iter().sum::<f32>() / BPB as f32
            })
            .collect()
    };
    Bars {
        energy: per_bar(&f.loud),
        low: per_bar(&f.low),
        high: per_bar(&f.high),
        onset: per_bar(&f.onset),
    }
}

impl Bars {
    /// Kick presence per bar, 0..1: the bass is in, or a clear low-band
    /// attack (a kick on its own in an intro, where there is no bass yet).
    /// Club breakdowns drop kick and bass together.
    pub fn kick(&self) -> Vec<f32> {
        let strong = percentile(&self.onset, 0.95).max(1e-6);
        let low = normalize(&self.low, LEVEL_FLOOR);
        self.onset
            .iter()
            .zip(&low)
            .map(|(o, l)| ((o / strong - 0.3) / 0.5).clamp(0.0, 1.0).max(*l))
            .collect()
    }

    pub fn features(&self) -> BarFeatures {
        let q = |v: &[f32]| v.iter().map(|x| (x.clamp(0.0, 1.0) * 255.0).round() as u8).collect();
        BarFeatures {
            energy: q(&normalize(&self.energy, LEVEL_FLOOR)),
            low: q(&normalize(&self.low, LEVEL_FLOOR)),
            high: q(&normalize(&self.high, LEVEL_FLOOR)),
            kick: q(&self.kick()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Quiet,
    Groove,
    Peak,
}

const STATES: [State; 3] = [State::Quiet, State::Groove, State::Peak];

fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}

/// Per-bar state sequence by Viterbi decoding, and its mean cost per bar.
fn decode_states(bars: &Bars) -> (Vec<State>, f32) {
    let n = bars.energy.len();
    if n == 0 {
        return (Vec::new(), 0.0);
    }
    let kicks = bars.kick();
    let energy = normalize(&bars.energy, LEVEL_FLOOR);
    let low = normalize(&bars.low, LEVEL_FLOOR);
    let cost = |bar: usize, state: State| -> f32 {
        let kick = sigmoid((kicks[bar] - 0.4) / 0.08).clamp(1e-4, 1.0 - 1e-4);
        let full = 0.6 * energy[bar] + 0.4 * low[bar];
        let high = sigmoid((full - 0.72) / 0.06).clamp(1e-4, 1.0 - 1e-4);
        match state {
            State::Quiet => -(1.0 - kick).ln(),
            State::Groove => -kick.ln() - (1.0 - high).ln(),
            State::Peak => -kick.ln() - high.ln(),
        }
    };
    // Club tracks are built from 4-, 8- and 16-bar phrases counted from the
    // first downbeat (DJ edits start on bar 1). Switching elsewhere needs
    // strong evidence, such as a drop that comes in early.
    let switch = |bar: usize| -> f32 {
        if bar % 16 == 0 {
            1.0
        } else if bar % 8 == 0 {
            1.5
        } else if bar % 4 == 0 {
            3.0
        } else {
            12.0
        }
    };

    let mut total: Vec<[f32; 3]> = vec![[0.0; 3]; n];
    let mut from: Vec<[usize; 3]> = vec![[0; 3]; n];
    for (s, state) in STATES.iter().enumerate() {
        total[0][s] = cost(0, *state);
    }
    for bar in 1..n {
        for (s, state) in STATES.iter().enumerate() {
            let (best, value) = (0..3)
                .map(|p| (p, total[bar - 1][p] + if p == s { 0.0 } else { switch(bar) }))
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .unwrap();
            total[bar][s] = value + cost(bar, *state);
            from[bar][s] = best;
        }
    }
    let mut s = (0..3).min_by(|a, b| total[n - 1][*a].total_cmp(&total[n - 1][*b])).unwrap();
    let cost = total[n - 1][s] / n as f32;
    let mut out = vec![State::Quiet; n];
    for bar in (0..n).rev() {
        out[bar] = STATES[s];
        s = from[bar][s];
    }
    (out, cost)
}

/// Runs of equal state as (state, first bar, end bar), with runs shorter
/// than [`MIN_RUN_BARS`] merged into the run before them (the first one into
/// the run after it).
fn runs(states: &[State]) -> Vec<(State, usize, usize)> {
    let mut runs: Vec<(State, usize, usize)> = Vec::new();
    for (bar, state) in states.iter().enumerate() {
        match runs.last_mut() {
            Some((s, _, end)) if s == state => *end = bar + 1,
            _ => runs.push((*state, bar, bar + 1)),
        }
    }
    while let Some(i) = runs.iter().position(|r| r.2 - r.1 < MIN_RUN_BARS) {
        if runs.len() == 1 {
            break;
        }
        let (_, start, end) = runs.remove(i);
        if i == 0 {
            runs[0].1 = start;
        } else {
            runs[i - 1].2 = end;
        }
        join_equal(&mut runs);
    }
    for run in runs.iter_mut() {
        if run.0 == State::Peak && run.2 - run.1 < MIN_DROP_BARS {
            run.0 = State::Groove;
        }
    }
    join_equal(&mut runs);
    runs
}

fn join_equal(runs: &mut Vec<(State, usize, usize)>) {
    let mut i = 1;
    while i < runs.len() {
        if runs[i - 1].0 == runs[i].0 {
            let (_, _, end) = runs.remove(i);
            runs[i - 1].2 = end;
        } else {
            i += 1;
        }
    }
}

/// Labels runs of bar states as sections. `beats` is the grid length.
pub fn sections(bars: &Bars, downbeat: u32, beats: u32) -> Vec<Section> {
    let runs = runs(&decode_states(bars).0);
    let high = normalize(&bars.high, LEVEL_FLOOR);

    // (kind, first bar, end bar)
    let mut labeled: Vec<(SectionKind, usize, usize)> = Vec::new();
    let last = runs.len().saturating_sub(1);
    for (i, &(state, start, end)) in runs.iter().enumerate() {
        let next_is_drop = runs.get(i + 1).is_some_and(|r| r.0 == State::Peak);
        let kind = match state {
            State::Peak => SectionKind::Drop,
            _ if i == 0 => SectionKind::Intro,
            _ if i == last && runs.len() > 1 => SectionKind::Outro,
            State::Quiet => SectionKind::Breakdown,
            State::Groove => SectionKind::Groove,
        };
        if next_is_drop && kind != SectionKind::Drop {
            // The bars before a drop are its build: always after a quiet
            // part, after a groove only when the highs rise into it.
            let len = (end - start).min(BUILD_BARS);
            let build_start = end - len;
            let rising = len >= 4 && {
                let half = len / 2;
                let mean = |bars: &[f32]| bars.iter().sum::<f32>() / half as f32;
                let first = mean(&high[build_start..build_start + half]);
                let second = mean(&high[end - half..end]);
                second > first + 0.08
            };
            if state == State::Quiet || rising {
                if build_start > start {
                    labeled.push((kind, start, build_start));
                }
                labeled.push((SectionKind::Build, build_start, end));
                continue;
            }
        }
        labeled.push((kind, start, end));
    }

    let bar_beat = |bar: usize| downbeat + (bar * BPB) as u32;
    let count = labeled.len();
    labeled
        .iter()
        .enumerate()
        .map(|(i, &(kind, start, end))| Section {
            kind,
            start: if i == 0 { 0 } else { bar_beat(start) },
            end: if i + 1 == count { beats } else { bar_beat(end) },
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bars from a compact description: per section (bars, kick and bass
    /// in, loudness, highs), levels in log-mel units like real tracks.
    fn synthetic(parts: &[(usize, bool, f32, f32)]) -> Bars {
        let mut b = Bars {
            energy: Vec::new(),
            low: Vec::new(),
            high: Vec::new(),
            onset: Vec::new(),
        };
        let mut seed = 7u32;
        let mut noise = || {
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            (seed >> 16) as f32 / 65_536.0 * 0.04
        };
        for &(count, kick, energy, high) in parts {
            for _ in 0..count {
                b.onset.push(if kick { 0.3 } else { 0.03 } + noise());
                b.low.push(if kick { 4.7 } else { 2.5 } + noise());
                b.energy.push(energy + noise());
                b.high.push(high + noise());
            }
        }
        b
    }

    fn kinds(s: &[Section]) -> Vec<(SectionKind, u32)> {
        s.iter().map(|s| (s.kind, (s.end - s.start) / 4)).collect()
    }

    #[test]
    fn labels_a_typical_club_track() {
        use SectionKind::*;
        let bars = synthetic(&[
            (32, true, 1.9, 1.3),  // intro with kick
            (16, false, 1.5, 1.2), // breakdown
            (8, false, 1.7, 1.9),  // riser
            (32, true, 2.5, 1.7),  // drop
            (16, false, 1.5, 1.2), // breakdown
            (32, true, 2.5, 1.7),  // drop
            (16, true, 1.9, 1.3),  // outro with kick
        ]);
        let s = sections(&bars, 0, 152 * 4);
        assert_eq!(
            kinds(&s),
            vec![
                (Intro, 32),
                (Breakdown, 16),
                (Build, 8),
                (Drop, 32),
                (Breakdown, 8),
                (Build, 8),
                (Drop, 32),
                (Outro, 16)
            ]
        );
        assert_eq!(s[0].start, 0);
        assert_eq!(s[3].start, 56 * 4);
        assert_eq!(s.last().unwrap().end, 152 * 4);
    }

    #[test]
    fn a_groove_only_becomes_a_build_when_the_highs_rise() {
        use SectionKind::*;
        let flat = synthetic(&[(32, true, 1.9, 1.3), (32, true, 2.5, 1.7)]);
        assert_eq!(kinds(&sections(&flat, 0, 64 * 4)), vec![(Intro, 32), (Drop, 32)]);
        let mut rising = synthetic(&[(32, true, 1.9, 1.3), (32, true, 2.5, 1.7)]);
        for bar in 28..32 {
            rising.high[bar] = 2.0;
        }
        assert_eq!(
            kinds(&sections(&rising, 0, 64 * 4)),
            vec![(Intro, 24), (Build, 8), (Drop, 32)]
        );
    }

    #[test]
    fn kick_and_bass_everywhere_is_not_stretched_into_breakdowns() {
        let bars = synthetic(&[(64, true, 2.5, 1.7)]);
        assert!(bars.kick().iter().all(|k| *k > 0.9));
        assert_eq!(kinds(&sections(&bars, 0, 64 * 4)), vec![(SectionKind::Drop, 64)]);
    }

    #[test]
    fn downbeat_is_where_things_change() {
        // Kick on for 8 bars, off for 8, on again, with the bars starting on
        // beat 2 of the grid.
        let mut f = BeatFeatures {
            loud: Vec::new(),
            low: Vec::new(),
            high: Vec::new(),
            onset: Vec::new(),
            pulse: Vec::new(),
        };
        let offset: usize = 2;
        for beat in 0..(offset + 96) {
            let bar = beat.saturating_sub(offset) / 4;
            let on = beat >= offset && (bar / 8) % 2 == 0;
            f.onset.push(if on { 1.0 } else { 0.1 });
            f.pulse.push(if on { 1.0 } else { 0.1 });
            f.low.push(if on { 1.0 } else { 0.2 });
            f.loud.push(if on { 0.8 } else { 0.5 });
            f.high.push(0.5);
        }
        // The music starts on beat 0, but the changes say beat 2.
        let (downbeat, margin) = pick_downbeat(&downbeat_scores(&f), 0);
        assert_eq!(downbeat, offset as u32);
        assert!(margin > 1.3, "{margin}");
        // Without changes, the music start decides.
        assert_eq!(pick_downbeat(&[1.0; 4], 3), (3, 1.2));
    }
}
