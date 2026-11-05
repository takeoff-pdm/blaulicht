"""Melodia (Salamon & Gomez 2012, Essentia's PredominantPitchMelodia) on a list of songs.

Per song writes `<out>/<id>.npz` (f0 in Hz per 128-sample hop at 44.1 kHz, 0 = unvoiced, plus
confidence) and `<out>/<id>.mid` (the f0 segmented into notes). `<out>/index.json` maps ids to
paths. Runs on CPU, one song per process.

    python melodia_batch.py songs.txt out/melodia --root audio --jobs 24
"""

import argparse
import hashlib
import json
import os
from multiprocessing import Pool
from pathlib import Path

import numpy as np

SR = 44100
HOP = 128


def song_id(path: str) -> str:
    return hashlib.sha1(path.encode()).hexdigest()[:12]


def melodia(path: str) -> tuple[np.ndarray, np.ndarray]:
    import essentia.standard as es

    audio = es.MonoLoader(filename=path, sampleRate=SR)()
    audio = es.EqualLoudness(sampleRate=SR)(audio)
    f0, conf = es.PredominantPitchMelodia(sampleRate=SR, frameSize=2048, hopSize=HOP)(audio)
    return np.asarray(f0, np.float32), np.asarray(conf, np.float32)


def f0_to_notes(f0, frame_s, min_dur=0.08, max_gap=0.03, smooth=7):
    """Segment an f0 track into (start s, end s, midi pitch) notes.

    Semitone-rounded pitch after a median filter over `smooth` frames; a note ends when the
    rounded pitch changes or the track is unvoiced for longer than `max_gap`.
    """
    from scipy.ndimage import median_filter

    voiced = f0 > 0
    midi = np.zeros_like(f0)
    midi[voiced] = 69 + 12 * np.log2(f0[voiced] / 440)
    midi = np.where(voiced, median_filter(midi, smooth), 0)
    q = np.where(voiced, np.round(midi), 0).astype(int)
    notes, start, pitch, last_voiced = [], None, 0, -1
    gap = int(round(max_gap / frame_s))
    for i, p in enumerate(q):
        if p and start is not None and p == pitch:
            last_voiced = i
            continue
        if not p and start is not None and i - last_voiced <= gap:
            continue
        if start is not None:
            notes.append((start * frame_s, (last_voiced + 1) * frame_s, pitch))
            start = None
        if p:
            start, pitch, last_voiced = i, p, i
    if start is not None:
        notes.append((start * frame_s, (last_voiced + 1) * frame_s, pitch))
    return [n for n in notes if n[1] - n[0] >= min_dur]


def write_midi(notes, path, program=81):
    import pretty_midi

    pm = pretty_midi.PrettyMIDI(initial_tempo=120)
    inst = pretty_midi.Instrument(program=program, name="melody")
    for s, e, p in notes:
        inst.notes.append(pretty_midi.Note(velocity=100, pitch=int(p), start=float(s), end=float(e)))
    pm.instruments.append(inst)
    pm.write(str(path))


def work(job):
    path, out = job
    sid = song_id(path)
    if (out / f"{sid}.mid").exists():
        return sid, path, "cached"
    try:
        f0, conf = melodia(path)
        np.savez_compressed(out / f"{sid}.npz", f0=f0, conf=conf, hop=HOP, sr=SR)
        notes = f0_to_notes(f0, HOP / SR)
        write_midi(notes, out / f"{sid}.mid")
        return sid, path, f"{len(notes)} notes, voiced {np.mean(f0 > 0):.0%}"
    except Exception as e:  # keep going on broken files
        return sid, path, f"error: {e}"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("songs", type=Path, help="text file, one absolute path per line")
    ap.add_argument("out", type=Path)
    ap.add_argument("--root", type=Path, default=None, help="prefix the listed paths with this")
    ap.add_argument("--jobs", type=int, default=16)
    args = ap.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    listed = [line.strip() for line in args.songs.read_text().splitlines() if line.strip()]
    paths = [str(args.root / p.lstrip("/")) if args.root else p for p in listed]
    index = {song_id(p): {"path": p, "listed": l} for p, l in zip(paths, listed)}
    (args.out / "index.json").write_text(json.dumps(index, indent=1))
    os.nice(10)
    with Pool(args.jobs) as pool:
        for i, (sid, path, msg) in enumerate(pool.imap_unordered(work, [(p, args.out) for p in paths])):
            print(f"[{i + 1}/{len(paths)}] {sid} {Path(path).name}: {msg}", flush=True)


if __name__ == "__main__":
    main()
