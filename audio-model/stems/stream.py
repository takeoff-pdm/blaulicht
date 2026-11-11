"""Frame-by-frame streaming inference, written the way the Rust port would be.

Push 441 samples (one hop), get 441 samples of each stem back, delayed by
a fixed latency of n_fft + lookahead * hop samples. Every layer keeps a
ring of its last 2d+1 normalised inputs; layer i emits output frame
u - r_i when input frame u arrives, so the network as a whole lags
`lookahead` frames. Pure numpy; no torch at inference time.

    python stream.py --ckpt runs/la50/best.pt --wav some.wav   # equivalence + timing
"""

import argparse
import time
from collections import deque
from pathlib import Path

import numpy as np


def layer_norm(x, w, b, eps=1e-5):
    m = x.mean(-1, keepdims=True)
    v = ((x - m) ** 2).mean(-1, keepdims=True)
    return (x - m) / np.sqrt(v + eps) * w + b


def gelu(x):
    from scipy.special import erf
    return 0.5 * x * (1 + erf(x / np.sqrt(2)))


class StreamingStudent:
    def __init__(self, ckpt_path: Path):
        import torch  # only to read the checkpoint

        from student import Student, StudentConfig

        c = torch.load(ckpt_path, map_location="cpu", weights_only=False)
        cfg = StudentConfig(**{k: tuple(v) if isinstance(v, list) else v for k, v in c["config"].items()})
        ref = Student(cfg)
        ref.load_state_dict(c["model"])
        self.cfg, self.edges = cfg, ref.edges
        W = {k: v.float().numpy() for k, v in c["model"].items()}
        self.W = W
        self.n_fft, self.hop = cfg.n_fft, cfg.hop
        self.window = np.hanning(cfg.n_fft + 1)[:-1].astype(np.float32)  # periodic Hann
        self.pads = cfg.right_pads()
        self.latency = cfg.n_fft + cfg.lookahead * cfg.hop  # samples
        n_blocks = len(cfg.dilations)
        self.time_layers, self.mix_layers = [], []
        for i in range(n_blocks):
            t, m = f"blocks.{2*i}.", f"blocks.{2*i+1}."
            self.time_layers.append(dict(
                norm=(W[t + "norm.weight"], W[t + "norm.bias"]),
                conv=(W[t + "conv.weight"].T, W[t + "conv.bias"]),
                mix=(W[t + "mix.weight"].T, W[t + "mix.bias"]),
                d=cfg.dilations[i], r=self.pads[i]))
            self.mix_layers.append(dict(
                norm=(W[m + "norm.weight"], W[m + "norm.bias"]),
                down=(W[m + "down.weight"].T, W[m + "down.bias"]),
                across=(W[m + "across.weight"].T, W[m + "across.bias"]),
                up=(W[m + "up.weight"].T, W[m + "up.bias"])))
        self.reset()

    def reset(self):
        k, c = len(self.edges), self.cfg.channels
        # Frame g covers the last n_fft samples up to the end of push g.
        self.in_buf = np.zeros(self.n_fft, np.float32)
        self.frame = 0
        # Per time layer: ring of normalised inputs (2d+1) and raw inputs (r+1).
        self.rings = [deque([np.zeros((k, c), np.float32)] * (2 * l["d"] + 1), maxlen=2 * l["d"] + 1)
                      for l in self.time_layers]
        self.raws = [deque([np.zeros((k, c), np.float32)] * (l["r"] + 1), maxlen=l["r"] + 1)
                     for l in self.time_layers]
        self.seen = [0] * len(self.time_layers)  # input frames received per layer
        self.specs = deque(maxlen=self.cfg.lookahead + 1)
        self.ola = np.zeros((4, self.n_fft), np.float32)
        # Steady-state window-square envelope of the first hop of a frame:
        # covered by this frame and the earlier ones at offsets m * hop.
        wsq = self.window**2
        self.norm = np.zeros(self.hop, np.float32)
        for m in range(-(-self.n_fft // self.hop)):
            seg = wsq[m * self.hop : (m + 1) * self.hop]
            self.norm[: len(seg)] += seg

    # -- network pieces on one frame ----------------------------------------
    def encode(self, mag):
        f = (np.log(mag + 1e-5) - self.W["feat_mean"]) / self.W["feat_std"]
        return np.stack([f[lo:hi] @ self.W[f"encode.{i}.weight"].T + self.W[f"encode.{i}.bias"]
                         for i, (lo, hi) in enumerate(self.edges)])  # (K, C)

    def band_mix(self, i, x):
        l = self.mix_layers[i]
        y = layer_norm(x, *l["norm"]) @ l["down"][0] + l["down"][1]  # (K, m)
        y = gelu(y.reshape(-1) @ l["across"][0] + l["across"][1]).reshape(y.shape)
        return x + y @ l["up"][0] + l["up"][1]

    def time_step(self, i, x):
        """Feed input frame u to time layer i; return output frame u - r or None."""
        l = self.time_layers[i]
        self.rings[i].append(layer_norm(x, *l["norm"]))
        self.raws[i].append(x)
        self.seen[i] += 1
        if self.seen[i] <= l["r"]:
            return None  # output frame would be negative
        ring = self.rings[i]
        d = l["d"]
        taps = np.concatenate([ring[0], ring[d], ring[2 * d]], -1)  # frames u-2d, u-d, u
        y = gelu(taps @ l["conv"][0] + l["conv"][1]) @ l["mix"][0] + l["mix"][1]
        return self.raws[i][0] + y

    def head(self, x):
        x = layer_norm(x, self.W["out_norm.weight"], self.W["out_norm.bias"])
        logits = np.concatenate([
            (x[i] @ self.W[f"decode.{i}.weight"].T + self.W[f"decode.{i}.bias"]).reshape(hi - lo, 4)
            for i, (lo, hi) in enumerate(self.edges)])  # (bins, 4)
        e = np.exp(logits - logits.max(-1, keepdims=True))
        return (e / e.sum(-1, keepdims=True)).T  # (4, bins)

    # -- streaming API ----------------------------------------------------------
    def push(self, hop_samples: np.ndarray) -> np.ndarray | None:
        """Mono block of `hop` samples -> (4, hop) stem samples, or None while
        the pipeline is filling. Output is delayed by `self.latency` samples
        relative to what `Student` computes offline."""
        assert hop_samples.shape == (self.hop,)
        self.in_buf = np.roll(self.in_buf, -self.hop)
        self.in_buf[-self.hop :] = hop_samples
        spec = np.fft.rfft(self.in_buf * self.window)
        self.specs.append(spec)
        x = self.encode(np.abs(spec).astype(np.float32))
        for i in range(len(self.time_layers)):
            x = self.time_step(i, x)
            if x is None:
                return None
            x = self.band_mix(i, x)
        masks = self.head(x)  # for frame self.frame
        spec_t = self.specs[0]
        frames = np.fft.irfft(masks * spec_t[None], self.n_fft).astype(np.float32) * self.window
        self.ola += frames
        out = self.ola[:, : self.hop] / self.norm
        self.ola = np.roll(self.ola, -self.hop, axis=1)
        self.ola[:, -self.hop :] = 0
        self.frame += 1
        return out


def main() -> None:
    import soundfile as sf
    import torch

    from evaluate import StudentSep

    ap = argparse.ArgumentParser()
    ap.add_argument("--ckpt", type=Path, required=True)
    ap.add_argument("--wav", type=Path, required=True)
    ap.add_argument("--seconds", type=float, default=20)
    args = ap.parse_args()

    x, sr = sf.read(args.wav, dtype="float32", always_2d=True)
    start = len(x) // 3
    x = x[start : start + int(args.seconds * sr)].mean(1)
    s = StreamingStudent(args.ckpt)
    hop = s.hop
    outs, t0, times = [], time.perf_counter(), []
    for i in range(0, len(x) - hop + 1, hop):
        t = time.perf_counter()
        y = s.push(x[i : i + hop])
        times.append(time.perf_counter() - t)
        if y is not None:
            outs.append(y)
    stream = np.concatenate(outs, 1)
    print(f"latency {s.latency} samples = {1000*s.latency/sr:.1f} ms; "
          f"per hop {1e3*np.mean(times):.2f} ms mean (budget {1e3*hop/sr:.1f} ms), numpy single thread")

    # Streaming frame g ends at sample (g+1)*hop. Offline (center=True) frame f
    # of an input delayed by D = n_fft/2 mod hop ends at (f+2)*hop, so the grids
    # coincide; stream output index k is input sample k - (n_fft - hop).
    D = (s.n_fft // 2) % hop
    off = StudentSep(args.ckpt, torch.device("cpu"))
    off.model.float()
    with torch.no_grad():
        ref, _ = off.model(torch.from_numpy(np.concatenate([np.zeros(D, np.float32), x]))[None, None])
    ref = ref[0, :, 0].numpy()
    lag = (s.n_fft - hop) - D
    a = stream[:, 4 * sr : 8 * sr]
    b = ref[:, 4 * sr - lag : 8 * sr - lag]
    print(f"max |stream - offline| = {np.abs(a - b).max():.2e} over 4 s "
          f"(stem peak {np.abs(b).max():.2f})")


if __name__ == "__main__":
    main()
