"""Melodia on separated stems: the instrumental lead ("other" = mix - drums - bass - vocals) and
the bass line (bass stem, pitch range lowered to 27.5-440 Hz).

Reads teacher stem files (`<id>.npy`, float16 (4, n) mono 44.1 kHz: mix, drums, bass, vocals)
and writes `<out>/<source>/<id>.npz` (f0 per 128-sample hop) and `<id>.mid`.

    python melodia_stems.py stems_songs out --jobs 16
"""

import argparse
import os
from multiprocessing import Pool
from pathlib import Path

import numpy as np

from melodia_batch import HOP, SR, f0_to_notes, write_midi

SETTINGS = {
    # Melodia defaults: salience from 55 Hz, peaks from 80 Hz
    "lead": dict(referenceFrequency=55, minFrequency=80, maxFrequency=20000),
    # one octave lower reference so the 5-octave salience grid covers 27.5-880 Hz
    "bass": dict(referenceFrequency=27.5, minFrequency=30, maxFrequency=440),
}
PROGRAM = {"lead": 81, "bass": 38}


def work(job):
    path, out = job
    import essentia.standard as es

    arr = np.load(path, mmap_mode="r").astype(np.float32)
    mix, drums, bass, vocals = arr
    sources = {"lead": mix - drums - bass - vocals, "bass": bass}
    msg = []
    for name, x in sources.items():
        dst = out / name / f"{path.stem}.mid"
        if dst.exists():
            continue
        x = es.EqualLoudness(sampleRate=SR)(np.ascontiguousarray(x))
        f0, conf = es.PredominantPitchMelodia(sampleRate=SR, frameSize=2048, hopSize=HOP, **SETTINGS[name])(x)
        f0 = np.asarray(f0, np.float32)
        np.savez_compressed(out / name / f"{path.stem}.npz", f0=f0, conf=np.asarray(conf, np.float32), hop=HOP, sr=SR)
        notes = f0_to_notes(f0, HOP / SR, min_dur=0.1 if name == "bass" else 0.08)
        write_midi(notes, dst, program=PROGRAM[name])
        msg.append(f"{name} {len(notes)} notes")
    return path.stem, ", ".join(msg) or "cached"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("stems", type=Path)
    ap.add_argument("out", type=Path)
    ap.add_argument("--jobs", type=int, default=16)
    args = ap.parse_args()
    for name in SETTINGS:
        (args.out / name).mkdir(parents=True, exist_ok=True)
    files = sorted(p for p in args.stems.glob("*.npy") if ".tmp" not in p.name)
    os.nice(10)
    with Pool(args.jobs) as pool:
        for i, (sid, msg) in enumerate(pool.imap_unordered(work, [(f, args.out) for f in files])):
            print(f"[{i + 1}/{len(files)}] {sid}: {msg}", flush=True)


if __name__ == "__main__":
    main()
