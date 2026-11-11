"""Causal TCN that turns mel frames into beat and downbeat activations.

Every convolution is left-padded, so output frame t depends only on frames
<= t and the network can run frame by frame on live audio with one ring
buffer per layer.
"""

from dataclasses import asdict, dataclass

import torch
import torch.nn.functional as F
from torch import nn


@dataclass
class ModelConfig:
    n_mels: int = 64
    channels: int = 64
    kernel: int = 3
    dilations: tuple[int, ...] = (1, 2, 4, 8, 16, 32, 64, 128)
    dropout: float = 0.1
    n_outputs: int = 2

    def receptive_field(self) -> int:
        """Frames of input that reach one output frame."""
        return 1 + (self.kernel - 1) * (1 + sum(self.dilations))

    def to_dict(self) -> dict:
        return asdict(self)


class CausalConv(nn.Conv1d):
    def __init__(self, cin: int, cout: int, kernel: int, dilation: int = 1):
        super().__init__(cin, cout, kernel, dilation=dilation)
        self.left_pad = (kernel - 1) * dilation

    def forward(self, x: torch.Tensor) -> torch.Tensor:
        return super().forward(F.pad(x, (self.left_pad, 0)))


class FrameNorm(nn.LayerNorm):
    """LayerNorm over channels of each frame; causal, unlike batch/group norm."""

    def forward(self, x: torch.Tensor) -> torch.Tensor:
        return super().forward(x.transpose(1, 2)).transpose(1, 2)


class Block(nn.Module):
    def __init__(self, channels: int, kernel: int, dilation: int, dropout: float):
        super().__init__()
        self.norm = FrameNorm(channels)
        self.conv = CausalConv(channels, channels, kernel, dilation)
        self.mix = nn.Conv1d(channels, channels, 1)
        self.dropout = nn.Dropout(dropout)

    def forward(self, x: torch.Tensor) -> torch.Tensor:
        return x + self.dropout(self.mix(F.gelu(self.conv(self.norm(x)))))


class BeatTCN(nn.Module):
    def __init__(self, config: ModelConfig):
        super().__init__()
        self.config = config
        # Kernel 3 on the input lets the first layer see frame-to-frame rises.
        self.input = CausalConv(config.n_mels, config.channels, 3)
        self.blocks = nn.ModuleList(
            Block(config.channels, config.kernel, d, config.dropout) for d in config.dilations
        )
        self.norm = FrameNorm(config.channels)
        self.head = nn.Conv1d(config.channels, config.n_outputs, 1)

    def forward(self, mel: torch.Tensor) -> torch.Tensor:
        """(batch, n_mels, frames) normalized input -> (batch, n_outputs,
        frames) logits; for the beat model, beat and downbeat."""
        x = self.input(mel)
        for block in self.blocks:
            x = block(x)
        return self.head(F.gelu(self.norm(x)))
