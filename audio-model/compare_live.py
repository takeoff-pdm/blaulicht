# /// script
# requires-python = ">=3.11"
# dependencies = ["numpy", "scipy", "torch"]
# ///
"""Score a trained model and the engine's live trackers on the same labels.

The live trackers' beats come from `beat_eval --trace-dir`, run on audio
files named `<track id>.<ext>`. Everything is matched against the Rekordbox
grid with train.py's ±70 ms F-measure after a 10 s warm-up.

    uv run audio-model/compare_live.py --features audio-model/data/features \
        --model audio-model/runs/baseline/model.pt \
        --trace legacy=/path/to/trace_legacy --trace tracker=/path/to/trace_new
"""

import argparse
import csv
from pathlib import Path

import numpy as np
import torch

from check_alignment import label_offsets_ms
from model import BeatTCN, ModelConfig
from train import MATCH_TOLERANCE_FRAMES, activations, f1, f_measure, load_tracks, pick_peaks, split

# Shortest gap between two causally fired beats (200 BPM is 30 frames).
REFRACTORY_FRAMES = 25


def causal_peaks(activation: np.ndarray, threshold: float) -> np.ndarray:
    """Fire at frame t + 1 when frame t is the maximum of the last tolerance
    window, at least the next frame, and above `threshold`: what a live
    picker can do with one frame of delay."""
    fired = []
    last = -REFRACTORY_FRAMES
    for t in range(MATCH_TOLERANCE_FRAMES, len(activation) - 1):
        a = activation[t]
        if (
            a >= threshold
            and a >= activation[t + 1]
            and a >= activation[t - MATCH_TOLERANCE_FRAMES : t].max()
            and t - last >= REFRACTORY_FRAMES
        ):
            fired.append(t + 1)
            last = t
    return np.asarray(fired, int)


def live_beats(path: Path) -> np.ndarray:
    with path.open() as f:
        times = [float(r["time_ms"]) for r in csv.DictReader(f) if r["kind"] == "beat"]
    return np.floor(np.asarray(times) / 10).astype(int)


def score(predicted: list[np.ndarray], references: list[np.ndarray]) -> dict:
    totals = np.zeros(3, int)
    offsets: list[int] = []
    per_track = []
    for p, r in zip(predicted, references):
        h, n_p, n_r, o = f_measure(p, r)
        totals += (h, n_p, n_r)
        offsets += o
        per_track.append(f1(h, n_p, n_r))
    return {
        "f1": f1(*totals),
        "precision": totals[0] / max(totals[1], 1),
        "recall": totals[0] / max(totals[2], 1),
        "offset_ms": 10 * float(np.mean(offsets)) if offsets else float("nan"),
        "offset_std_ms": 10 * float(np.std(offsets)) if offsets else float("nan"),
        "per_track": per_track,
    }


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--features", type=Path, required=True)
    ap.add_argument("--model", type=Path, required=True)
    ap.add_argument("--trace", action="append", default=[], help="name=trace_dir")
    ap.add_argument("--device", default="mps" if torch.backends.mps.is_available() else "cpu")
    args = ap.parse_args()

    _, tracks = load_tracks(args.features)
    offsets, _ = label_offsets_ms(args.features)
    _, val = split(tracks, offsets)

    ckpt = torch.load(args.model, weights_only=False)
    model = BeatTCN(ModelConfig(**ckpt["model_config"]))
    model.load_state_dict(ckpt["model"])
    device = torch.device(args.device)
    acts = activations(model.to(device), val, ckpt["mean"], ckpt["std"], device)
    lead = round(ckpt["lead_ms"] / 10)

    results = {}
    beats = [t.beats for t in val]
    threshold = ckpt["thresholds"]["beat"]
    results["model, offline peaks"] = score([pick_peaks(a[0], threshold) + lead for a in acts], beats)
    # The causal picker has its own best threshold; sweep it like train.py does.
    results["model, causal"] = max(
        (score([causal_peaks(a[0], th) + lead for a in acts], beats) for th in
         [*np.arange(0.3, 0.9, 0.05), 0.9, 0.93, 0.95, 0.97, 0.98, 0.99]),
        key=lambda s: s["f1"],
    )
    for spec in args.trace:
        name, trace_dir = spec.split("=", 1)
        predicted = [live_beats(Path(trace_dir) / f"{t.id}.csv") for t in val]
        results[name] = score(predicted, beats)

    print(f"beat F1 on {len(val)} validation tracks (±70 ms vs Rekordbox grid, after 10 s)\n")
    print(f"{'':24} {'F1':>6} {'prec':>6} {'recall':>6} {'offset':>8} {'± std':>7}")
    for name, s in results.items():
        print(
            f"{name:24} {s['f1']:6.3f} {s['precision']:6.3f} {s['recall']:6.3f} "
            f"{s['offset_ms']:+7.1f}ms {s['offset_std_ms']:6.1f}"
        )

    names = list(results)
    print("\nper track F1 (worst for the causal model first):")
    print("  " + "  ".join(f"{n[:14]:>14}" for n in names))
    rows = sorted(
        zip(*(results[n]["per_track"] for n in names), (t.title for t in val)),
        key=lambda row: row[1],
    )
    for row in rows:
        print("  " + "  ".join(f"{v:14.3f}" for v in row[:-1]) + f"  {row[-1][:50]}")


if __name__ == "__main__":
    main()
