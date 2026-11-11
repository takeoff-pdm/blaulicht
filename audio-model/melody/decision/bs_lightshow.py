"""The aligned Beat Saber maps as per-song lightshow files, for the viewer's Human Lighter look.

    python3 audio-model/melody/decision/bs_lightshow.py    # standard library only; then pipeline/song.py compile --stale

Reads bs/maps/ (bs_fetch.py, events in seconds of our audio) and keeps the same maps as bs_compare.py: 200+ events,
a clear alignment peak, and most lighting on our 16ths after a constant shift. Writes
data/melody/viz/lightshows/<song id>.json and index.json there. Maps are other people's work, so they stay out of the
repo: data/ is gitignored, like bs/.

Alignment uses one constant offset per map, so a map made for a shorter edit of the song can sit on the beat and still
lose the song's structure after the edit's first cut. `fit` is there to show that: how well the map's per-bar activity
follows our loudness, in the first and second half of what it covers, and `on_drops` is the share of our drops
with a light event on the downbeat.
"""

import bisect
import json
import math
from collections import defaultdict
from pathlib import Path

HERE = Path(__file__).resolve().parent
OUT = HERE.parent.parent / "data/melody/viz/lightshows"
LIGHTS = (0, 1, 2, 3, 4, 6, 7, 10, 11)
KEEP = (0, 1, 2, 3, 4, 5, 8, 9, 12, 13)  # the groups the look uses, boost, ring spin and zoom, laser speeds
MIN_EVENTS, MIN_PEAK, MIN_SHARP, MIN_GRID = 200, 0.5, 12.0, 0.6


def sixteenths(bars):
    out = []
    for b in bars:
        step = (b["t1"] - b["t0"]) / 16
        out += [b["t0"] + k * step for k in range(16)]
    return out


def grid_fit(events, grid):
    """(shift, share of light events within 25 ms of our 16ths after it), as in bs_compare.py."""
    times = [e[0] for e in events if e[1] in LIGHTS and e[2] != 0 and grid[0] <= e[0] <= grid[-1]]
    if len(times) < 50:
        return 0.0, 0.0
    step = sorted(b - a for a, b in zip(grid, grid[1:]))[len(grid) // 2]
    best = (0.0, -1.0)
    for k in range(int(step / 0.005) + 1):
        shift = -step / 2 + k * 0.005
        on = 0
        for t in times:
            x = t + shift
            i = min(max(bisect.bisect_left(grid, x), 1), len(grid) - 1)
            on += min(abs(x - grid[i - 1]), abs(grid[i] - x)) <= 0.025
        if on / len(times) > best[1]:
            best = (shift, on / len(times))
    return best


def spearman(x, y):
    if len(x) < 8:
        return None

    def ranks(v):
        order = sorted(range(len(v)), key=v.__getitem__)
        r = [0.0] * len(v)
        i = 0
        while i < len(v):
            j = i
            while j + 1 < len(v) and v[order[j + 1]] == v[order[i]]:
                j += 1
            for k in range(i, j + 1):
                r[order[k]] = (i + j) / 2
            i = j + 1
        return r

    a, b = ranks(x), ranks(y)
    ma, mb = sum(a) / len(a), sum(b) / len(b)
    num = sum((p - ma) * (q - mb) for p, q in zip(a, b))
    den = math.sqrt(sum((p - ma) ** 2 for p in a) * sum((q - mb) ** 2 for q in b))
    return round(num / den, 3) if den else None


def fit(events, bars, cover):
    """Per-bar light activity against our loudness, in the first and second half of the covered bars; share of drops hit."""
    edges = [b["t0"] for b in bars] + [bars[-1]["t1"]]
    lit = [0] * len(bars)
    times = sorted(e[0] for e in events if e[1] in LIGHTS and e[2] != 0)
    for t in times:
        i = bisect.bisect_right(edges, t) - 1
        if 0 <= i < len(bars):
            lit[i] += 1
    idx = [i for i, b in enumerate(bars) if b["t0"] >= cover[0] and b["t1"] <= cover[1]]
    half = len(idx) // 2

    def part(ids):
        return spearman([bars[i]["mix"] for i in ids], [math.log1p(lit[i]) for i in ids])

    drops = [b["t0"] for b in bars if b["drop"] and cover[0] <= b["t0"] <= cover[1]]
    hit = sum(any(abs(t - d) <= 0.03 for t in times[max(0, bisect.bisect_left(times, d - 0.03)):bisect.bisect_left(times, d + 0.031)]) for d in drops)
    return [part(idx[:half]), part(idx[half:])], round(hit / len(drops), 3) if drops else None


def main():
    by_song = defaultdict(list)
    for p in sorted((HERE / "bs/maps").glob("*.json")):
        m = json.loads(p.read_text())
        if len(m["events"]) < MIN_EVENTS or m["peak"] < MIN_PEAK or m["sharp"] < MIN_SHARP:
            continue
        bars_file = HERE / f"bars/{m['song']}.rules.json"
        if not bars_file.exists():
            continue
        bars = json.loads(bars_file.read_text())
        shift, share = grid_fit(m["events"], sixteenths(bars))
        if share < MIN_GRID:
            continue
        # beyond 8 a light value is a legacy custom colour, not a state, and the look keeps to the song's own pair
        events = [[round(t + shift, 3), ty, v] for t, ty, v in m["events"] if ty in KEEP and t + shift >= 0 and not (ty in LIGHTS and v > 8)]
        duration = bars[-1]["t1"]
        cover = [round(max(0.0, m["lag"] + shift), 3), round(min(m["our_audio_s"], m["lag"] + shift + m["map_audio_s"]), 3)]
        halves, on_drops = fit(events, bars, cover)
        by_song[m["song"]].append({"map": m["map"], "name": m["name"], "curated": bool(m["curated"]), "ranked": bool(m["ranked"]), "score": m["score"],
                                   "grid_share": round(share, 3), "shift": round(shift, 3), "cover": cover, "cover_share": round((cover[1] - cover[0]) / max(duration, 1e-3), 3),
                                   "fit": halves, "on_drops": on_drops, "events": events})
    OUT.mkdir(parents=True, exist_ok=True)
    for old in OUT.glob("*.json"):
        old.unlink()
    index = []
    songs = {s["id"]: s for s in json.loads((HERE.parent.parent / "data/melody/viz/songs.json").read_text())}
    for sid, maps in sorted(by_song.items(), key=lambda kv: songs[kv[0]]["name"].lower()):
        maps.sort(key=lambda m: (-m["curated"], -m["ranked"], -(m["score"] or 0), m["map"]))  # curated first: someone reviewed it
        (OUT / f"{sid}.json").write_text(json.dumps({"song": sid, "maps": maps}, separators=(",", ":")))
        index.append({"id": sid, "name": songs[sid]["name"], "maps": len(maps)})
    (OUT / "index.json").write_text(json.dumps(index, separators=(",", ":")))
    print(len(index), "songs,", sum(len(v) for v in by_song.values()), "maps")
    print(f"{'song':34} {'map':6} cur   ev  cover%  grid  fit 1st/2nd  on drops")
    for row in index:
        for m in by_song[row["id"]]:
            print(f"{row['name'][:34]:34} {m['map']:6} {'y' if m['curated'] else ' ':3} {len(m['events']):5} {100 * m['cover_share']:6.0f}  {m['grid_share']:.2f}  {m['fit'][0]} / {m['fit'][1]}  {m['on_drops']}")


if __name__ == "__main__":
    main()
