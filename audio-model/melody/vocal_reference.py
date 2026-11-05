"""Reference vocal melody for the MUSDB18 test mixes: SwiftF0 on the isolated vocal stem.

MUSDB18 ships the vocal stem of every mix, and a monophonic tracker is reliable on it, so this
gives a vocal-melody reference for the mixes (it marks instrumental leads as unvoiced, so
compare systems by RPA/RCA on its voiced frames rather than by OA). Written on the student
grid (frame g centred on sample g * 441 at 44.1 kHz), ids match melodia_labels.py.

    python vocal_reference.py musdb_test_mix.txt data/labels/musdb_test_vocals
"""

import hashlib
import sys
from pathlib import Path

import numpy as np
import soundfile as sf
from swift_f0 import SwiftF0

SR, HOP = 44100, 441


def main():
    lst, out = Path(sys.argv[1]), Path(sys.argv[2])
    out.mkdir(parents=True, exist_ok=True)
    det = SwiftF0()
    for line in lst.read_text().splitlines():
        mix = line.strip()
        if not mix:
            continue
        sid = hashlib.sha1(mix.encode()).hexdigest()[:12]
        voc, sr = sf.read(str(Path(mix).with_name("vocals.wav")), dtype="float32")
        voc = voc.mean(1) if voc.ndim > 1 else voc
        r = det.detect(voc, sr)
        n = int(len(voc) * SR / sr) // HOP + 1
        t = np.arange(n) * HOP / SR
        idx = np.clip(np.searchsorted(r.timestamps, t), 0, len(r.timestamps) - 1)
        prev = np.clip(idx - 1, 0, None)
        idx = np.where(np.abs(r.timestamps[prev] - t) < np.abs(r.timestamps[idx] - t), prev, idx)
        f0 = np.where(r.confidence[idx] >= 0.5, r.pitch_hz[idx], 0).astype(np.float32)
        np.save(out / f"{sid}.npy", f0)
        print(sid, Path(mix).parent.name, f"voiced {np.mean(f0 > 0):.0%}", flush=True)


if __name__ == "__main__":
    main()
