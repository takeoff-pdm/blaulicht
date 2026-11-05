"""Streaming predominant-melody student with a fixed, explicit lookahead.

Distils Melodia (offline, whole-track) into a network that emits one pitch distribution per
10 ms frame once `lookahead` future frames have arrived.

Mono STFT (n_fft 2048, hop 441 = 100 frames/s at 44.1 kHz, the stem student's front end)
-> magnitude resampled to a log-frequency grid (20 cents per bin) -> harmonic stack: for every
output pitch bin k (55 Hz * 2^(k/60), 300 bins up to 1745 Hz) the log magnitude at h * f_k for
h in HARMONICS, so a pitch and its harmonics line up in one column -> blocks of
  * a conv across pitch bins (per frame, no latency), and
  * a dilated conv over time per pitch bin (weights shared across bins), padded asymmetrically
    so its output at frame t sees frames <= t + r_i (the stem student's TimeBlock),
-> per bin a pitch logit plus one "no melody" logit from the bin-pooled features; softmax over
the 301 classes. Inside the network five neighbouring 20-cent bins share one position (one
semitone, 60 positions), so the blocks run on 60 x C instead of 300 x C values per frame; the
head unpacks each position back into its five bins.

Every op is linear / layernorm / GELU over stacked taps, like the stem student, so a Rust port
can reuse the same kernels; the resampling and the harmonic gather are fixed index/weight tables.
"""

import math
from dataclasses import asdict, dataclass

import torch
import torch.nn.functional as F
from torch import nn

F_MIN = 55.0
BINS_PER_OCTAVE = 60
N_PITCH = 300
HARMONICS = (0.5, 1, 2, 3, 4, 5, 6, 7, 8)


