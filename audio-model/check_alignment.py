# /// script
# requires-python = ">=3.11"
# dependencies = ["numpy"]
# ///
"""Check that beat targets line up with the onsets in the exported features.

For each constant-tempo track, finds the lag (in ms) at which the low-band
mel flux peaks around the target frames. The same measurement on a synthetic
kick track with exact beat times gives the lag a perfectly aligned label
produces; the difference is the label offset.

    uv run audio-model/check_alignment.py --features audio-model/data/features
"""

import argparse
import json
from pathlib import Path

import numpy as np

import features

LOW_BAND_MAX_HZ = 150.0
LAGS_FRAMES = np.arange(-5.0, 10.0, 0.1)


def low_flux(mel: np.ndarray, centers: np.ndarray) -> np.ndarray:
    low = mel[:, centers <= LOW_BAND_MAX_HZ].astype(np.float32)
    flux = np.maximum(np.diff(low, axis=0, prepend=low[:1]), 0.0).sum(axis=1)
    return flux


def peak_lag_ms(flux: np.ndarray, beat_frames: np.ndarray, frame_rate: int) -> float:
    frames = np.arange(len(flux))
    beat_frames = beat_frames[(beat_frames > 100) & (beat_frames < len(flux) - 100)]
    scores = [np.interp(beat_frames + lag, frames, flux).mean() for lag in LAGS_FRAMES]
    return float(LAGS_FRAMES[int(np.argmax(scores))] * 1000 / frame_rate)


def synthetic_lag_ms(centers: np.ndarray, frame_rate: int) -> float:
    sr = features.SAMPLE_RATE
    rng = np.random.default_rng(0)
    # Beat times off the frame grid, so the result covers all sub-frame phases.
    beats = np.cumsum(np.full(240, 60 / 124.0)) + rng.uniform(0, 0.01)
    t = np.arange(int(0.15 * sr)) / sr
    kick = np.sin(2 * np.pi * (50 + 100 * np.exp(-t * 30)) * t) * np.exp(-t * 12)
    pcm = np.zeros(int((beats[-1] + 1) * sr), np.float32)
    for b in beats:
        i = int(round(b * sr))
        pcm[i : i + len(kick)] += kick[: len(pcm) - i]
    flux = low_flux(features.extract(pcm), centers)
    return peak_lag_ms(flux, beats * frame_rate, frame_rate)


def label_offsets_ms(features_dir: Path) -> tuple[dict[str, float], float]:
    """Label offset per constant-tempo track, and the synthetic baseline lag."""
    index = json.loads((features_dir / "index.json").read_text())
    config = index["frontend"]
    rate = config["frame_rate"]
    centers = np.asarray(config["band_centers_hz"])

    expected = synthetic_lag_ms(centers, rate)
    offsets = {}
    for track in index["tracks"]:
        if track["variable_tempo"]:
            continue
        with np.load(features_dir / track["file"]) as data:
            flux = low_flux(data["mel"], centers)
            beat_frames = np.flatnonzero(data["beat"]).astype(float)
        offsets[track["id"]] = peak_lag_ms(flux, beat_frames, rate) - expected
    return offsets, expected


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--features", type=Path, required=True)
    args = ap.parse_args()

    offsets, expected = label_offsets_ms(args.features)
    lags = np.asarray(list(offsets.values()))
    p10, p50, p90 = np.percentile(lags, [10, 50, 90])
    print(f"synthetic kick lag {expected:.1f} ms (perfect labels)")
    print(
        f"label offset over {len(lags)} constant-tempo tracks: median {p50:+.1f} ms, "
        f"p10 {p10:+.1f}, p90 {p90:+.1f}"
    )
    print(f"tracks off by more than 20 ms: {int((np.abs(lags) > 20).sum())}")


if __name__ == "__main__":
    main()
