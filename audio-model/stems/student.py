"""Streaming stem separator with a fixed, explicit lookahead.

Mono STFT (n_fft 2048, hop 441 = 100 frames/s at 44.1 kHz) -> magnitude
features split into ~48 frequency bands -> a stack of blocks, each
  * a dilated conv over time per band (weights shared across bands), padded
    asymmetrically so its output at frame t sees frames <= t + r_i, and
  * a per-frame mix across all bands (no time context, so no latency).
-> per-band heads emit one real mask per bin and stem; a softmax over stems
makes the stems sum to the mixture exactly.

The future context of the whole network is sum(r_i) = `lookahead` frames,
so a streaming implementation emits frame t once frame t + lookahead has
arrived: total latency = n_fft samples (46 ms, the analysis window) +
lookahead * 10 ms. Every op is conv1d / linear / layernorm / GELU, the
same building blocks the Rust `BeatModel` already implements.

Masks computed from the mono mix can be applied to each channel of a
stereo STFT, which is how stereo stems are produced.
"""

from dataclasses import asdict, dataclass, field

import torch
import torch.nn.functional as F
from torch import nn

SOURCES = ("drums", "bass", "other", "vocals")


@dataclass
class StudentConfig:
    n_fft: int = 2048
    hop: int = 441
    channels: int = 64
    kernel: int = 3
    dilations: tuple[int, ...] = (1, 2, 4, 8, 16, 32, 1, 2, 4, 8, 16, 32)
    lookahead: int = 0  # frames of future context (10 ms each)
    mix_channels: int = 16  # per-band bottleneck of the cross-band mix
    sources: int = 4
    dropout: float = 0.0

    def right_pads(self) -> list[int]:
        """Future frames each time conv sees. Centred convs first (most
        efficient use of the budget), then fully forward-looking ones."""
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


def band_edges(n_bins: int) -> list[tuple[int, int]]:
    """Frequency bands in STFT bins (21.5 Hz each at n_fft 2048): narrow in
    the bass where bass and kick overlap, wide at the top."""
    plan = [(24, 2), (48, 4), (192, 12), (384, 24), (768, 48), (n_bins, 128)]
    edges, lo = [], 0
    for stop, width in plan:
        while lo < min(stop, n_bins):
            hi = min(lo + width, stop, n_bins)
            edges.append((lo, hi))
            lo = hi
    return edges


class TimeBlock(nn.Module):
    """Pre-norm residual dilated conv over time, channels last: (B, K, T, C).
    The conv is written as a linear layer over `kernel` stacked taps."""

    def __init__(self, c: int, kernel: int, dilation: int, right: int, dropout: float):
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
        taps = torch.cat([y[:, :, j * self.dilation : j * self.dilation + t] for j in range(self.kernel)], -1)
        return x + self.drop(self.mix(F.gelu(self.conv(taps))))


class BandMix(nn.Module):
    """Per frame: squeeze each band to `m` channels, mix all bands with one
    linear layer, expand back. (B, K, T, C)."""

    def __init__(self, c: int, bands: int, m: int):
        super().__init__()
        self.norm = nn.LayerNorm(c)
        self.down = nn.Linear(c, m)
        self.across = nn.Linear(bands * m, bands * m)
        self.up = nn.Linear(m, c)

    def forward(self, x):
        b, k, t, _ = x.shape
        y = self.down(self.norm(x)).transpose(1, 2).reshape(b, t, -1)  # (B, T, K*m)
        y = F.gelu(self.across(y)).reshape(b, t, k, -1).transpose(1, 2)
        return x + self.up(y)


