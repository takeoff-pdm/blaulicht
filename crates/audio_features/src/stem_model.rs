//! Streaming stem separation (drums, bass, other, vocals) with the student
//! trained in `audio-model/stems/` (see `audio-model/stems/student.py`).
//!
//! Mono 44.1 kHz PCM goes in one hop (441 samples, 10 ms) at a time. Each
//! hop yields one analysis frame: the last 2048 samples, Hann-windowed, as
//! in [`crate::MelFrontend`]. The network sees `lookahead` frames past the
//! frame it separates, so frame `g` is separated once frame `g + lookahead`
//! has arrived. Per separated frame the model reports each stem's spectral
//! energy (what a light show needs, no inverse FFT) and, on request, the
//! stem audio by overlap-add.
//!
//! Latency from a sample entering to its stems coming out: `n_fft +
//! lookahead * hop` samples (46 ms + lookahead * 10 ms), plus the block size.
//!
//! Weights come from `audio-model/stems/export_stems.py`, little-endian:
//! header `STEM`, format, n_fft, hop, channels, kernel, mix channels,
//! sources, block count, dilations, right pads, band count, band edges;
//! then f32 arrays in PyTorch layout: feature mean and std, per band the
//! encoder, per block (time: norm, conv, mix; band mix: norm, down,
//! across, up), final norm, per band the decoder.

use std::{path::Path, sync::Arc};

use rustfft::{num_complex::Complex, Fft, FftPlanner};

const MAGIC: &[u8; 4] = b"STEM";
const FORMAT_VERSION: u32 = 1;
const LAYER_NORM_EPS: f32 = 1e-5;
pub const SAMPLE_RATE: u32 = 44_100;
pub const SOURCES: usize = 4;
pub const SOURCE_NAMES: [&str; SOURCES] = ["drums", "bass", "other", "vocals"];

#[derive(Clone, Debug, PartialEq)]
pub struct StemInfo {
    pub n_fft: usize,
    pub hop: usize,
    pub channels: usize,
    pub lookahead: usize,
    pub dilations: Vec<usize>,
    pub bands: usize,
}

impl StemInfo {
    /// Samples from input to output, excluding the caller's block size.
    pub fn latency_samples(&self) -> usize {
        self.n_fft + self.lookahead * self.hop
    }
}

/// One separated analysis frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct StemFrame {
    /// Index of the analysis frame (one per hop since the stream start).
    pub frame: u64,
    /// Sum over bins of |mask * X|^2 per stem. The four add up to at most
    /// `mix_energy`; compare stems by ratio or in dB.
    pub energy: [f32; SOURCES],
    pub mix_energy: f32,
}

pub struct StemModel {
    info: StemInfo,
    edges: Vec<(usize, usize)>,
    feat_mean: Vec<f32>,
    feat_std: Vec<f32>,
    encode: Vec<Linear>,
    time: Vec<TimeLayer>,
    mix: Vec<BandMix>,
    out_norm: LayerNorm,
    decode: Vec<Linear>,
    // Signal path.
    window: Vec<f32>,
    fft: Arc<dyn Fft<f32>>,
    ifft: Arc<dyn Fft<f32>>,
    history: Vec<f32>,
    write: usize,
    since_frame: usize,
    frames_in: u64,
    /// Spectra of the last `lookahead + 1` frames, oldest at `spec_head`.
    specs: Vec<Vec<Complex<f32>>>,
    spec_head: usize,
    spec_count: usize,
    ola: Vec<Vec<f32>>,
    ola_norm: Vec<f32>,
    frames_out: u64,
    // Scratch.
    buf: Vec<Complex<f32>>,
    feat: Vec<f32>,
    x: Vec<f32>,
    masks: Vec<f32>,
}

