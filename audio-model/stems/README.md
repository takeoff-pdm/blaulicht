# Online stem separation (drums, bass, other, vocals)

Research into separating stems online, from live audio, with a bounded lookahead, so the light
engine can react to drums, bass and vocals separately. Result: a small student network distilled
from `htdemucs_ft`, streaming frame by frame with a fixed, configurable lookahead, ported to Rust
(`crates/audio_features/src/stem_model.rs`) and verified against PyTorch.

## Results in short

- **It works online.** On held-out dance tracks the streaming student follows the teacher's stems
  with a log-energy envelope correlation of 0.92-0.93 (drums 0.96) and a stem-activity F1 of
  0.88-0.89. The DSP baseline (HPSS + bands) gets 0.70 / 0.62. In Rust the full model costs about
  20% of one M2 Pro core, frame by frame and unoptimised; a 4x smaller variant costs 7% and keeps an
  envelope correlation of 0.92.
- **Lookahead helps up to about 100 ms, then stops.** Going from 0 to 10 frames (46 -> 146 ms total
  latency) adds +0.3 dB on MUSDB and +0.55 dB on dance tracks. 30 and 100 frames (346 ms, 1.05 s)
  add almost nothing more (+0.06 dB at 1 s). For light signals, even lookahead 0 is nearly as good
  (envelope correlation 0.923 vs 0.930).
- **Distillation is what makes it work on dance music.** The same model trained only on MUSDB ground
  truth reaches 4.6 dB agreement on dance tracks (envelope correlation 0.84). Trained only on
  teacher labels (no MUSDB at all) it reaches 8.0 dB (0.93), and it even matches the ground-truth
  model on MUSDB itself (4.99 vs 4.93 dB). So MUSDB, which is non-commercial, is not needed.
- **Where it stands.** On MUSDB18-HQ test: 5.2 dB uSDR at 146 ms. For comparison, the teacher scores
  8.6 offline, and an ideal real-valued mask with this STFT scores 8.0. So model capacity is the
  limit, not the mask design. Published streaming models:
  - HS-TasNet: 4.65 dB at 23 ms, with 42M parameters and about 3.6 GMAC/s.
  - Band-SCNet: 7.79 dB at 92 ms, with 2.6M parameters and 17.6 GFLOP/s.

  Our student (1.85 GMAC/s) is in the HS-TasNet class. Band-SCNet shows that about 2.5 dB more is
  reachable with about 5x the compute. Those papers report museval cSDR; our cSDR' has no
  distortion filter, so treat gaps under about 1 dB as noise.

### MUSDB18-HQ test (50 tracks, stereo, ground truth)

uSDR columns are per stem; cSDR' is the median over 1 s chunks.