class Student(nn.Module):
    def __init__(self, cfg: StudentConfig):
        super().__init__()
        self.cfg = cfg
        n_bins = cfg.n_fft // 2 + 1
        self.edges = band_edges(n_bins)
        k = len(self.edges)
        self.register_buffer("window", torch.hann_window(cfg.n_fft), persistent=False)
        self.register_buffer("feat_mean", torch.zeros(n_bins))
        self.register_buffer("feat_std", torch.ones(n_bins))
        self.encode = nn.ModuleList(nn.Linear(hi - lo, cfg.channels) for lo, hi in self.edges)
        blocks = []
        for d, r in zip(cfg.dilations, cfg.right_pads()):
            blocks += [TimeBlock(cfg.channels, cfg.kernel, d, r, cfg.dropout),
                       BandMix(cfg.channels, k, cfg.mix_channels)]
        self.blocks = nn.Sequential(*blocks)
        self.out_norm = nn.LayerNorm(cfg.channels)
        self.decode = nn.ModuleList(
            nn.Linear(cfg.channels, (hi - lo) * cfg.sources) for lo, hi in self.edges
        )

    # -- spectral helpers -------------------------------------------------
    def stft(self, wav: torch.Tensor) -> torch.Tensor:
        """(..., n) -> complex (..., bins, frames)."""
        shape = wav.shape
        spec = torch.stft(wav.reshape(-1, shape[-1]), self.cfg.n_fft, self.cfg.hop,
                          window=self.window, center=True, return_complex=True)
        return spec.reshape(*shape[:-1], *spec.shape[-2:])

    def istft(self, spec: torch.Tensor, length: int) -> torch.Tensor:
        shape = spec.shape
        wav = torch.istft(spec.reshape(-1, *shape[-2:]), self.cfg.n_fft, self.cfg.hop,
                          window=self.window, center=True, length=length)
        return wav.reshape(*shape[:-2], length)

    def features(self, mag: torch.Tensor) -> torch.Tensor:
        """(B, bins, T) magnitude -> normalised log magnitude."""
        f = torch.log(mag + 1e-5)
        return (f - self.feat_mean[:, None]) / self.feat_std[:, None]

    # -- network ----------------------------------------------------------
    def masks(self, mag: torch.Tensor) -> torch.Tensor:
        """(B, bins, T) mono magnitude -> (B, S, bins, T) masks summing to 1."""
        f = self.features(mag).transpose(1, 2)  # (B, T, bins)
        x = torch.stack([enc(f[..., lo:hi]) for enc, (lo, hi) in zip(self.encode, self.edges)], 1)
        x = self.out_norm(self.blocks(x))  # (B, K, T, C)
        s = self.cfg.sources
        logits = torch.cat(
            [dec(x[:, i]).unflatten(-1, (hi - lo, s)) for i, (dec, (lo, hi)) in enumerate(zip(self.decode, self.edges))],
            dim=2,
        )  # (B, T, bins, S)
        return logits.float().softmax(-1).permute(0, 3, 2, 1)

    def forward(self, mix: torch.Tensor) -> tuple[torch.Tensor, torch.Tensor]:
        """mix: (B, C, n) mono or stereo. Masks come from the channel mean and
        are applied per channel. Returns (stems (B, S, C, n), spec (B, S, C, bins, T))."""
        spec = self.stft(mix)  # (B, C, bins, T)
        m = self.masks(spec.abs().mean(1) if mix.shape[1] > 1 else spec[:, 0].abs())
        est = m[:, :, None] * spec[:, None]
        return self.istft(est, mix.shape[-1]), est

    @torch.no_grad()
    def fit_feature_stats(self, mixes: torch.Tensor) -> None:
        """mixes: (B, n) mono. Per-bin mean/std of log magnitude."""
        f = torch.log(self.stft(mixes).abs() + 1e-5)  # (B, bins, T)
        self.feat_mean.copy_(f.mean((0, 2)))
        self.feat_std.copy_(f.std((0, 2)) + 1e-3)

    def macs_per_frame(self) -> int:
        cfg, k = self.cfg, len(self.edges)
        n_bins = cfg.n_fft // 2 + 1
        c, m = cfg.channels, cfg.mix_channels
        enc = n_bins * c
        time = len(cfg.dilations) * k * (c * c * cfg.kernel + c * c)
        mix = len(cfg.dilations) * (k * c * m * 2 + (k * m) ** 2)
        dec = n_bins * cfg.sources * c
        return enc + time + mix + dec