@dataclass
class MelodyConfig:
    n_fft: int = 2048
    hop: int = 441
    sr: int = 44100
    channels: int = 64
    pool: int = 5  # pitch bins per network position (5 x 20 cents = one semitone)
    embed_channels: int = 8  # per pitch bin, before pooling
    freq_kernel: int = 5
    kernel: int = 3
    dilations: tuple[int, ...] = (1, 2, 4, 8, 16, 32, 64, 1)
    lookahead: int = 0
    dropout: float = 0.0

    def right_pads(self) -> list[int]:
        left = self.lookahead
        pads = []
        for d in self.dilations:
            r = min(d * (self.kernel - 1) // 2, left)
            pads.append(r)
            left -= r
        for i, d in enumerate(self.dilations):
            extra = min(d * (self.kernel - 1) - pads[i], left)
            pads[i] += extra
            left -= extra
        if left:
            raise ValueError(f"lookahead {self.lookahead} exceeds the receptive field")
        return pads

    def to_dict(self) -> dict:
        return asdict(self)


def bin_to_hz(b):
    return F_MIN * 2.0 ** (b / BINS_PER_OCTAVE)


def hz_to_bin(f):
    return BINS_PER_OCTAVE * torch.log2(f / F_MIN)


def harmonic_offsets() -> list[int]:
    return [round(BINS_PER_OCTAVE * math.log2(h)) for h in HARMONICS]


def logfreq_matrix(n_fft: int, sr: int) -> tuple[torch.Tensor, int]:
    """(n_log, n_fft // 2 + 1) linear-interpolation weights from STFT bins to the log grid,
    which spans the lowest to the highest harmonic of the pitch range; and the index of
    pitch bin 0 in that grid."""
    offs = harmonic_offsets()
    lo, hi = min(offs), N_PITCH - 1 + max(offs)
    n_bins = n_fft // 2 + 1
    w = torch.zeros(hi - lo + 1, n_bins)
    for i, k in enumerate(range(lo, hi + 1)):
        pos = bin_to_hz(k) * n_fft / sr
        a = int(math.floor(pos))
        frac = pos - a
        if a + 1 < n_bins:
            w[i, a] += 1 - frac
            w[i, a + 1] += frac
    return w, -lo


class TimeBlock(nn.Module):
    """Pre-norm residual dilated conv over time, channels last: (B, K, T, C)."""

    def __init__(self, c, kernel, dilation, right, dropout):
        super().__init__()
        self.norm = nn.LayerNorm(c)
        self.conv = nn.Linear(kernel * c, c)
        self.mix = nn.Linear(c, c)
        self.kernel, self.dilation = kernel, dilation
        self.pad = (dilation * (kernel - 1) - right, right)
        self.drop = nn.Dropout(dropout)

    def forward(self, x):
        t = x.shape[2]
        y = F.pad(self.norm(x), (0, 0, *self.pad))
        taps = torch.cat([y[:, :, j * self.dilation: j * self.dilation + t] for j in range(self.kernel)], -1)
        return x + self.drop(self.mix(F.gelu(self.conv(taps))))


class FreqBlock(nn.Module):
    """Pre-norm residual conv across pitch bins (zero padded), per frame: (B, K, T, C)."""

    def __init__(self, c, kernel):
        super().__init__()
        self.norm = nn.LayerNorm(c)
        self.conv = nn.Linear(kernel * c, c)
        self.mix = nn.Linear(c, c)
        self.kernel = kernel

    def forward(self, x):
        k = x.shape[1]
        h = self.kernel // 2
        y = F.pad(self.norm(x), (0, 0, 0, 0, h, h))
        taps = torch.cat([y[:, j: j + k] for j in range(self.kernel)], -1)
        return x + self.mix(F.gelu(self.conv(taps)))


class MelodyStudent(nn.Module):
    def __init__(self, cfg: MelodyConfig):
        super().__init__()
        self.cfg = cfg
        w, zero = logfreq_matrix(cfg.n_fft, cfg.sr)
        self.register_buffer("logfreq", w, persistent=False)
        idx = torch.tensor([[zero + k + o for o in harmonic_offsets()] for k in range(N_PITCH)])
        self.register_buffer("harm_idx", idx, persistent=False)  # (N_PITCH, H)
        self.register_buffer("window", torch.hann_window(cfg.n_fft), persistent=False)
        self.register_buffer("feat_mean", torch.zeros(w.shape[0]))
        self.register_buffer("feat_std", torch.ones(w.shape[0]))
        c, e = cfg.channels, cfg.embed_channels
        assert N_PITCH % cfg.pool == 0
        self.k = N_PITCH // cfg.pool
        self.embed = nn.Linear(len(HARMONICS), e)
        self.patch = nn.Linear(cfg.pool * e, c)  # pool neighbouring bins into one position
        self.pitch_pos = nn.Parameter(torch.zeros(self.k, 1, c))  # per-position bias (register etc.)
        blocks = []
        for d, r in zip(cfg.dilations, cfg.right_pads()):
            blocks += [FreqBlock(c, cfg.freq_kernel), TimeBlock(c, cfg.kernel, d, r, cfg.dropout)]
        self.blocks = nn.Sequential(*blocks)
        self.out_norm = nn.LayerNorm(c)
        self.pitch_head = nn.Linear(c, cfg.pool)  # back to 20-cent bins
        self.voice_head = nn.Linear(2 * c, 1)

    def spectrum(self, wav):
        """(B, n) -> log-frequency log magnitude (B, T, n_log)."""
        spec = torch.stft(wav, self.cfg.n_fft, self.cfg.hop, window=self.window, center=True,
                          return_complex=True).abs()  # (B, bins, T)
        return torch.log(torch.einsum("lb,nbt->ntl", self.logfreq, spec) + 1e-5)

    @torch.no_grad()
    def fit_feature_stats(self, wavs):
        f = self.spectrum(wavs)
        self.feat_mean.copy_(f.mean((0, 1)))
        self.feat_std.copy_(f.std((0, 1)) + 1e-3)

    def forward(self, wav):
        """(B, n) mono -> logits (B, T, N_PITCH + 1); class N_PITCH = no melody."""
        f = (self.spectrum(wav) - self.feat_mean) / self.feat_std  # (B, T, n_log)
        x = f[:, :, self.harm_idx]  # (B, T, N_PITCH, H)
        x = F.gelu(self.embed(x)).flatten(-2)  # (B, T, N_PITCH * e)
        x = self.patch(x.unflatten(-1, (self.k, -1)))  # (B, T, K, C)
        x = x.transpose(1, 2) + self.pitch_pos  # (B, K, T, C)
        x = self.out_norm(self.blocks(x))
        pitch = self.pitch_head(x).permute(0, 2, 1, 3).flatten(-2)  # (B, T, N_PITCH)
        pooled = torch.cat([x.mean(1), x.amax(1)], -1)  # (B, T, 2C)
        voice = self.voice_head(pooled)  # (B, T, 1)
        return torch.cat([pitch, voice], -1)

    def macs_per_frame(self) -> int:
        cfg = self.cfg
        c, k, e = cfg.channels, self.k, cfg.embed_channels
        n = len(cfg.dilations)
        front = self.logfreq.shape[0] * 2 + N_PITCH * len(HARMONICS) * e + k * cfg.pool * e * c
        blocks = n * k * (cfg.freq_kernel * c * c + c * c + cfg.kernel * c * c + c * c)
        return front + blocks + k * c * cfg.pool + 2 * c


def targets(f0: torch.Tensor, sigma: float = 1.0) -> torch.Tensor:
    """f0 (B, T) Hz, 0 = unvoiced -> soft targets (B, T, N_PITCH + 1)."""
    voiced = f0 > 0
    b = hz_to_bin(f0.clamp(min=1.0))
    k = torch.arange(N_PITCH, device=f0.device, dtype=torch.float32)
    g = torch.exp(-0.5 * ((k - b[..., None]) / sigma) ** 2)
    in_range = voiced & (b > -1) & (b < N_PITCH)
    g = g / g.sum(-1, keepdim=True).clamp(min=1e-8)
    t = torch.zeros(*f0.shape, N_PITCH + 1, device=f0.device)
    t[..., :N_PITCH] = torch.where(in_range[..., None], g, 0.0)
    t[..., N_PITCH] = (~in_range).float()
    return t


def decode(logits: torch.Tensor, voicing_bias: float = 0.0) -> torch.Tensor:
    """logits (B, T, N_PITCH + 1) -> f0 Hz (B, T), 0 = unvoiced. Sub-bin pitch from the
    probability-weighted mean of the 5 bins around the peak."""
    p = logits.float().softmax(-1)
    pitch_p = p[..., :N_PITCH]
    voiced = pitch_p.sum(-1) > p[..., N_PITCH] * math.exp(voicing_bias)
    peak = pitch_p.argmax(-1)
    k = torch.arange(N_PITCH, device=logits.device)
    near = (k - peak[..., None]).abs() <= 2
    w = pitch_p * near
    b = (w * k).sum(-1) / w.sum(-1).clamp(min=1e-8)
    return torch.where(voiced, bin_to_hz(b), torch.zeros_like(b))
