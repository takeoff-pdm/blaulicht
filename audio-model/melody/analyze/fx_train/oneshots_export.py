"""Rekordbox's bundled sampler sounds, as one small file for ae-charlie. Run on the Mac:

    python3 audio-model/melody/analyze/fx_train/oneshots_export.py

The titles come from Rekordbox's own library (master.db); `~/Music/rekordbox/Sampler` holds the files.
They stay out of every training set: they are the one test of real, professionally made effects
that none of the heads has heard.
"""

import json
import subprocess
from pathlib import Path

import numpy as np

SRC = Path.home() / "Music/rekordbox/Sampler"
OUT = Path(__file__).parent / "rekordbox_oneshots.npz"
MF = "MERGE FX/MergeFX Sample Sound {}.wav"
# file, title in Rekordbox, our class ("none": a distractor, must not fire)
TABLE = [(MF.format(203), "BOOM DOWN LIFTER1", "impact"), (MF.format(204), "BOOM DOWN LIFTER2", "impact"),
         (MF.format(205), "NOISE DOWN LIFTER1", "downlifter"), (MF.format(206), "NOISE DOWN LIFTER2", "downlifter"),
         (MF.format(207), "IMPACT KICK", "impact"), (MF.format(208), "IMPACT SNARE", "impact"),
         (MF.format(209), "SHORT BOOM", "impact"), (MF.format(304), "SHORT BOOM", "impact"),
         (MF.format(201), "CLASH CYMBAL", "impact"), (MF.format(104), "CYMBAL", "impact"),
         (MF.format(302), "HORN", "horn"), ("OSC_SAMPLER/PRESET ONESHOT/HORN.wav", "HORN", "horn"),
         ("OSC_SAMPLER/PRESET ONESHOT/SIREN.wav", "SIREN", "siren"),
         (MF.format(101), "SNARE9", "none"), (MF.format(102), "SNARE8", "none"), (MF.format(103), "CLAP", "none"),
         (MF.format(202), "SNARE", "none"), (MF.format(301), "NEEDLE SCRATCH NOISE", "none"),
         (MF.format(303), "REWIND", "none"), (MF.format(305), "LET'S GO", "none"),
         ("OSC_SAMPLER/PRESET ONESHOT/NOISE.wav", "NOISE", "none"), ("OSC_SAMPLER/PRESET ONESHOT/SINEWAVE.wav", "SINEWAVE", "none")]


def main():
    arrays, rows = {}, []
    for i, (f, title, cls) in enumerate(TABLE):
        pcm = subprocess.run(["ffmpeg", "-v", "error", "-i", str(SRC / f), "-ac", "1", "-ar", "32000", "-f", "f32le", "-"],
                             check=True, capture_output=True).stdout
        x = np.frombuffer(pcm, np.float32)
        arrays[f"a{i}"] = (x * 32767 / max(float(np.abs(x).max()), 1e-6)).astype(np.int16)
        rows.append({"i": i, "file": f, "title": title, "class": cls, "dur": round(len(x) / 32000, 2)})
    np.savez_compressed(OUT, rows=json.dumps(rows), **arrays)
    print(len(rows), "one-shots,", round(OUT.stat().st_size / 1e6, 1), "MB")
    for r in rows:
        x = arrays[f"a{r['i']}"].astype(float)
        n = len(x)
        P = np.abs(np.fft.rfft(x[: n // 4 * 4].reshape(4, -1), axis=1)) ** 2 + 1e-6
        f = np.fft.rfftfreq(n // 4, 1 / 32000)
        cen = (P * f).sum(1) / P.sum(1)
        print(f"{r['title']:22s} {r['class']:10s} {r['dur']:5.2f} s  level by quarter dB {np.round(10 * np.log10(P.sum(1) / P.sum(1).max()), 0)} centroid {np.round(cen, 0)}")


if __name__ == "__main__":
    main()
