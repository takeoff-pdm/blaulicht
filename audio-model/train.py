# /// script
# requires-python = ">=3.11"
# dependencies = ["numpy", "scipy", "torch"]
# ///
"""Train the causal beat/downbeat TCN on exported features.

    uv run audio-model/train.py --features audio-model/data/features --run baseline

Writes `audio-model/runs/<run>/` with the best checkpoint (`model.pt`), the
per-epoch log (`log.jsonl`) and the final validation scores (`scores.json`).
"""

import argparse
import json
import math
import time
from dataclasses import dataclass
from pathlib import Path

import numpy as np
import torch
import torch.nn.functional as F
from scipy.ndimage import maximum_filter1d
from scipy.signal import lfilter

from check_alignment import label_offsets_ms
from model import BeatTCN, ModelConfig

HERE = Path(__file__).resolve().parent
VAL_PERCENT = 15
# Tracks whose labels disagree with their kick onsets by more than this stay
# out of validation (see check_alignment.py).
MAX_VAL_LABEL_OFFSET_MS = 20.0
# Scoring matches beat_eval: ±70 ms, ignoring the first 10 s.
MATCH_TOLERANCE_FRAMES = 7
WARMUP_FRAMES = 1_000


@dataclass
class Track:
    id: str
    title: str
    mel: np.ndarray  # (frames, n_mels), raw log-mel
    beats: np.ndarray  # frame indices
    downbeats: np.ndarray


def load_tracks(features_dir: Path) -> tuple[dict, list[Track]]:
    index = json.loads((features_dir / "index.json").read_text())
    tracks = []
    for t in index["tracks"]:
        with np.load(features_dir / t["file"]) as data:
            tracks.append(
                Track(
                    id=t["id"],
                    title=f"{t['artist']} – {t['title']}",
                    mel=data["mel"].astype(np.float32),
                    beats=np.flatnonzero(data["beat"]),
                    downbeats=np.flatnonzero(data["downbeat"]),
                )
            )
    return index, tracks


def split(tracks: list[Track], offsets: dict[str, float]) -> tuple[list[Track], list[Track]]:
    def is_val(t: Track) -> bool:
        return int(t.id, 16) % 100 < VAL_PERCENT

    def clean(t: Track) -> bool:
        return abs(offsets.get(t.id, 0.0)) <= MAX_VAL_LABEL_OFFSET_MS

    train = [t for t in tracks if not is_val(t)]
    val = [t for t in tracks if is_val(t) and clean(t)]
    return train, val


# ---------------------------------------------------------------------------
# Crops and augmentation
# ---------------------------------------------------------------------------


def widen(frames: np.ndarray, length: int) -> np.ndarray:
    """1 at each event frame, 0.5 at its neighbours."""
    target = np.zeros(length, np.float32)
    for offset, value in ((-1, 0.5), (1, 0.5), (0, 1.0)):
        idx = frames + offset
        idx = idx[(idx >= 0) & (idx < length)]
        target[idx] = np.maximum(target[idx], value)
    return target


def crop(
    track: Track, length: int, rng: np.random.Generator, stretch: float, lead_frames: float
) -> tuple[np.ndarray, np.ndarray]:
    """A `length`-frame crop resampled by `stretch` source frames per output
    frame (stretch > 1 speeds the music up), with targets moved `lead_frames`
    earlier."""
    span = math.ceil(length * stretch) + 2
    n = len(track.mel)
    start = int(rng.integers(0, n - span)) if n > span else 0
    pos = start + np.arange(length) * stretch
    i0 = np.minimum(pos.astype(int), n - 1)
    i1 = np.minimum(i0 + 1, n - 1)
    frac = (pos - i0)[:, None]
    mel = track.mel[i0] * (1 - frac) + track.mel[i1] * frac
    valid = pos <= n - 1
    mel[~valid] = 0.0

    targets = []
    for events in (track.beats, track.downbeats):
        # Label frame k means the beat lies in [k, k + 1); use its centre.
        out = np.floor((events + 0.5 - start) / stretch - lead_frames).astype(int)
        targets.append(widen(out, length))
    return mel, np.stack(targets)


def augment(mel: np.ndarray, rng: np.random.Generator) -> np.ndarray:
    """Room and mic variation applied to linear band magnitudes. Gain and EQ
    are exact; noise and reverb are magnitude-domain approximations."""
    lin = np.expm1(mel) / 1_000.0
    n_mels = mel.shape[1]
    band = np.linspace(-1.0, 1.0, n_mels)

    eq_db = rng.uniform(-18, 6) + rng.uniform(-6, 6) * band
    for _ in range(2):
        eq_db += rng.uniform(-6, 6) * np.exp(-0.5 * ((band - rng.uniform(-1, 1)) / 0.15) ** 2)
    lin = lin * 10 ** (eq_db / 20)

    if rng.random() < 0.6:
        rt60 = rng.uniform(0.2, 1.5)
        decay = 10 ** (-3 * 0.01 / rt60)
        tail = lfilter([1 - decay], [1, -decay], lin, axis=0)
        lin = np.sqrt(lin**2 + (rng.uniform(0.1, 0.7) * tail) ** 2)

    if rng.random() < 0.7:
        level = 10 ** (rng.uniform(-80, -40) / 20) * 10 ** (-0.5 * (band + 1))
        noise = level * np.abs(1 + 0.3 * rng.standard_normal(lin.shape))
        lin = np.sqrt(lin**2 + noise**2)

    return np.log1p(1_000.0 * lin).astype(np.float32)


