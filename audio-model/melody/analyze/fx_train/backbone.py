"""The frozen backbone and the frame features every head sees, the same in training and inference.

BEATs (PretrainedSED checkpoint BEATs_strong_1) in 10 s pieces overlapping by half, blended with a
triangular weight as in `sed.py`: back-to-back pieces made events start on 10 s multiples. Plus a
64-band log-mel at 25 fps from 32 kHz audio, because 16 kHz BEATs can't see the top of a riser.
"""

import os
import sys

import numpy as np
import torch
from scipy.signal import resample_poly

from analyze import common

SRC = common.ROOT / "models/src/PretrainedSED"
FPS, FX_SR, B_SR, PIECE = 25, 32000, 16000, 10
N_FFT, HOP, N_MELS = 2048, 1280, 64
SEG, STEP = PIECE * B_SR, PIECE * B_SR // 2
FRAMES = PIECE * FPS  # 250 per piece
DIM = 768


def to_32k(x):
    return resample_poly(x, 320, 441).astype(np.float32)


def to_16k(x32):
    return resample_poly(x32, 1, 2).astype(np.float32)


def _mel_bank():
    f = np.fft.rfftfreq(N_FFT, 1 / FX_SR)
    m = lambda h: 2595 * np.log10(1 + h / 700)
    pts = 700 * (10 ** (np.linspace(m(30.0), m(15500.0), N_MELS + 2) / 2595) - 1)
    B = np.zeros((N_MELS, len(f)), np.float32)
    for i in range(N_MELS):
        lo, c, hi = pts[i: i + 3]
        B[i] = np.clip(np.minimum((f - lo) / (c - lo), (hi - f) / (hi - c)), 0, None)
    return B / (B.sum(1, keepdims=True) + 1e-9)


BANK = _mel_bank()
WINDOW = np.hanning(N_FFT).astype(np.float32)


def mel_power(x32):
    """(frames, 64) power, frame i centred on i / 25 s."""
    n = int(np.ceil(len(x32) / HOP))
    y = np.pad(x32, (N_FFT // 2, N_FFT))
    idx = np.lib.stride_tricks.sliding_window_view(y, N_FFT)[::HOP][:n]
    return (np.abs(np.fft.rfft(idx * WINDOW, axis=1)) ** 2 @ BANK.T).astype(np.float32)


def mel_db(x32):
    return (10 * np.log10(mel_power(x32) + 1e-7)).astype(np.float32)


class Beats:
    def __init__(self):
        sys.path.insert(0, str(SRC))
        cwd = os.getcwd()
        os.chdir(SRC)  # its checkpoint folder is relative ("resources")
        from models.beats.BEATs_wrapper import BEATsWrapper
        from models.prediction_wrapper import PredictionsWrapper

        self.model = PredictionsWrapper(BEATsWrapper(), checkpoint="BEATs_strong_1").eval().cuda()
        os.chdir(cwd)
        for p in self.model.parameters():
            p.requires_grad_(False)

    @torch.no_grad()
    def tokens(self, pieces):
        """(pieces, 250, 768) as float32 CPU array."""
        out = []
        for i in range(0, len(pieces), 48):
            b = torch.from_numpy(pieces[i: i + 48]).cuda()
            mel = self.model.mel_forward(b)
            with torch.autocast("cuda", dtype=torch.float16):
                x = self.model.model(mel)
                x = torch.nn.functional.adaptive_avg_pool1d(x.transpose(1, 2), FRAMES).transpose(1, 2)
            out.append(x.float().cpu().numpy())
        return np.concatenate(out)

    @torch.no_grad()
    def strong(self, pieces):
        """PretrainedSED's own 447 classes: (pieces, 447, 250) probabilities, for the speech check."""
        out = []
        for i in range(0, len(pieces), 48):
            b = torch.from_numpy(pieces[i: i + 48]).cuda()
            with torch.autocast("cuda", dtype=torch.float16):
                s, _ = self.model(self.model.mel_forward(b))
            out.append(torch.sigmoid(s).float().cpu().numpy())
        return np.concatenate(out)


def cut_pieces(x16):
    """Every real sample sits in the middle of some piece, as in sed.py."""
    y = np.pad(x16, (STEP, SEG))
    return np.stack([y[a: a + SEG] for a in range(0, len(y) - SEG + 1, STEP)])


_W = (1 - np.abs(np.linspace(-1, 1, FRAMES)) + 1e-3).astype(np.float32)


def blend(tok, n_seconds):
    """Overlap-add the pieces' frames back onto the audio's timeline: (frames, dim)."""
    step = FRAMES // 2
    n = step * (len(tok) - 1) + FRAMES
    total, weight = np.zeros((n, tok.shape[2]), np.float32), np.zeros(n, np.float32)
    for i, t in enumerate(tok):
        total[i * step: i * step + FRAMES] += t * _W[:, None]
        weight[i * step: i * step + FRAMES] += _W
    return (total / weight[:, None])[step: step + int(round(n_seconds * FPS))]


def features(beats, x32, reverse=False):
    """(frames, 768), (frames, 64) for audio at 32 kHz."""
    if reverse:
        x32 = x32[::-1].copy()
    n = len(x32) / FX_SR
    tok = beats.tokens(cut_pieces(to_16k(x32)))
    e = blend(tok, n)
    m = mel_db(x32)[: len(e)]
    if len(m) < len(e):
        m = np.pad(m, ((0, len(e) - len(m)), (0, 0)), mode="edge")
    return e, m
