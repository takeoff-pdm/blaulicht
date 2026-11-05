"""Polyphonic notes of the synths, pads, plucks and chord stabs ("other" stem).

Spotify's Basic Pitch (Bittner et al., ICASSP 2022; code and weights Apache-2.0), instrument
agnostic, on the "other" stem, CPU (TensorFlow). Kept: the notes [[start, end, pitch,
amplitude]] (at most 6000, the loudest), chord stabs (3 or more notes starting within 30 ms:
[[t, notes, amplitude]]), and per 100 ms the polyphony (notes sounding) and note onsets per
second. Lights can hit on stabs, spread by polyphony (a wide pad chord lights more), and use the
top line where the melody line has nothing.
"""

import os
import tempfile

import numpy as np
import soundfile as sf

from analyze import common

NAME, VERSION = "transcribe", 1
FPS = 10


def setup():
    os.environ.setdefault("TF_CPP_MIN_LOG_LEVEL", "3")
    from basic_pitch import ICASSP_2022_MODEL_PATH
    from basic_pitch.inference import Model, predict

    return predict, Model(ICASSP_2022_MODEL_PATH)


def analyse(song, state):
    predict, model = state
    x = common.stems(song)["other"]
    with tempfile.NamedTemporaryFile(suffix=".wav", dir=str(common.ROOT / "tmp")) as f:
        sf.write(f.name, x, common.SR, subtype="FLOAT")
        _, _, events = predict(f.name, model, onset_threshold=0.5, frame_threshold=0.3, minimum_note_length=58)
    notes = sorted([[round(float(s), 3), round(float(e), 3), int(p), round(float(a), 3)] for s, e, p, a, *_ in events])
    if len(notes) > 6000:
        keep = set(map(id, sorted(notes, key=lambda n: -n[3])[:6000]))
        notes = [n for n in notes if id(n) in keep]
    dur = len(x) / common.SR
    n = int(dur * FPS) + 1
    poly, onset = np.zeros(n), np.zeros(n)
    for s, e, _, _ in notes:
        poly[int(s * FPS): int(e * FPS) + 1] += 1
        onset[min(n - 1, int(s * FPS))] += 1
    stabs, i = [], 0
    while i < len(notes):
        j = i
        while j + 1 < len(notes) and notes[j + 1][0] - notes[i][0] <= 0.03:
            j += 1
        if j - i + 1 >= 3:
            stabs.append([notes[i][0], j - i + 1, round(float(np.mean([m[3] for m in notes[i: j + 1]])), 3)])
        i = j + 1
    rate = common.smooth(onset, FPS) * FPS
    return {"fps": FPS, "notes": notes, "stabs": stabs, "polyphony": common.track(np.minimum(poly, 12), 0, 12),
            "onset_rate": common.track(np.minimum(rate, 16), 0, 16),
            "summary": {"notes": len(notes), "stabs": len(stabs), "max_polyphony": int(poly.max()) if n else 0}}