impl StemModel {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, String> {
        let path = path.as_ref();
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Self::from_bytes(&bytes)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        let mut r = Reader { bytes, pos: 0 };
        if r.take(4)? != MAGIC {
            return Err("not a stem model file".into());
        }
        let format = r.usize()? as u32;
        if format != FORMAT_VERSION {
            return Err(format!("stem model format {format}, expected {FORMAT_VERSION}"));
        }
        let n_fft = r.usize()?;
        let hop = r.usize()?;
        let channels = r.usize()?;
        let kernel = r.usize()?;
        if kernel != 3 {
            return Err(format!("time kernel {kernel}; only 3 is supported"));
        }
        let mix_channels = r.usize()?;
        let sources = r.usize()?;
        if sources != SOURCES {
            return Err(format!("{sources} sources, expected {SOURCES}"));
        }
        let n_blocks = r.usize()?;
        let dilations = (0..n_blocks).map(|_| r.usize()).collect::<Result<Vec<_>, _>>()?;
        let pads = (0..n_blocks).map(|_| r.usize()).collect::<Result<Vec<_>, _>>()?;
        let n_bands = r.usize()?;
        let edges = (0..n_bands)
            .map(|_| Ok((r.usize()?, r.usize()?)))
            .collect::<Result<Vec<_>, String>>()?;
        let bins = n_fft / 2 + 1;
        if edges.first().map(|e| e.0) != Some(0) || edges.last().map(|e| e.1) != Some(bins) {
            return Err("band edges do not cover the spectrum".into());
        }

        let feat_mean = r.f32s(bins)?;
        let feat_std = r.f32s(bins)?;
        let encode = edges
            .iter()
            .map(|&(lo, hi)| Linear::read(&mut r, hi - lo, channels))
            .collect::<Result<Vec<_>, _>>()?;
        let mut time = Vec::new();
        let mut mix = Vec::new();
        for (&d, &right) in dilations.iter().zip(&pads) {
            if right > 2 * d {
                return Err(format!("right pad {right} exceeds the reach of dilation {d}"));
            }
            time.push(TimeLayer {
                norm: LayerNorm::read(&mut r, channels)?,
                conv: Linear::read(&mut r, 3 * channels, channels)?,
                mix: Linear::read(&mut r, channels, channels)?,
                dilation: d,
                right,
                normed: Ring::new(2 * d + 1, n_bands * channels),
                raw: Ring::new(right + 1, n_bands * channels),
                seen: 0,
                taps: vec![0.0; 3 * channels],
                h: vec![0.0; channels],
                out: vec![0.0; n_bands * channels],
            });
            mix.push(BandMix {
                norm: LayerNorm::read(&mut r, channels)?,
                down: Linear::read(&mut r, channels, mix_channels)?,
                across: Linear::read(&mut r, n_bands * mix_channels, n_bands * mix_channels)?,
                up: Linear::read(&mut r, mix_channels, channels)?,
                a: vec![0.0; channels],
                squeezed: vec![0.0; n_bands * mix_channels],
                mixed: vec![0.0; n_bands * mix_channels],
                b: vec![0.0; channels],
            });
        }
        let out_norm = LayerNorm::read(&mut r, channels)?;
        let decode = edges
            .iter()
            .map(|&(lo, hi)| Linear::read(&mut r, channels, (hi - lo) * SOURCES))
            .collect::<Result<Vec<_>, _>>()?;
        if r.pos != bytes.len() {
            return Err(format!("{} trailing bytes in stem model file", bytes.len() - r.pos));
        }

