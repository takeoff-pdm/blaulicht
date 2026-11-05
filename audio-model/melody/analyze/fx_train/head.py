"""The small head: a conv over the frames, a BiGRU, one sigmoid per class at 25 fps."""

import numpy as np
import torch
from torch import nn

from analyze.fx_train.effects import CLASSES

WIN, HOP_F = 1000, 500  # 40 s windows, half overlapping, the length it was trained on
# frames: shortest event, bridged gap
RULES = {"riser": (25, 12), "downlifter": (10, 5), "impact": (3, 1), "horn": (5, 3), "siren": (15, 5), "revcym": (6, 2)}


class Head(nn.Module):
    def __init__(self, dim, hidden=192, n=len(CLASSES), drop=0.25):
        super().__init__()
        self.conv = nn.Sequential(nn.Conv1d(dim, 256, 5, padding=2), nn.GELU(), nn.Dropout(drop))
        self.gru = nn.GRU(256, hidden, 2, batch_first=True, bidirectional=True, dropout=drop)
        self.out = nn.Linear(hidden * 2, n)

    def forward(self, x):  # (batch, frames, dim)
        z = self.conv(x.transpose(1, 2)).transpose(1, 2)
        z, _ = self.gru(z)
        return self.out(z)


def inputs(emb, mel, features, mean, std):
    """Stack and normalise what this head reads ("beats", "mel" or "both"), as float32."""
    parts = []
    if features in ("beats", "both"):
        parts.append(np.asarray(emb, np.float32))
    if features in ("mel", "both"):
        parts.append(np.asarray(mel, np.float32))
    x = np.concatenate(parts, axis=-1)
    return (x - mean) / std


def load(path, device="cuda"):
    ck = torch.load(path, map_location="cpu", weights_only=False)
    head = Head(ck["dim"], ck.get("hidden", 192))
    head.load_state_dict(ck["state"])
    return head.eval().to(device), ck


@torch.no_grad()
def predict(head, ck, emb, mel, device="cuda"):
    """Per-frame probabilities for a whole song, (frames, classes): overlapping windows, triangular blend."""
    x = inputs(emb, mel, ck["features"], ck["mean"], ck["std"])
    n = len(x)
    if n <= WIN:
        t = torch.from_numpy(x[None]).to(device)
        return torch.sigmoid(head(t))[0].cpu().numpy()
    w = 1 - np.abs(np.linspace(-1, 1, WIN)) + 1e-3
    total, weight = np.zeros((n, len(CLASSES)), np.float32), np.zeros(n, np.float32)
    starts = list(range(0, n - WIN, HOP_F)) + [n - WIN]
    for i in range(0, len(starts), 16):
        b = np.stack([x[s: s + WIN] for s in starts[i: i + 16]])
        p = torch.sigmoid(head(torch.from_numpy(b).to(device))).cpu().numpy()
        for s, q in zip(starts[i: i + 16], p):
            total[s: s + WIN] += q * w[:, None]
            weight[s: s + WIN] += w
    return total / weight[:, None]


def events(P, thresholds, fps=25, runs=None):
    """[[start, end, kind, peak]] from the frame tracks."""
    from analyze import common

    out = []
    for j, kind in enumerate(CLASSES):
        lo, gap = RULES[kind]
        for a, b in common.runs(P[:, j] >= thresholds[j], lo, gap):
            out.append([round(a / fps, 2), round(b / fps, 2), kind, round(float(P[a:b, j].max()), 3)])
    return sorted(out)
