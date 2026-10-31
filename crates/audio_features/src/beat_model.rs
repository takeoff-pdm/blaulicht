//! Streaming inference for the causal beat/downbeat TCN trained in
//! `audio-model/` (see `audio-model/model.py`).
//!
//! The network runs one mel frame at a time. Each causal convolution keeps
//! the past inputs its dilated taps still need, so a step costs one frame of
//! work and the output equals the trained model run over the whole sequence.
//!
//! Weights come from `audio-model/export_model.py`, little-endian:
//! header `BTCN`, format, feature version, n_mels, channels, input kernel,
//! kernel, block count, dilations, lead ms, beat and downbeat thresholds;
//! then f32 arrays in PyTorch layout: mean, std, input conv, per block
//! (norm, conv, mix), final norm, head.

use std::path::Path;

use crate::{MelFrame, FEATURE_VERSION, N_MELS};

const MAGIC: &[u8; 4] = b"BTCN";
const FORMAT_VERSION: u32 = 1;
const LAYER_NORM_EPS: f32 = 1e-5;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BeatActivation {
    /// Probability that a beat became audible in this frame.
    pub beat: f32,
    pub downbeat: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ModelInfo {
    pub channels: usize,
    pub kernel: usize,
    pub dilations: Vec<usize>,
    /// The model was trained to fire this long before the beat.
    pub lead_ms: f32,
    /// Peak-picking thresholds that scored best on validation.
    pub beat_threshold: f32,
    pub downbeat_threshold: f32,
}

pub struct BeatModel {
    info: ModelInfo,
    mean: Vec<f32>,
    std: Vec<f32>,
    input: CausalConv,
    blocks: Vec<Block>,
    norm: LayerNorm,
    head: CausalConv,
    // Per-step scratch.
    x: Vec<f32>,
    h: Vec<f32>,
    a: Vec<f32>,
    b: Vec<f32>,
    logits: [f32; 2],
}

struct Block {
    norm: LayerNorm,
    conv: CausalConv,
    mix: CausalConv,
}

impl BeatModel {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, String> {
        let path = path.as_ref();
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Self::from_bytes(&bytes)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        let mut r = Reader { bytes, pos: 0 };
        if r.take(4)? != MAGIC {
            return Err("not a beat model file".into());
        }
        let format = r.u32()?;
        if format != FORMAT_VERSION {
            return Err(format!("model format {format}, expected {FORMAT_VERSION}"));
        }
        let feature_version = r.u32()?;
        if feature_version != FEATURE_VERSION {
            return Err(format!(
                "model was trained on front-end v{feature_version}, this is v{FEATURE_VERSION}"
            ));
        }
        let n_mels = r.u32()? as usize;
        if n_mels != N_MELS {
            return Err(format!("model expects {n_mels} mel bands, front end has {N_MELS}"));
        }
        let channels = r.u32()? as usize;
        let input_kernel = r.u32()? as usize;
        let kernel = r.u32()? as usize;
        let n_blocks = r.u32()? as usize;
        let dilations = (0..n_blocks)
            .map(|_| r.u32().map(|d| d as usize))
            .collect::<Result<Vec<_>, _>>()?;
        let info = ModelInfo {
            channels,
            kernel,
            lead_ms: r.f32()?,
            beat_threshold: r.f32()?,
            downbeat_threshold: r.f32()?,
            dilations,
        };

        let mean = r.f32s(n_mels)?;
        let std = r.f32s(n_mels)?;
        let input = CausalConv::read(&mut r, n_mels, channels, input_kernel, 1)?;
        let blocks = info
            .dilations
            .iter()
            .map(|&d| {
                Ok(Block {
                    norm: LayerNorm::read(&mut r, channels)?,
                    conv: CausalConv::read(&mut r, channels, channels, kernel, d)?,
                    mix: CausalConv::read(&mut r, channels, channels, 1, 1)?,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let norm = LayerNorm::read(&mut r, channels)?;
        let head = CausalConv::read(&mut r, channels, 2, 1, 1)?;
        if r.pos != bytes.len() {
            return Err(format!("{} trailing bytes in model file", bytes.len() - r.pos));
        }

        Ok(Self {
            info,
            mean,
            std,
            input,
            blocks,
            norm,
            head,
            x: vec![0.0; n_mels],
            h: vec![0.0; channels],
            a: vec![0.0; channels],
            b: vec![0.0; channels],
            logits: [0.0; 2],
        })
    }

    pub fn info(&self) -> &ModelInfo {
        &self.info
    }

    /// Forget all past frames, as at the start of a stream.
    pub fn reset(&mut self) {
        self.input.reset();
        for block in &mut self.blocks {
            block.conv.reset();
        }
    }

    pub fn step(&mut self, mel: &MelFrame) -> BeatActivation {
        let Self { mean, std, input, blocks, norm, head, x, h, a, b, logits, .. } = self;
        for (((x, m), mean), std) in x.iter_mut().zip(mel).zip(mean.iter()).zip(std.iter()) {
            *x = (m - mean) / std;
        }
        input.step(x, h);
        for block in blocks.iter_mut() {
            block.norm.apply(h, a);
            block.conv.step(a, b);
            b.iter_mut().for_each(|v| *v = gelu(*v));
            block.mix.step(b, a);
            h.iter_mut().zip(a.iter()).for_each(|(h, a)| *h += a);
        }
        norm.apply(h, a);
        a.iter_mut().for_each(|v| *v = gelu(*v));
        head.step(a, logits);
        BeatActivation { beat: sigmoid(logits[0]), downbeat: sigmoid(logits[1]) }
    }
}

/// A causal, dilated 1-D convolution evaluated one frame at a time.
struct CausalConv {
    cin: usize,
    cout: usize,
    kernel: usize,
    dilation: usize,
    /// `[tap][out][in]`, tap 0 being the oldest input.
    weights: Vec<f32>,
    bias: Vec<f32>,
    /// Ring buffer of the last `(kernel - 1) * dilation + 1` inputs.
    history: Vec<f32>,
    newest: usize,
}

impl CausalConv {
    fn read(
        r: &mut Reader,
        cin: usize,
        cout: usize,
        kernel: usize,
        dilation: usize,
    ) -> Result<Self, String> {
        // PyTorch stores Conv1d weights as [out][in][tap].
        let torch = r.f32s(cout * cin * kernel)?;
        let mut weights = vec![0.0; torch.len()];
        for o in 0..cout {
            for i in 0..cin {
                for t in 0..kernel {
                    weights[(t * cout + o) * cin + i] = torch[(o * cin + i) * kernel + t];
                }
            }
        }
        let frames = (kernel - 1) * dilation + 1;
        Ok(Self {
            cin,
            cout,
            kernel,
            dilation,
            weights,
            bias: r.f32s(cout)?,
            history: vec![0.0; frames * cin],
            newest: frames - 1,
        })
    }

    fn reset(&mut self) {
        self.history.fill(0.0);
    }

    fn step(&mut self, input: &[f32], out: &mut [f32]) {
        let frames = self.history.len() / self.cin;
        self.newest = (self.newest + 1) % frames;
        self.history[self.newest * self.cin..][..self.cin].copy_from_slice(input);
        out.copy_from_slice(&self.bias);
        for tap in 0..self.kernel {
            let back = (self.kernel - 1 - tap) * self.dilation;
            let frame = (self.newest + frames - back) % frames;
            let x = &self.history[frame * self.cin..][..self.cin];
            let w = &self.weights[tap * self.cout * self.cin..][..self.cout * self.cin];
            for (out, row) in out.iter_mut().zip(w.chunks_exact(self.cin)) {
                *out += dot(row, x);
            }
        }
    }
}

/// LayerNorm over the channels of one frame.
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

fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}

struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8], String> {
        let end = self.pos + n;
        let slice = self.bytes.get(self.pos..end).ok_or("model file is truncated")?;
        self.pos = end;
        Ok(slice)
    }

    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn f32(&mut self) -> Result<f32, String> {
        Ok(f32::from_le_bytes(self.take(4)?.try_into().unwrap()))
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

    const CHANNELS: usize = 12;
    const KERNEL: usize = 3;
    const DILATIONS: [usize; 3] = [1, 2, 4];

    struct Rng(u32);

    impl Rng {
        fn next(&mut self) -> f32 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 17;
            self.0 ^= self.0 << 5;
            self.0 as f32 / u32::MAX as f32 - 0.5
        }

        fn vec(&mut self, n: usize, scale: f32) -> Vec<f32> {
            (0..n).map(|_| self.next() * scale).collect()
        }
    }

    /// A random model in PyTorch layout, kept alongside its serialized form.
    struct Raw {
        mean: Vec<f32>,
        std: Vec<f32>,
        /// (weight [out][in][tap], bias, in, out, kernel, dilation) in run order.
        convs: Vec<(Vec<f32>, Vec<f32>, usize, usize, usize, usize)>,
        norms: Vec<(Vec<f32>, Vec<f32>)>,
    }

    fn random_model() -> (Vec<u8>, Raw) {
        let mut rng = Rng(0xdead_beef);
        let mut bytes = MAGIC.to_vec();
        let mut header = |v: u32| bytes.extend_from_slice(&v.to_le_bytes());
        for v in [FORMAT_VERSION, FEATURE_VERSION, N_MELS as u32, CHANNELS as u32, 3, KERNEL as u32]
        {
            header(v);
        }
        header(DILATIONS.len() as u32);
        DILATIONS.iter().for_each(|d| header(*d as u32));
        for v in [0.0f32, 0.5, 0.5] {
            bytes.extend_from_slice(&v.to_le_bytes());
        }

        let mut raw = Raw {
            mean: rng.vec(N_MELS, 1.0),
            std: rng.vec(N_MELS, 1.0).iter().map(|v| v + 1.0).collect(),
            convs: Vec::new(),
            norms: Vec::new(),
        };
        let mut conv = |rng: &mut Rng, cin, cout, kernel, dilation| {
            let w = rng.vec(cout * cin * kernel, 0.6);
            let b = rng.vec(cout, 0.2);
            raw.convs.push((w, b, cin, cout, kernel, dilation));
        };
        conv(&mut rng, N_MELS, CHANNELS, 3, 1);
        for d in DILATIONS {
            raw.norms.push((rng.vec(CHANNELS, 1.0), rng.vec(CHANNELS, 0.2)));
            conv(&mut rng, CHANNELS, CHANNELS, KERNEL, d);
            conv(&mut rng, CHANNELS, CHANNELS, 1, 1);
        }
        raw.norms.push((rng.vec(CHANNELS, 1.0), rng.vec(CHANNELS, 0.2)));
        conv(&mut rng, CHANNELS, 2, 1, 1);

        // Serialize in file order: mean, std, input, (norm, conv, mix)*, norm, head.
        let mut put = |v: &[f32]| v.iter().for_each(|x| bytes.extend_from_slice(&x.to_le_bytes()));
        put(&raw.mean);
        put(&raw.std);
        let c = &raw.convs;
        put(&c[0].0);
        put(&c[0].1);
        for i in 0..DILATIONS.len() {
            put(&raw.norms[i].0);
            put(&raw.norms[i].1);
            for conv in &c[1 + 2 * i..3 + 2 * i] {
                put(&conv.0);
                put(&conv.1);
            }
        }
        put(&raw.norms[DILATIONS.len()].0);
        put(&raw.norms[DILATIONS.len()].1);
        put(&c[c.len() - 1].0);
        put(&c[c.len() - 1].1);
        (bytes, raw)
    }

    /// Whole-sequence forward pass with explicit zero left-padding, written
    /// straight from the PyTorch definition: sequences are `[time][channel]`.
    fn reference(raw: &Raw, mels: &[MelFrame]) -> Vec<[f32; 2]> {
        let conv = |x: &[Vec<f32>], (w, b, cin, cout, k, d): &(Vec<f32>, Vec<f32>, usize, usize, usize, usize)| {
            (0..x.len())
                .map(|t| {
                    (0..*cout)
                        .map(|o| {
                            let mut sum = b[o];
                            for tap in 0..*k {
                                let back = (k - 1 - tap) * d;
                                if t >= back {
                                    for i in 0..*cin {
                                        sum += w[(o * cin + i) * k + tap] * x[t - back][i];
                                    }
                                }
                            }
                            sum
                        })
                        .collect::<Vec<f32>>()
                })
                .collect::<Vec<_>>()
        };
        let norm = |x: &[Vec<f32>], (w, b): &(Vec<f32>, Vec<f32>)| {
            let ln = LayerNorm { weight: w.clone(), bias: b.clone() };
            x.iter()
                .map(|v| {
                    let mut out = vec![0.0; v.len()];
                    ln.apply(v, &mut out);
                    out
                })
                .collect::<Vec<_>>()
        };
        let map = |x: Vec<Vec<f32>>| -> Vec<Vec<f32>> {
            x.into_iter().map(|v| v.into_iter().map(gelu).collect()).collect()
        };

        let x: Vec<Vec<f32>> = mels
            .iter()
            .map(|m| (0..N_MELS).map(|i| (m[i] - raw.mean[i]) / raw.std[i]).collect())
            .collect();
        let mut h = conv(&x, &raw.convs[0]);
        for i in 0..DILATIONS.len() {
            let u = map(conv(&norm(&h, &raw.norms[i]), &raw.convs[1 + 2 * i]));
            let mixed = conv(&u, &raw.convs[2 + 2 * i]);
            for (h, m) in h.iter_mut().zip(mixed) {
                h.iter_mut().zip(m).for_each(|(h, m)| *h += m);
            }
        }
        let out = conv(&map(norm(&h, &raw.norms[DILATIONS.len()])), raw.convs.last().unwrap());
        out.iter().map(|v| [sigmoid(v[0]), sigmoid(v[1])]).collect()
    }

    #[test]
    fn streaming_matches_whole_sequence() {
        let (bytes, raw) = random_model();
        let mut model = BeatModel::from_bytes(&bytes).unwrap();
        let mut rng = Rng(42);
        let mels: Vec<MelFrame> =
            (0..40).map(|_| std::array::from_fn(|_| rng.next() * 4.0)).collect();
        let expected = reference(&raw, &mels);
        for (t, (mel, want)) in mels.iter().zip(&expected).enumerate() {
            let got = model.step(mel);
            assert!(
                (got.beat - want[0]).abs() < 1e-5 && (got.downbeat - want[1]).abs() < 1e-5,
                "frame {t}: {got:?} vs {want:?}"
            );
        }

        model.reset();
        let again = model.step(&mels[0]);
        assert!((again.beat - expected[0][0]).abs() < 1e-5);
    }

    #[test]
    fn rejects_mismatched_files() {
        let (bytes, _) = random_model();
        assert!(BeatModel::from_bytes(&bytes[..bytes.len() - 4]).is_err());
        let mut extra = bytes.clone();
        extra.push(0);
        assert!(BeatModel::from_bytes(&extra).is_err());
        let mut wrong_version = bytes.clone();
        wrong_version[8..12].copy_from_slice(&(FEATURE_VERSION + 1).to_le_bytes());
        let err = BeatModel::from_bytes(&wrong_version).err().unwrap();
        assert!(err.contains("front-end"), "{err}");
    }

    #[test]
    fn erf_matches_known_values() {
        for (x, want) in [(0.0, 0.0), (0.5, 0.520_499_9), (1.0, 0.842_700_8), (-2.0, -0.995_322_3)] {
            assert!((erf(x) - want).abs() < 3e-7, "erf({x})");
        }
    }
}