        // Periodic Hann, as torch.hann_window.
        let window: Vec<f32> = (0..n_fft)
            .map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / n_fft as f32).cos())
            .collect();
        // Steady-state overlap-add normalisation of the first hop of a frame.
        let mut ola_norm = vec![0.0; hop];
        for start in (0..n_fft).step_by(hop) {
            for (j, w) in window[start..(start + hop).min(n_fft)].iter().enumerate() {
                ola_norm[j] += w * w;
            }
        }
        let lookahead = pads.iter().sum();
        let mut planner = FftPlanner::new();
        Ok(Self {
            info: StemInfo { n_fft, hop, channels, lookahead, dilations, bands: n_bands },
            feat_mean,
            feat_std,
            encode,
            time,
            mix,
            out_norm,
            decode,
            window,
            fft: planner.plan_fft_forward(n_fft),
            ifft: planner.plan_fft_inverse(n_fft),
            history: vec![0.0; n_fft],
            write: 0,
            since_frame: 0,
            frames_in: 0,
            specs: vec![vec![Complex::default(); bins]; lookahead + 1],
            spec_head: 0,
            spec_count: 0,
            ola: vec![vec![0.0; n_fft]; SOURCES],
            ola_norm,
            frames_out: 0,
            buf: vec![Complex::default(); n_fft],
            feat: vec![0.0; bins],
            x: vec![0.0; n_bands * channels],
            masks: vec![0.0; bins * SOURCES],
            edges,
        })
    }

    pub fn info(&self) -> &StemInfo {
        &self.info
    }

    /// Forget all past audio, as at the start of a stream.
    pub fn reset(&mut self) {
        self.history.fill(0.0);
        self.write = 0;
        self.since_frame = 0;
        self.frames_in = 0;
        self.spec_head = 0;
        self.spec_count = 0;
        self.frames_out = 0;
        self.ola.iter_mut().for_each(|o| o.fill(0.0));
        for t in &mut self.time {
            t.normed.clear();
            t.raw.clear();
            t.seen = 0;
        }
    }

    /// Feed mono 44.1 kHz PCM. Appends one [`StemFrame`] per separated
    /// frame; with `audio`, also appends `hop` samples per stem and frame.
    pub fn push(
        &mut self,
        samples: &[f32],
        frames: &mut Vec<StemFrame>,
        mut audio: Option<&mut [Vec<f32>; SOURCES]>,
    ) {
        for &sample in samples {
            self.history[self.write] = sample;
            self.write = (self.write + 1) % self.history.len();
            self.since_frame += 1;
            if self.since_frame == self.info.hop {
                self.since_frame = 0;
                if let Some(frame) = self.frame(audio.as_deref_mut()) {
                    frames.push(frame);
                }
            }
        }
    }

    fn frame(&mut self, audio: Option<&mut [Vec<f32>; SOURCES]>) -> Option<StemFrame> {
        let n_fft = self.info.n_fft;
        let bins = n_fft / 2 + 1;
        let c = self.info.channels;
        // Analysis: the last n_fft samples, oldest first.
        let (oldest, newest) = self.history.split_at(self.write);
        for ((slot, s), w) in self.buf.iter_mut().zip(newest.iter().chain(oldest)).zip(&self.window) {
            *slot = Complex::new(s * w, 0.0);
        }
        self.fft.process(&mut self.buf);
        let slot = (self.spec_head + self.spec_count) % self.specs.len();
        self.specs[slot].copy_from_slice(&self.buf[..bins]);
        if self.spec_count < self.specs.len() {
            self.spec_count += 1;
        } else {
            self.spec_head = (self.spec_head + 1) % self.specs.len();
        }
        self.frames_in += 1;

        for (k, f) in self.feat.iter_mut().enumerate() {
            *f = ((self.buf[k].norm() + 1e-5).ln() - self.feat_mean[k]) / self.feat_std[k];
        }
        for (b, (enc, &(lo, hi))) in self.encode.iter().zip(&self.edges).enumerate() {
            enc.apply(&self.feat[lo..hi], &mut self.x[b * c..][..c]);
        }

        // The network: layer i lags its input by its right pad.
        for (t, m) in self.time.iter_mut().zip(&mut self.mix) {
            if !t.step(&self.x, c) {
                return None;
            }
            self.x.copy_from_slice(&t.out);
            m.apply(&mut self.x, c);
        }

        // Heads: per band and bin, a softmax over the stems.
        let mut a = vec![0.0; c];
        for (b, (dec, &(lo, hi))) in self.decode.iter().zip(&self.edges).enumerate() {
            self.out_norm.apply(&self.x[b * c..][..c], &mut a);
            let out = &mut self.masks[lo * SOURCES..hi * SOURCES];
            dec.apply(&a, out);
            for m in out.chunks_exact_mut(SOURCES) {
                let max = m.iter().fold(f32::NEG_INFINITY, |a, &b| a.max(b));
                let mut sum = 0.0;
                m.iter_mut().for_each(|v| {
                    *v = (*v - max).exp();
                    sum += *v;
                });
                m.iter_mut().for_each(|v| *v /= sum);
            }
        }

        // The masks belong to the oldest stored spectrum.
        let spec = &self.specs[self.spec_head];
        let mut frame = StemFrame { frame: self.frames_out, ..Default::default() };
        for (k, x) in spec.iter().enumerate() {
            let p = x.norm_sqr();
            frame.mix_energy += p;
            for s in 0..SOURCES {
                let m = self.masks[k * SOURCES + s];
                frame.energy[s] += m * m * p;
            }
        }
        if let Some(audio) = audio {
            let hop = self.info.hop;
            for s in 0..SOURCES {
                for (k, x) in spec.iter().enumerate() {
                    self.buf[k] = x * self.masks[k * SOURCES + s];
                }
                for k in 1..n_fft - bins + 1 {
                    self.buf[n_fft - k] = self.buf[k].conj();
                }
                self.ifft.process(&mut self.buf);
                let ola = &mut self.ola[s];
                for ((o, y), w) in ola.iter_mut().zip(&self.buf).zip(&self.window) {
                    *o += y.re / n_fft as f32 * w;
                }
                audio[s].extend(ola[..hop].iter().zip(&self.ola_norm).map(|(o, n)| o / n));
                ola.copy_within(hop.., 0);
                ola[n_fft - hop..].fill(0.0);
            }
        }
        self.frames_out += 1;
        Some(frame)
    }
}

