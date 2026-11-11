"""Hindsight drop labels: where kick and bass come back after a breakdown.

With the whole track in view a drop is easy to find; the drop model learns to
anticipate these from the past only. Works on any track with a downbeat grid.

    uv run --with numpy --with matplotlib audio-model/drops.py \
        --features audio-model/data/features --plot drops.png
"""

import argparse
import json
from dataclasses import dataclass
from pathlib import Path

import numpy as np

LOW_BAND_MAX_HZ = 150.0
# Per-bar kick/bass energy, relative to the track's 90th percentile.
FULL_LEVEL = 0.7
# The four bars after a drop must exceed the four bars before by this much.
MIN_RISE = 0.4
# The bar before the drop must be below this, and the drop bar must jump.
MAX_LEVEL_BEFORE = 0.6
MIN_JUMP = 0.25
BARS_AFTER = 4
MIN_BARS_APART = 4


@dataclass
class Drop:
    frame: int  # mel frame (100 fps) of the drop's downbeat
    bar_frames: float  # length of the bar before the drop, in mel frames
    breakdown_bars: int  # consecutive bars below MAX_LEVEL_BEFORE before it


def bar_energy(mel: np.ndarray, downbeats: np.ndarray, centers: np.ndarray) -> np.ndarray:
    """Mean kick/bass magnitude per bar, normalized to the track's loud bars."""
    low = (np.expm1(mel[:, centers <= LOW_BAND_MAX_HZ].astype(np.float32)) / 1000).sum(1)
    energy = np.array([low[a:b].mean() for a, b in zip(downbeats[:-1], downbeats[1:])])
    return energy / max(float(np.percentile(energy, 90)), 1e-9)


def find_drops(mel: np.ndarray, downbeats: np.ndarray, centers: np.ndarray) -> list[Drop]:
    if len(downbeats) < 2 * BARS_AFTER + 1:
        return []
    e = bar_energy(mel, downbeats, centers)
    found: list[int] = []
    for b in range(4, len(e) - BARS_AFTER + 1):
        before, after = e[b - 4 : b], e[b : b + BARS_AFTER]
        if (
            after.mean() > FULL_LEVEL
            and after.min() > FULL_LEVEL - 0.2
            and after.mean() - before.mean() > MIN_RISE
            and e[b - 1] < MAX_LEVEL_BEFORE
            and e[b] - e[b - 1] > MIN_JUMP
            and (not found or b - found[-1] >= MIN_BARS_APART)
        ):
            found.append(b)
    drops = []
    for b in found:
        n = 0
        while b - n - 1 >= 0 and e[b - n - 1] < MAX_LEVEL_BEFORE:
            n += 1
        drops.append(Drop(int(downbeats[b]), float(downbeats[b] - downbeats[b - 1]), n))
    return drops


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--features", type=Path, required=True)
    ap.add_argument("--plot", type=Path, help="spectrograms with drops for a few tracks")
    ap.add_argument("--titles", nargs="*", default=[], help="title substrings to plot")
    args = ap.parse_args()

    index = json.loads((args.features / "index.json").read_text())
    centers = np.asarray(index["frontend"]["band_centers_hz"])
    counts = []
    for t in index["tracks"]:
        with np.load(args.features / t["file"]) as data:
            counts.append(len(find_drops(data["mel"], np.flatnonzero(data["downbeat"]), centers)))
    print(
        f"{sum(counts)} drops in {sum(c > 0 for c in counts)} of {len(counts)} tracks; "
        f"drops per track: {np.bincount(counts).tolist()}"
    )

    if args.plot:
        import matplotlib

        matplotlib.use("Agg")
        import matplotlib.pyplot as plt

        tracks = [t for t in index["tracks"] if any(s in t["title"] for s in args.titles)][:8]
        fig, axes = plt.subplots(len(tracks), 1, figsize=(13, 2.1 * len(tracks)), squeeze=False)
        for ax, t in zip(axes[:, 0], tracks):
            with np.load(args.features / t["file"]) as data:
                mel = data["mel"].astype(np.float32)
                downbeats = np.flatnonzero(data["downbeat"])
            e = bar_energy(mel, downbeats, centers)
            ax.imshow(mel.T[::-1], aspect="auto", cmap="magma", extent=[0, len(mel) / 100, 0, 1.6])
            ax.step(downbeats[:-1] / 100, np.clip(e, 0, 1.5), where="post", color="cyan", lw=1.2)
            for drop in find_drops(mel, downbeats, centers):
                ax.axvline(drop.frame / 100, color="lime", lw=2)
            ax.set_title(f"{t['artist']} – {t['title']}"[:70], fontsize=9, loc="left")
            ax.set_yticks([])
        axes[-1, 0].set_xlabel("seconds (cyan: per-bar kick/bass energy, green: drop)")
        plt.tight_layout()
        plt.savefig(args.plot, dpi=80)


if __name__ == "__main__":
    main()