| model | latency | uSDR avg | drums | bass | other | vocals | cSDR' avg |
|---|---|---|---|---|---|---|---|
| teacher `htdemucs_ft` | offline | **8.60** | 10.20 | 9.01 | 6.20 | 9.00 | 9.27 |
| ideal ratio mask, stereo (oracle) | - | 8.25 | 8.86 | 6.84 | 7.62 | 9.67 | 7.63 |
| ideal ratio mask from mono (oracle, our design's ceiling) | - | 7.99 | 8.64 | 6.70 | 7.29 | 9.34 | 7.36 |
| student, lookahead 0 | 46 ms | 4.92 | 6.09 | 3.87 | 3.78 | 5.92 | 4.77 |
| **student, lookahead 10** | **146 ms** | **5.21** | 6.52 | 4.17 | 3.94 | 6.21 | 5.14 |
| student, lookahead 30 | 346 ms | 5.23 | 6.58 | 4.24 | 3.92 | 6.20 | 5.13 |
| student, lookahead 100 | 1046 ms | 5.28 | 6.63 | 4.30 | 3.95 | 6.22 | 5.28 |
| student la30, MUSDB ground truth only | 346 ms | 4.93 | 6.23 | 3.76 | 3.65 | 6.11 | 5.02 |
| student la30, teacher labels only (no MUSDB) | 346 ms | 4.99 | 6.39 | 4.14 | 3.64 | 5.77 | 4.97 |
| student small (32 ch, 2.1M params), lookahead 10 | 146 ms | 4.87 | 6.19 | 3.96 | 3.61 | 5.72 | 4.79 |
| DSP strawman (HPSS + bands) | - | 1.42 | 1.96 | 0.61 | 1.01 | 2.11 | 1.50 |

### Held-out dance tracks (11 tracks, 55 min, teacher stems as reference)

This measures agreement with the teacher, not ground truth. The uSDR columns are per stem.

| model | latency | uSDR vs teacher | drums | bass | other | vocals | envelope corr | activity F1 |
|---|---|---|---|---|---|---|---|---|
| student, lookahead 0 | 46 ms | 7.48 | 13.07 | 7.17 | 5.13 | 4.54 | 0.923 | 0.877 |
| **student, lookahead 10** | **146 ms** | **8.03** | 13.83 | 7.62 | 5.37 | 5.30 | 0.930 | 0.887 |
| student, lookahead 30 | 346 ms | 7.94 | 13.52 | 7.53 | 5.61 | 5.10 | 0.929 | 0.887 |
| student, lookahead 100 | 1046 ms | 8.08 | 13.67 | 7.58 | 5.43 | 5.64 | 0.932 | 0.889 |
| student la30, MUSDB ground truth only | 346 ms | 4.64 | 9.85 | 4.42 | 2.89 | 1.40 | 0.844 | 0.793 |
| student la30, teacher labels only | 346 ms | 7.99 | 13.60 | 7.51 | 5.43 | 5.43 | 0.929 | 0.889 |
| student small (32 ch), lookahead 10 | 146 ms | 7.41 | 13.05 | 7.00 | 4.90 | 4.70 | 0.920 | 0.870 |
| DSP strawman | - | -0.90 | 1.19 | -3.78 | 1.05 | -2.06 | 0.698 | 0.618 |

All students use the same recipe: 40k steps, batch 16 x 6 s. Data is MUSDB ground truth, the dance
teacher labels and the FMA teacher labels, sampled 1 : 1 : 2, except where the row says otherwise.
Each figure is one seed. Raw numbers are in `results/*.json`.

### Cost

| model | params | MAC / frame | torch, 1 thread, batched | Rust `StemModel`, 1 thread, per hop |
|---|---|---|---|---|
| full (64 ch) | 7.95M | 18.5M (1.85 GMAC/s) | real-time factor 0.03-0.04 | **0.18-0.21** (1.8-2.1 ms per 10 ms hop) |
| small (32 ch) | 2.08M | 4.7M (0.47 GMAC/s) | 0.021 | **0.067** (0.7 ms per hop) |

Measured on an M2 Pro. The small model gives up 0.3-0.6 dB and keeps envelope correlation at 0.92,
for a third of the cost. The Rust port reproduces a float64 PyTorch reference at a 47-69 dB
signal-to-difference ratio (bass lowest). PyTorch's own float32 run sits at about 50 dB on trained weights, and
the numpy streaming reference at 70-83 dB (it does its FFTs in float64).

### What this means for blaulicht

- **Latency vs light sync.** People notice sound arriving before the picture at about 45 ms
  (ITU-R BT.1359), and lights that lag the sound are that case. The 46 ms model (lookahead 0) sits
  at that threshold; 146 ms is visible on hit-synced effects. Two ways to handle it:
  - Let the stems decide *what* reacts: drum or bass energy, vocals on or off, section changes.
    Those are envelopes, and a little latency doesn't hurt them.
  - Keep transient timing on the beat tracker, which already fires early (`lead_ms`).

  For lights alone, lookahead 0 loses almost nothing (envelope correlation 0.923 vs 0.930).
- **Integration (not done).**
  - `StemModel::push` returns a `StemFrame` per hop: per-stem spectral energy and the mix energy.
    It needs no inverse FFT, and stem audio is optional.
  - It runs at 44.1 kHz only. A 48 kHz input needs a resampler, or a retrain with hop 480.
  - The natural home is a `StemTracker` next to `beat_tracker.rs` that publishes per-stem envelopes
    as signals.
  - Export with `export_stems.py`. The weights file is 31.8 MB in f32; storing it in f16 would
    halve that.
- **Next steps if more quality is wanted.**
  - A dual-path model with causal LSTMs over time and bands (SCNet / BSRNN style), as in Band-SCNet.
  - A stronger teacher; check the licences of community BS-RoFormer / SCNet-XL weights first.
  - More dance music for distillation; it is cheap (the teacher runs at about 40x realtime per
    A100).
  - Complex masks for better phase when the stems are listened to.

## How Rekordbox does it

Rekordbox 7 ships the separator as `Contents/Frameworks/libDemucs3.dylib` (plus an older
`libSpleeter.dylib`). Its symbols show:

- an **HDemucs** (Demucs v3, hybrid time/spectrogram) model in ONNX, `hdemucs.onnx`, run through
  `libonnxruntime.1.18.0`;
- the STFT and iSTFT as separate TensorFlow SavedModels (`stft_model`, `istft_model`);
- an API that works through a whole track buffer: `demucs3OperationAtOnce` and, chunk by chunk,
  `demucs3OperationAtIndex`, in two modes (`...SeparatedBass`, 4 stems, and `...OtherWithBass`,
  3 stems: vocal / drums / inst). "Segment length is not valid" errors mean fixed-size segments.

The weights are not in the bundle; Rekordbox downloads them. It is not a streaming model. It runs
a large offline separator on segments **ahead of the playhead**, which works because a DJ app
has the whole file. VirtualDJ, Serato and others do the same (pre-analysis, or "load the track a
few seconds early"). A live input, such as blaulicht's microphone source, has no future to read
ahead into, so the lookahead becomes latency. That is the trade-off studied here.

## Method

**Teacher.** `htdemucs_ft` (Meta, MIT licence; 9.0 dB cSDR on MUSDB18 in its paper) separates
unlabeled music offline: 97 of the user's dance tracks and 6000 random FMA-medium clips (plain
`htdemucs` for speed, ~50 h). MUSDB18-HQ train (86 tracks, 14 held out for validation) adds real
ground truth. Targets are stored mono as `[mix, drums, bass, vocals]`; "other" is the residual,
so the stems always sum to the mix.

**Student** (`student.py`). Mono STFT, n_fft 2048, hop 441 (exactly 100 frames/s, matching
blaulicht's mel front end) -> log magnitude in 49 bands (2 bins wide in the bass, 128 at the top)
-> 12 blocks of [dilated kernel-3 conv over time, shared across bands | per-frame linear mix across
all bands] -> per band a linear head -> softmax over the 4 stems per bin (masks sum to 1). 7.95M
parameters, 18.5M MAC per frame (1.85 GMAC per second of audio).

**Lookahead is exact and explicit.** Each time conv is padded asymmetrically; the right pads sum
to `lookahead` frames (centred convs get the budget first). Frame t's mask depends on input
frames <= t + lookahead and nothing later (checked by perturbation). Latency = 2048 samples
(46 ms analysis window) + lookahead x 10 ms. Masks from the mono mix are applied to each channel
for stereo output.

**Training** (`train.py`). 6 s crops, batch 16, half of them remixed from stems of four random
clips with random per-stem gains, random polarity. Loss: L1 on complex STFT + L1 on magnitude +
10 x L1 on waveform. AdamW 1e-3, 2k warm-up, cosine, 40k steps, bf16, `torch.compile`.

**Streaming** (`stream.py`, numpy; `stem_model.rs`, Rust). One 441-sample hop in, one hop of each
stem out. Each time layer keeps a ring of its last 2d+1 normalised inputs and emits output frame
u - r_i when input frame u arrives. Both match the offline PyTorch model (numpy 6e-6; Rust 2e-4
absolute, float order and erf approximation).

## Evaluation (`evaluate.py`)

- MUSDB18-HQ test, 50 full tracks, stereo, ground truth: uSDR (MDX mean) and cSDR' (median over
  1 s chunks and tracks, without BSS-eval's distortion filter, so close to but not identical to
  museval; the teacher scores 9.27 here vs its published 9.00).
- Held-out dance tracks (12), teacher stems as reference: uSDR vs teacher, and two light-oriented
  numbers: correlation of each stem's 100 fps log-energy envelope (floored 30 dB below the mix),
  and F1 of "stem active" frames (stem within 12 dB of the mix).
- Baselines: the teacher; the ideal ratio mask with our STFT (the ceiling of a real mask, mono or
  stereo); a DSP strawman (HPSS + bands).

## Files

| File | What |
|---|---|
| `teacher.py` | teacher separation / MUSDB conversion to training clips |
| `data.py` | random crops, remixing, splits |
| `student.py` | the streaming student |
| `train.py` | training with validation |
| `evaluate.py` | MUSDB test + dance test, teacher / oracle / DSP baselines |
| `stream.py` | numpy frame-by-frame reference and equivalence check |
| `bench_cpu.py` | CPU cost on this machine |
| `export_stems.py` | checkpoint -> `stems.bin` for Rust `StemModel` |
| `check_rust_stems.py` | Rust vs PyTorch on a real song |

Training ran on `ae-charlie` in `/localhome/sokrates/stems` (data, runs, venv).

## Licences

MUSDB18(-HQ) is non-commercial. FMA audio carries each artist's licence. `htdemucs_ft` weights are
MIT. The `nomusdb_la30` run shows what a student trained without MUSDB reaches.