/// Dilated kernel-3 conv over time with `right` frames of lookahead,
/// pre-norm and residual, applied to every band with shared weights.
struct TimeLayer {
    norm: LayerNorm,
    conv: Linear,
    mix: Linear,
    dilation: usize,
    right: usize,
    normed: Ring,
    raw: Ring,
    seen: usize,
    taps: Vec<f32>,
    h: Vec<f32>,
    out: Vec<f32>,
}

impl TimeLayer {
    /// Feed input frame u (all bands); writes output frame u - right to
    /// `out` and returns true once that frame exists.
    fn step(&mut self, x: &[f32], c: usize) -> bool {
        let slot = self.normed.push();
        for (xb, nb) in x.chunks_exact(c).zip(slot.chunks_exact_mut(c)) {
            self.norm.apply(xb, nb);
        }
        self.raw.push().copy_from_slice(x);
        self.seen += 1;
        if self.seen <= self.right {
            return false;
        }
        let d = self.dilation;
        let bands = x.len() / c;
        for b in 0..bands {
            for (j, back) in [2 * d, d, 0].into_iter().enumerate() {
                self.taps[j * c..][..c].copy_from_slice(&self.normed.back(back)[b * c..][..c]);
            }
            self.conv.apply(&self.taps, &mut self.h);
            self.h.iter_mut().for_each(|v| *v = gelu(*v));
            let out = &mut self.out[b * c..][..c];
            self.mix.apply(&self.h, out);
            for (o, r) in out.iter_mut().zip(&self.raw.back(self.right)[b * c..][..c]) {
                *o += r;
            }
        }
        true
    }
}

/// Per frame: squeeze each band, mix all bands with one linear layer,
/// expand back, residual.
struct BandMix {
    norm: LayerNorm,
    down: Linear,
    across: Linear,
    up: Linear,
    a: Vec<f32>,
    squeezed: Vec<f32>,
    mixed: Vec<f32>,
    b: Vec<f32>,
}

impl BandMix {
    fn apply(&mut self, x: &mut [f32], c: usize) {
        let m = self.down.cout;
        for (b, xb) in x.chunks_exact(c).enumerate() {
            self.norm.apply(xb, &mut self.a);
            self.down.apply(&self.a, &mut self.squeezed[b * m..][..m]);
        }
        self.across.apply(&self.squeezed, &mut self.mixed);
        self.mixed.iter_mut().for_each(|v| *v = gelu(*v));
        for (b, xb) in x.chunks_exact_mut(c).enumerate() {
            self.up.apply(&self.mixed[b * m..][..m], &mut self.b);
            xb.iter_mut().zip(&self.b).for_each(|(x, y)| *x += y);
        }
    }
}

