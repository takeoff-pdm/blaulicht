# /// script
# requires-python = ">=3.11"
# dependencies = ["numpy", "scipy", "torch"]
# ///
"""Check the Rust BeatModel against PyTorch on the validation tracks.

Decodes each track, runs the Rust front end + Rust model through the
`audio-features` binary, runs PyTorch on the same Rust mel frames, and
compares the activations. Then scores beat F1 from the Rust activations.

    uv run audio-model/check_rust_model.py --features audio-model/data/features \
        --run audio-model/runs/baseline
"""

import argparse
import time
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

import numpy as np
import torch

import features
from check_alignment import label_offsets_ms
from compare_live import causal_peaks, score
from export_model import export
from model import BeatTCN, ModelConfig
from train import load_tracks, pick_peaks, split


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--features", type=Path, required=True)
    ap.add_argument("--run", type=Path, required=True)
    args = ap.parse_args()

    ckpt_path = args.run / "model.pt"
    bin_path = args.run / "beat_model.bin"
    export(ckpt_path, bin_path)

    index_tracks, tracks = load_tracks(args.features)
    offsets, _ = label_offsets_ms(args.features)
    _, val = split(tracks, offsets)
    paths = {t["id"]: t["path"] for t in index_tracks["tracks"]}

    ckpt = torch.load(ckpt_path, map_location="cpu", weights_only=False)
    model = BeatTCN(ModelConfig(**ckpt["model_config"])).eval()
    model.load_state_dict(ckpt["model"])
    torch.set_num_threads(4)

    def run(track):
        pcm = features.decode(paths[track.id])
        started = time.perf_counter()
        rust = features.beat_activations(pcm, bin_path)
        rust_seconds = time.perf_counter() - started
        mel = features.extract(pcm)
        with torch.no_grad():
            x = torch.from_numpy(((mel - ckpt["mean"]) / ckpt["std"]).T[None].astype(np.float32))
            reference = torch.sigmoid(model(x))[0].numpy()
        return rust, reference, rust_seconds, len(pcm) / features.SAMPLE_RATE

    with ThreadPoolExecutor(4) as pool:
        results = list(pool.map(run, val))

    diffs = [float(np.abs(r - ref).max()) for r, ref, _, _ in results]
    audio_s = sum(r[3] for r in results)
    rust_s = sum(r[2] for r in results)
    print(f"{len(val)} tracks, {audio_s / 60:.0f} min of audio")
    print(f"max |rust - torch| activation difference: {max(diffs):.2e} (median {np.median(diffs):.2e})")
    print(
        f"rust decode-free pipeline (mel + model, 1 thread): {rust_s:.1f} s "
        f"= {audio_s / rust_s:.0f}x realtime, {rust_s / audio_s * 1e6 / 100:.1f} us per frame"
    )

    beats = [t.beats for t in val]
    lead = round(ckpt["lead_ms"] / 10)
    rust_acts = [r[0] for r in results]
    offline = score([pick_peaks(a[0], ckpt["thresholds"]["beat"]) + lead for a in rust_acts], beats)
    causal = max(
        (score([causal_peaks(a[0], th) + lead for a in rust_acts], beats)
         for th in [*np.arange(0.3, 0.9, 0.05), 0.9, 0.93, 0.95, 0.97, 0.98, 0.99]),
        key=lambda s: s["f1"],
    )
    print(f"beat F1 from rust activations: offline {offline['f1']:.3f}, causal {causal['f1']:.3f}")


if __name__ == "__main__":
    main()
