"""Which songs go where, and which bars of a song are probably free of effects.

Backgrounds are real songs, which hold real, unlabelled effects: label noise that would teach the head
to ignore exactly what it should hear. So backgrounds only use bars that look effect-free:
  away from drops (16 bars before to 2 after), not in a Build section, not the first or last bars
  of any section (where fills and transitions sit), and at least a bar away from anything the DSP
  `risers` analyzer or the sound-event analyzers (`sed`, `events`) marks as an effect.
That removes most of the noise, not all of it: a quiet riser below every threshold can stay.

Songs are split by artist, with duplicates of a track together, so no audio and little style leaks.
"""

import hashlib
import json
import re
from functools import lru_cache
from pathlib import Path

import numpy as np

from analyze import common

HERE = Path(__file__).parent
META = HERE / "meta.json"  # beats, drops, sections, genre from the viewer's /api/analysis
MUST_TEST = {"58e556af6855", "fe0b3c607b1b"}  # Acraze, Hardwell Spaceman
FX_EVENTS = re.compile(r"horn|siren|whoosh|boom|explosion|zing|white noise|thump|rumble|gunshot", re.I)


@lru_cache(maxsize=1)
def meta():
    return json.loads(META.read_text())


def _artist(name):
    n = re.sub(r"^\s*\d+[.\s-]*", "", name).lower()
    return re.split(r"\s[-–—]\s|／|/|\sfeat|\sx\s|,|&", n)[0].strip() or n


def _hash(s, salt="fx"):
    return int(hashlib.sha1((salt + s).encode()).hexdigest()[:8], 16)


def splits():
    """{"train": [...], "val": [...], "dev": [...], "test": [...]}: dev tunes on real songs, test only reports."""
    m = meta()
    groups = {}
    for sid, v in m.items():
        groups.setdefault(_artist(v["name"]), []).append(sid)
    fixed = {_artist(m[s]["name"]) for s in MUST_TEST}
    out = {"train": [], "val": [], "dev": [], "test": []}
    for art in sorted(groups, key=_hash):
        ids = groups[art]
        el = sum(m[s]["genre"] == "electronic" for s in ids)
        pop = len(ids) - el
        if art in fixed:
            out["test"] += ids
            continue
        c = {k: sum(m[s]["genre"] == "electronic" for s in out[k]) for k in out}
        if el:  # artists with many songs train, or one would swallow a small hold-out set
            if len(ids) > 4:
                k = "train"
            elif c["test"] < 60:
                k = "test"
            elif c["dev"] < 20:
                k = "dev"
            elif c["val"] < 15:
                k = "val"
            else:
                k = "train"
        else:  # pop only widens what the head must stay quiet on
            p = {k: sum(m[s]["genre"] != "electronic" for s in out[k]) for k in out}
            k = "train" if p["train"] < 40 else ("val" if p["val"] < 5 else None)
        if k:
            out[k] += ids
    return out


def downbeats(v):
    return np.array([t for t, k in zip(v["beats"], v["beat_in_bar"]) if k == 1], float)


def _features(name, sid):
    p = common.FEATURES / name / f"{sid}.json"
    return json.loads(p.read_text()) if p.exists() else {}


def clean_bars(sid):
    """(bar starts, bar ends, clean mask) of a song."""
    v = meta()[sid]
    db = downbeats(v)
    if len(db) < 8:
        return db[:-1], db[1:], np.zeros(max(len(db) - 1, 0), bool)
    a, b = db[:-1], db[1:]
    ok = np.ones(len(a), bool)
    bar = float(np.median(np.diff(db)))

    def cut(t0, t1):
        ok[(b > t0) & (a < t1)] = False

    for d in v["drops"]:
        cut(d["time"] - 16 * bar, d["time"] + 2 * bar)
    for s in v["sections"]:
        if s["kind"] == "Build":
            cut(s["start"], s["end"])
        cut(s["start"] - 0.01, s["start"] + bar * 0.99)  # its first bar
        cut(s["end"] - 2 * bar, s["end"] + 0.01)  # and its last two
    r = _features("risers", sid)
    for e in r.get("risers", []):
        cut(e[0] - bar, e[1] + bar)
    for e in r.get("impacts", []):
        cut(e[0] - bar, e[0] + bar)
    for e in r.get("gaps", []):
        cut(e[0] - bar, e[1] + bar)
    for name in ("sed", "events"):
        for e in _features(name, sid).get("events", []):
            if isinstance(e, list) and len(e) >= 3 and isinstance(e[2], str) and FX_EVENTS.search(e[2]):
                cut(e[0] - bar, e[1] + bar)
    return a, b, ok


def clean_runs(sid, min_bars=2):
    """[(first bar, one past last bar)] of consecutive clean bars, with the bar times."""
    a, b, ok = clean_bars(sid)
    return a, b, common.runs(ok, min_bars)


def stats():
    s = splits()
    m = meta()
    for k, ids in s.items():
        n = [clean_bars(i) for i in ids]
        sh = [x[2].mean() for x in n if len(x[2])]
        print(k, len(ids), "electronic", sum(m[i]["genre"] == "electronic" for i in ids), "drops",
              sum(len(m[i]["drops"]) for i in ids), "clean share median", round(float(np.median(sh)), 2),
              "songs with under 16 clean bars", sum(x[2].sum() < 16 for x in n))


if __name__ == "__main__":
    stats()