class Sampler:
    def __init__(self, tracks: list[Track], args, seed: int):
        self.tracks = tracks
        weights = np.array([len(t.mel) for t in tracks], float)
        self.p = weights / weights.sum()
        self.rng = np.random.default_rng(seed)
        self.length = int(args.crop_s * 100)
        self.augment = not args.no_augment
        self.lead_frames = args.lead_ms / 10.0

    def batch(self, size: int) -> tuple[np.ndarray, np.ndarray]:
        mels, targets = [], []
        for i in self.rng.choice(len(self.tracks), size, p=self.p):
            stretch = self.rng.uniform(0.85, 1.15) if self.augment and self.rng.random() < 0.7 else 1.0
            mel, target = crop(self.tracks[i], self.length, self.rng, stretch, self.lead_frames)
            if self.augment:
                mel = augment(mel, self.rng)
            mels.append(mel.T)
            targets.append(target)
        return np.stack(mels), np.stack(targets)


# ---------------------------------------------------------------------------
# Evaluation
# ---------------------------------------------------------------------------


def pick_peaks(activation: np.ndarray, threshold: float) -> np.ndarray:
    """Local maxima above `threshold`, at least a tolerance window apart."""
    is_max = activation == maximum_filter1d(activation, 2 * MATCH_TOLERANCE_FRAMES + 1)
    peaks = np.flatnonzero(is_max & (activation >= threshold))
    kept = []
    for p in peaks:
        if not kept or p - kept[-1] > MATCH_TOLERANCE_FRAMES:
            kept.append(p)
    return np.asarray(kept, int)


def f_measure(predicted: np.ndarray, reference: np.ndarray) -> tuple[int, int, int, list[int]]:
    """(hits, predicted, reference, offsets) with one-to-one matching."""
    predicted = predicted[predicted >= WARMUP_FRAMES]
    reference = reference[reference >= WARMUP_FRAMES]
    used = np.zeros(len(reference), bool)
    offsets = []
    j = 0
    for p in predicted:
        while j < len(reference) and reference[j] < p - MATCH_TOLERANCE_FRAMES:
            j += 1
        best = None
        k = j
        while k < len(reference) and reference[k] <= p + MATCH_TOLERANCE_FRAMES:
            if not used[k] and (best is None or abs(reference[k] - p) < abs(reference[best] - p)):
                best = k
            k += 1
        if best is not None:
            used[best] = True
            offsets.append(int(p - reference[best]))
    return len(offsets), len(predicted), len(reference), offsets


def f1(hits: int, predicted: int, reference: int) -> float:
    return 2 * hits / max(predicted + reference, 1)


@torch.no_grad()
def activations(model: BeatTCN, tracks: list[Track], mean, std, device) -> list[np.ndarray]:
    model.eval()
    out = []
    for t in tracks:
        x = torch.from_numpy(((t.mel - mean) / std).T[None]).to(device)
        out.append(torch.sigmoid(model(x))[0].cpu().numpy())
    model.train()
    return out


def evaluate(acts: list[np.ndarray], tracks: list[Track], lead_frames: int) -> dict:
    """Best-threshold F1 for beats and downbeats over all validation tracks."""
    scores = {}
    for kind, channel in (("beat", 0), ("downbeat", 1)):
        best = None
        # Positive weighting pushes activations up, so sample densely near 1.
        for threshold in [*np.arange(0.1, 0.9, 0.05), 0.9, 0.93, 0.95, 0.97, 0.98, 0.99]:
            totals = np.zeros(3, int)
            offsets: list[int] = []
            per_track = []
            for act, t in zip(acts, tracks):
                reference = t.beats if kind == "beat" else t.downbeats
                predicted = pick_peaks(act[channel], threshold) + lead_frames
                h, p, r, o = f_measure(predicted, reference)
                totals += (h, p, r)
                offsets += o
                per_track.append(f1(h, p, r))
            score = f1(*totals)
            if best is None or score > best["f1"]:
                best = {
                    "f1": score,
                    "threshold": round(float(threshold), 2),
                    "precision": totals[0] / max(totals[1], 1),
                    "recall": totals[0] / max(totals[2], 1),
                    "offset_mean_ms": 10 * float(np.mean(offsets)) if offsets else 0.0,
                    "per_track_f1": per_track,
                }
        scores[kind] = best
    return scores


