"""Melodia teacher labels for distillation, on the student's 100 fps frame grid.

For every clip writes `<out>/<id>.npy`: float32 f0 in Hz per student frame (frame g is centred
on sample g * 441 at 44.1 kHz, as torch.stft(center=True) frames it; 0 = unvoiced). Melodia's
own frames (hop 128, frame i centred on sample i * 128) are sampled at the nearest frame.

Audio comes either from stem-teacher files (`--npy-dir`, float16 (4, n) mono, row 0 = mix,
used in place) or from audio files (`--list`), which are decoded to `<audio-out>/<id>.npy`
(float16 mono 44.1 kHz) so training reads one format.

    python melodia_labels.py --npy-dir /localhome/sokrates/stems/data/teacher/fma --out labels/fma
    python melodia_labels.py --list songs_remote.txt --audio-out audio_npy/songs --out labels/songs
"""

import argparse
import hashlib
import os
import subprocess
from multiprocessing import Pool
from pathlib import Path

import numpy as np

SR = 44100
MEL_HOP = 128
HOP = 441


def melodia(x: np.ndarray) -> np.ndarray:
    import essentia.standard as es

    x = es.EqualLoudness(sampleRate=SR)(x.astype(np.float32))
    f0, _ = es.PredominantPitchMelodia(sampleRate=SR, frameSize=2048, hopSize=MEL_HOP)(x)
    return np.asarray(f0, np.float32)


def to_grid(f0: np.ndarray, n_samples: int) -> np.ndarray:
    frames = n_samples // HOP + 1  # torch.stft(center=True) frame count
    idx = np.clip(np.round(np.arange(frames) * HOP / MEL_HOP).astype(int), 0, len(f0) - 1)
    return f0[idx]


def decode(path: str) -> np.ndarray:
    pcm = subprocess.run(
        ["ffmpeg", "-v", "error", "-i", path, "-ac", "1", "-ar", str(SR), "-f", "f32le", "-"],
        check=True, capture_output=True,
    ).stdout
    return np.frombuffer(pcm, np.float32)


def work(job):
    kind, src, out, audio_out = job
    try:
        if kind == "npy":
            sid = Path(src).stem
            if (out / f"{sid}.npy").exists():
                return sid, "cached"
            x = np.load(src, mmap_mode="r")[0].astype(np.float32)
        else:
            sid = hashlib.sha1(src.encode()).hexdigest()[:12]
            if (out / f"{sid}.npy").exists():
                return sid, "cached"
            x = decode(src)
            np.save(audio_out / f"{sid}.npy", x.astype(np.float16))
        lab = to_grid(melodia(x), len(x))
        np.save(out / f"{sid}.npy", lab)
        return sid, f"{len(x) / SR:.0f} s, voiced {np.mean(lab > 0):.0%}"
    except Exception as e:
        return src, f"error: {e}"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--npy-dir", type=Path)
    ap.add_argument("--list", type=Path)
    ap.add_argument("--audio-out", type=Path)
    ap.add_argument("--out", type=Path, required=True)
    ap.add_argument("--jobs", type=int, default=24)
    args = ap.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    if args.npy_dir:
        jobs = [("npy", str(p), args.out, None) for p in sorted(args.npy_dir.glob("*.npy")) if ".tmp" not in p.name]
    else:
        args.audio_out.mkdir(parents=True, exist_ok=True)
        paths = [l.strip() for l in args.list.read_text().splitlines() if l.strip()]
        jobs = [("audio", p, args.out, args.audio_out) for p in paths]
    os.nice(10)
    with Pool(args.jobs) as pool:
        for i, (sid, msg) in enumerate(pool.imap_unordered(work, jobs, chunksize=4)):
            if i % 50 == 0 or "error" in msg:
                print(f"[{i + 1}/{len(jobs)}] {sid}: {msg}", flush=True)
    print("done", flush=True)


if __name__ == "__main__":
    main()
