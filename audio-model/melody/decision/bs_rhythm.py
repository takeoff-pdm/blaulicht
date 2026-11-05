"""Does the show's rhythm after a drop look like what human lighters do in the same bars?

    python audio-model/melody/decision/bs_rhythm.py    # after harness.cjs rhythm <variant> and bs_fetch.py; writes bs/rhythm.json

Per drop, bars 2-4 (bar 1 is the strobe), the profile over the 16 sixteenths of a bar, compared by Pearson correlation: the show's flashes
per plan variant, the kick and snare pulses alone, another human on the same drop, and a human from another song as the floor.
A rhythm profile is one narrow part of a light show, so this says nothing about looks or colour.
"""

import json
import random
import sys
from collections import defaultdict
from pathlib import Path

import numpy as np

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
sys.dont_write_bytecode = True
from bs_compare import LIGHTS, MIN_GRID, MIN_PEAK, MIN_SHARP, grid_fit, load_maps, sixteenths  # noqa: E402

rng = random.Random(11)


def human(m, bars, bar):
    v = np.zeros(16)
    for b in range(bar + 1, bar + 4):
        a0, a1 = bars[b]["t0"], bars[b]["t1"]
        for e in m["events"]:
            if a0 - 0.03 <= e[0] < a1 - 0.03 and e[1] in LIGHTS and e[2] != 0:
                v[int(round((e[0] - a0) / (a1 - a0) * 16)) % 16] += 1
    return v


def pear(a, b):
    a, b = a - a.mean(), b - b.mean()
    na, nb = np.linalg.norm(a), np.linalg.norm(b)
    return float(a @ b / (na * nb)) if na > 0 and nb > 0 else np.nan


def med(x):
    x = [v for v in x if v == v]
    return {"median": round(float(np.median(x)), 3), "q25": round(float(np.percentile(x, 25)), 3), "q75": round(float(np.percentile(x, 75)), 3), "n": len(x)} if x else None


def main():
    maps = load_maps()
    table = defaultdict(list)
    profiles = []
    for sid, ms in maps.items():
        bf = HERE / f"bars/{sid}.rules.json"
        rf = HERE / f"bars/{sid}.rules.rhythm.json"
        if not bf.exists() or not rf.exists():
            continue
        bars = json.loads(bf.read_text())
        grid = sixteenths(bars)
        kept = []
        for m in ms:
            shift, share, _ = grid_fit(m["events"], grid)
            if m["peak"] >= MIN_PEAK and m["sharp"] >= MIN_SHARP and share >= MIN_GRID:
                m["events"] = [[e[0] + shift, e[1], e[2]] for e in m["events"]]
                kept.append(m)
        if not kept:
            continue
        variants = {}
        for v in ("rules", "random", "beam", "llm"):
            f = HERE / f"bars/{sid}.{v}.rhythm.json"
            if f.exists():
                variants[v] = {d["bar"]: d for d in json.loads(f.read_text())}
        for bar, d in variants["rules"].items():
            if bar + 4 >= len(bars):
                continue
            hs = []
            for m in kept:
                lo, hi = m["lag"], m["lag"] + m["map_audio_s"]
                if bars[bar]["t0"] >= lo and bars[bar + 3]["t1"] <= hi:
                    h = human(m, bars, bar)
                    if h.sum() >= 8:
                        hs.append(h)
            for h in hs:
                profiles.append((sid, h, np.array(d["show"], float)))
                table["pulses alone (kick + snare)"].append(pear(np.array(d["kick"], float), h))
                for v, dd in variants.items():
                    if bar in dd:
                        table[f"show: {v}"].append(pear(np.array(dd[bar]["show"], float), h))
                        if v == "llm":
                            table["show: rules, on LLM songs"].append(pear(np.array(d["show"], float), h))
            for a in range(len(hs)):
                for b in range(a + 1, len(hs)):
                    table["another human, same drop"].append(pear(hs[a], hs[b]))
    for _ in range(4000):
        a, b = rng.sample(profiles, 2)
        if a[0] != b[0]:
            table["a human from another song"].append(pear(a[1], b[1]))
            table["rules show vs a human from another song"].append(pear(a[2], b[1]))
    res = {k: med(v) for k, v in table.items()}
    (HERE / "bs/rhythm.json").write_text(json.dumps(res, indent=1))
    for k, v in res.items():
        print(k.ljust(44), v)


if __name__ == "__main__":
    main()