# ---------------------------------------------------------------------------


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--features", type=Path, required=True)
    ap.add_argument("--run", required=True, help="name of the run directory")
    ap.add_argument("--epochs", type=int, default=30)
    ap.add_argument("--steps", type=int, default=200, help="steps per epoch")
    ap.add_argument("--batch", type=int, default=32)
    ap.add_argument("--crop-s", type=float, default=15.0)
    ap.add_argument("--lr", type=float, default=2e-3)
    ap.add_argument("--channels", type=int, default=64)
    ap.add_argument("--lead-ms", type=float, default=0.0, help="predict beats this early")
    ap.add_argument("--no-augment", action="store_true")
    ap.add_argument("--seed", type=int, default=0)
    ap.add_argument("--device", default="mps" if torch.backends.mps.is_available() else "cpu")
    args = ap.parse_args()

    torch.manual_seed(args.seed)
    run_dir = HERE / "runs" / args.run
    run_dir.mkdir(parents=True, exist_ok=True)
    device = torch.device(args.device)

    index, tracks = load_tracks(args.features)
    offsets, _ = label_offsets_ms(args.features)
    train, val = split(tracks, offsets)
    print(f"train {len(train)} tracks, val {len(val)} tracks")

    stacked = np.concatenate([t.mel for t in train])
    mean = stacked.mean(0).astype(np.float32)
    std = (stacked.std(0) + 1e-3).astype(np.float32)
    beat_rate = sum(len(t.beats) for t in train) / len(stacked)
    downbeat_rate = sum(len(t.downbeats) for t in train) / len(stacked)
    del stacked
    pos_weight = torch.tensor(
        [(1 - beat_rate) / beat_rate, (1 - downbeat_rate) / downbeat_rate], device=device
    )[:, None]

    config = ModelConfig(n_mels=mean.shape[0], channels=args.channels)
    model = BeatTCN(config).to(device)
    n_params = sum(p.numel() for p in model.parameters())
    print(
        f"{n_params / 1e3:.0f}k params, receptive field {config.receptive_field() / 100:.1f} s, "
        f"pos_weight {pos_weight.squeeze().tolist()}"
    )

    optimizer = torch.optim.AdamW(model.parameters(), lr=args.lr, weight_decay=1e-2)
    total_steps = args.epochs * args.steps
    schedule = torch.optim.lr_scheduler.OneCycleLR(
        optimizer, args.lr, total_steps=total_steps, pct_start=0.05
    )
    sampler = Sampler(train, args, args.seed)
    mean_t = torch.from_numpy(mean).to(device)[:, None]
    std_t = torch.from_numpy(std).to(device)[:, None]
    lead_frames = round(args.lead_ms / 10)

    log = (run_dir / "log.jsonl").open("w")
    best_score = -1.0
    started = time.time()
    for epoch in range(1, args.epochs + 1):
        losses = []
        for _ in range(args.steps):
            mel, target = sampler.batch(args.batch)
            x = (torch.from_numpy(mel).to(device) - mean_t) / std_t
            y = torch.from_numpy(target).to(device)
            loss = F.binary_cross_entropy_with_logits(model(x), y, pos_weight=pos_weight)
            optimizer.zero_grad(set_to_none=True)
            loss.backward()
            torch.nn.utils.clip_grad_norm_(model.parameters(), 1.0)
            optimizer.step()
            schedule.step()
            losses.append(loss.item())

        scores = evaluate(activations(model, val, mean, std, device), val, lead_frames)
        beat, downbeat = scores["beat"], scores["downbeat"]
        entry = {
            "epoch": epoch,
            "loss": float(np.mean(losses)),
            "beat_f1": beat["f1"],
            "downbeat_f1": downbeat["f1"],
            "minutes": (time.time() - started) / 60,
        }
        log.write(json.dumps(entry) + "\n")
        log.flush()
        print(
            f"epoch {epoch:3d}  loss {entry['loss']:.4f}  beat F1 {beat['f1']:.3f} "
            f"(thr {beat['threshold']}, {beat['offset_mean_ms']:+.0f} ms)  "
            f"downbeat F1 {downbeat['f1']:.3f}  {entry['minutes']:.1f} min",
            flush=True,
        )

        score = beat["f1"] + downbeat["f1"]
        if score > best_score:
            best_score = score
            torch.save(
                {
                    "model": model.state_dict(),
                    "model_config": config.to_dict(),
                    "frontend": index["frontend"],
                    "mean": mean,
                    "std": std,
                    "lead_ms": args.lead_ms,
                    "thresholds": {k: scores[k]["threshold"] for k in scores},
                    "epoch": epoch,
                    "args": {k: str(v) for k, v in vars(args).items()},
                },
                run_dir / "model.pt",
            )
            per_track = sorted(
                zip(beat["per_track_f1"], downbeat["per_track_f1"], (t.title for t in val))
            )
            summary = {
                k: {m: v for m, v in s.items() if m != "per_track_f1"} for k, s in scores.items()
            }
            summary["epoch"] = epoch
            summary["val_tracks"] = [
                {"title": title, "beat_f1": b, "downbeat_f1": d} for b, d, title in per_track
            ]
            (run_dir / "scores.json").write_text(json.dumps(summary, indent=1))

    print(f"best checkpoint: {run_dir / 'model.pt'}")


if __name__ == "__main__":
    main()