/// Fixed-size ring of frames of `width` values.
struct Ring {
    data: Vec<f32>,
    width: usize,
    newest: usize,
}

impl Ring {
    fn new(frames: usize, width: usize) -> Self {
        Self { data: vec![0.0; frames * width], width, newest: frames - 1 }
    }

    fn frames(&self) -> usize {
        self.data.len() / self.width
    }

    fn clear(&mut self) {
        self.data.fill(0.0);
    }

    /// Advance and return the slot of the new newest frame.
    fn push(&mut self) -> &mut [f32] {
        self.newest = (self.newest + 1) % self.frames();
        &mut self.data[self.newest * self.width..][..self.width]
    }

    /// The frame `back` frames before the newest.
    fn back(&self, back: usize) -> &[f32] {
        let i = (self.newest + self.frames() - back) % self.frames();
        &self.data[i * self.width..][..self.width]
    }
}

/// `y = W x + b`, PyTorch `nn.Linear` layout (`W` is `[out][in]`).
struct Linear {
    cin: usize,
    cout: usize,
    weight: Vec<f32>,
    bias: Vec<f32>,
}

impl Linear {
    fn read(r: &mut Reader, cin: usize, cout: usize) -> Result<Self, String> {
        Ok(Self { cin, cout, weight: r.f32s(cin * cout)?, bias: r.f32s(cout)? })
    }

    fn apply(&self, x: &[f32], out: &mut [f32]) {
        debug_assert_eq!((x.len(), out.len()), (self.cin, self.cout));
        for ((o, row), b) in out.iter_mut().zip(self.weight.chunks_exact(self.cin)).zip(&self.bias) {
            *o = dot(row, x) + b;
        }
    }
}

struct LayerNorm {
    weight: Vec<f32>,
    bias: Vec<f32>,
}

impl LayerNorm {
    fn read(r: &mut Reader, channels: usize) -> Result<Self, String> {
        Ok(Self { weight: r.f32s(channels)?, bias: r.f32s(channels)? })
    }

    fn apply(&self, x: &[f32], out: &mut [f32]) {
        let n = x.len() as f32;
        let mean = x.iter().sum::<f32>() / n;
        let var = x.iter().map(|v| (v - mean) * (v - mean)).sum::<f32>() / n;
        let scale = 1.0 / (var + LAYER_NORM_EPS).sqrt();
        for (((out, x), w), b) in out.iter_mut().zip(x).zip(&self.weight).zip(&self.bias) {
            *out = (x - mean) * scale * w + b;
        }
    }
}

/// Eight independent accumulators so the compiler can vectorize the sum.
fn dot(a: &[f32], b: &[f32]) -> f32 {
    let mut acc = [0.0f32; 8];
    let (a8, a_rest) = a.split_at(a.len() / 8 * 8);
    let (b8, b_rest) = b.split_at(a8.len());
    for (a, b) in a8.chunks_exact(8).zip(b8.chunks_exact(8)) {
        for k in 0..8 {
            acc[k] += a[k] * b[k];
        }
    }
    acc.iter().sum::<f32>() + a_rest.iter().zip(b_rest).map(|(a, b)| a * b).sum::<f32>()
}

/// Exact (erf-based) GELU, as `torch.nn.functional.gelu` computes it.
fn gelu(x: f32) -> f32 {
    0.5 * x * (1.0 + erf(x * std::f32::consts::FRAC_1_SQRT_2))
}

/// Abramowitz & Stegun 7.1.26; absolute error below 1.5e-7.
fn erf(x: f32) -> f32 {
    let t = 1.0 / (1.0 + 0.327_591_1 * x.abs());
    let poly = ((((1.061_405_4 * t - 1.453_152) * t + 1.421_413_8) * t - 0.284_496_74) * t
        + 0.254_829_6)
        * t;
    (1.0 - poly * (-x * x).exp()).copysign(x)
}

struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8], String> {
        let end = self.pos + n;
        let slice = self.bytes.get(self.pos..end).ok_or("stem model file is truncated")?;
        self.pos = end;
        Ok(slice)
    }

    fn usize(&mut self) -> Result<usize, String> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()) as usize)
    }

    fn f32s(&mut self, n: usize) -> Result<Vec<f32>, String> {
        Ok(self
            .take(4 * n)?
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tiny random model: 2 bands over n_fft 8, 2 blocks.
    fn tiny(lookahead_pads: [u32; 2]) -> Vec<u8> {
        let (n_fft, hop, c, m, dil) = (8u32, 2u32, 4u32, 2u32, [1u32, 2]);
        let edges = [(0u32, 3u32), (3, 5)];
        let mut bytes = MAGIC.to_vec();
        let u = |v: u32, b: &mut Vec<u8>| b.extend_from_slice(&v.to_le_bytes());
        for v in [FORMAT_VERSION, n_fft, hop, c, 3, m, SOURCES as u32, 2] {
            u(v, &mut bytes);
        }
        dil.iter().for_each(|&d| u(d, &mut bytes));
        lookahead_pads.iter().for_each(|&p| u(p, &mut bytes));
        u(edges.len() as u32, &mut bytes);
        for (lo, hi) in edges {
            u(lo, &mut bytes);
            u(hi, &mut bytes);
        }
        let mut seed = 0x1234_5678u32;
        let mut f = |n: u32, b: &mut Vec<u8>| {
            for _ in 0..n {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                b.extend_from_slice(&((seed as f32 / u32::MAX as f32 - 0.5) * 0.5).to_le_bytes());
            }
        };
        let bins = n_fft / 2 + 1;
        f(2 * bins, &mut bytes);
        for (lo, hi) in edges {
            f((hi - lo) * c + c, &mut bytes);
        }
        for _ in dil {
            f(2 * c + 3 * c * c + c + c * c + c, &mut bytes);
            let k = edges.len() as u32;
            f(2 * c + c * m + m + (k * m) * (k * m) + k * m + m * c + c, &mut bytes);
        }
        f(2 * c, &mut bytes);
        for (lo, hi) in edges {
            f(c * (hi - lo) * SOURCES as u32 + (hi - lo) * SOURCES as u32, &mut bytes);
        }
        bytes
    }

    #[test]
    fn loads_and_reports_lookahead() {
        let m = StemModel::from_bytes(&tiny([1, 3])).unwrap();
        assert_eq!(m.info().lookahead, 4);
        assert_eq!(m.info().latency_samples(), 8 + 4 * 2);
        assert!(StemModel::from_bytes(&tiny([1, 5])).is_err(), "pad beyond 2 * dilation");
        let mut bytes = tiny([0, 0]);
        bytes.push(0);
        assert!(StemModel::from_bytes(&bytes).is_err(), "trailing bytes");
    }

    #[test]
    fn stems_sum_to_the_mix_and_start_after_the_lookahead() {
        let mut m = StemModel::from_bytes(&tiny([1, 3])).unwrap();
        let pcm: Vec<f32> = (0..400).map(|i| ((i * 37 % 101) as f32 / 50.0 - 1.0) * 0.5).collect();
        let mut frames = Vec::new();
        let mut audio: [Vec<f32>; SOURCES] = Default::default();
        m.push(&pcm, &mut frames, Some(&mut audio));
        assert_eq!(frames.len(), 200 - 4, "first frame waits for 4 frames of lookahead");
        for f in &frames {
            let sum: f32 = f.energy.iter().sum();
            assert!(sum <= f.mix_energy * 1.0001);
        }
        // Masks sum to one, so the stems add up to the mix. Output sample i
        // is input sample i - (n_fft - hop) = i - 6; check in steady state.
        for i in 20..audio[0].len() {
            let sum: f32 = (0..SOURCES).map(|s| audio[s][i]).sum();
            assert!((sum - pcm[i - 6]).abs() < 1e-4, "{i}: {sum} vs {}", pcm[i - 6]);
        }
    }
}
