"""Club-music effects at 40 ms: risers, downlifters, impacts, air horns, sirens and reverse cymbals.

A small head (conv + BiGRU) on frozen BEATs embeddings, trained on synthetic mixtures (analyze/fx_train/):
effects pasted into real song bars with exact labels. It listens to the full mix at 16 kHz in
overlapping 10 s pieces, as `sed.py` does, plus a log-mel up to 15.5 kHz at 25 fps if the chosen head
reads it. Kept: a probability track per class at 25 fps, events where a class stays above its
threshold ([start, end, kind, peak]) and a summary. Riser and impact thresholds are set from how often
they fire away from drops on songs kept apart for that; the rest come from synthetic validation. See
fx_train/README.md for what it was checked against, which isn't ground truth.
"""

import numpy as np

from analyze import common
from analyze.fx_train import backbone as bb
from analyze.fx_train import head as H
from analyze.fx_train.effects import CLASSES

NAME, VERSION = "fxhead", 1
GPU = True
MODEL = common.ROOT / "models/fx_head/best.pt"
OUT_KIND = {"horn": "airhorn"}  # the name the hand labels use


def setup():
    beats = bb.Beats()
    head, ck = H.load(MODEL)
    return beats, head, ck


def analyse(song, state):
    beats, head, ck = state
    x = bb.to_32k(common.stems(song)["mix"])
    emb, mel = bb.features(beats, x)
    P = H.predict(head, ck, emb, mel)
    tracks = {OUT_KIND.get(c, c): common.track(P[:, j]) for j, c in enumerate(CLASSES)}
    events = [[a, b, OUT_KIND.get(k, k), pk] for a, b, k, pk in H.events(P, ck["thresholds"])]
    minutes = len(P) / bb.FPS / 60
    kinds = [OUT_KIND.get(c, c) for c in CLASSES]
    counts = {k: sum(e[2] == k for e in events) for k in kinds}
    summary = {"events": counts, "per_minute": {k: round(v / max(minutes, 1e-6), 2) for k, v in counts.items()},
               "active": {OUT_KIND.get(c, c): round(float((P[:, j] >= ck["thresholds"][j]).mean()), 3) for j, c in enumerate(CLASSES)},
               "model": {"sources": ck["sources"], "features": ck["features"], "thresholds": [round(t, 2) for t in ck["thresholds"]]}}
    return {"fps": bb.FPS, "tracks": tracks, "events": events, "summary": summary}
