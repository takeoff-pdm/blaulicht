"""Human lighting as a reference: how do Beat Saber lighters treat our songs, and does anything learned from them
beat the rules at predicting it?

    python audio-model/melody/decision/bs_compare.py    # needs numpy, scipy, scikit-learn; writes bs/compare.json

What this can show: whether the rules' per-bar activity and look changes follow what people do by hand on the same
songs, how well two people agree (the ceiling for any imitation), and whether a model trained on that beats the
rules. What it can't show: that any of it looks good in a club. Beat Saber lighting is made by hobbyists for a VR
game with other fixtures, and activity is a coarse summary of it.
"""

import json
import random
from collections import defaultdict
from pathlib import Path

import numpy as np
from scipy.stats import spearmanr
from sklearn.ensemble import HistGradientBoostingRegressor
from sklearn.model_selection import GroupKFold

HERE = Path(__file__).resolve().parent
LIGHTS = (0, 1, 2, 3, 4, 6, 7, 10, 11)
MOTION = (8, 9, 12, 13)
MIN_PEAK, MIN_SHARP, MIN_GRID = 0.5, 12.0, 0.6
KINDS = ("Intro", "Groove", "Breakdown", "Build", "Drop", "Outro")
rng = random.Random(7)


def load_maps():
    maps = defaultdict(list)
    for p in sorted((HERE / "bs/maps").glob("*.json")):
        m = json.loads(p.read_text())
        if len(m["events"]) >= 200:
            maps[m["song"]].append(m)
    return maps


def sixteenths(bars):
    out = []
    for b in bars:
        step = (b["t1"] - b["t0"]) / 16
        out += [b["t0"] + k * step for k in range(16)]
    return np.array(out)


def grid_fit(events, grid):
    """Mappers' grids sit a constant few ms from ours (the mp3 and ogg decoders, the map's own offset, the 10 ms of the
    envelope alignment), so find that shift first: (shift, share of light events on our 16ths within +-25 ms after it, chance)."""
    t = np.array([e[0] for e in events if e[1] in LIGHTS and e[2] != 0 and grid[0] <= e[0] <= grid[-1]])
    if len(t) < 50:
        return 0.0, 0.0, 1.0
    step = float(np.median(np.diff(grid)))
    best = (0.0, -1.0)
    for shift in np.arange(-step / 2, step / 2, 0.005):
        x = t + shift
        i = np.clip(np.searchsorted(grid, x), 1, len(grid) - 1)
        d = np.minimum(np.abs(x - grid[i - 1]), np.abs(grid[i] - x))
        share = float((d <= 0.025).mean())
        if share > best[1]:
            best = (float(shift), share)
    return best[0], best[1], min(1.0, 0.05 / step)


def per_bar(m, bars):
    """Per bar: lit events, flashes and fades, events per light group, on-beat share, motion events."""
    n = len(bars)
    edges = np.array([b["t0"] for b in bars] + [bars[-1]["t1"]])
    lit, flash, off, motion = np.zeros(n), np.zeros(n), np.zeros(n), np.zeros(n)
    groups = np.zeros((n, len(LIGHTS)))
    red = np.zeros(n)
    covered = np.zeros(n, bool)
    lo, hi = m["lag"], m["lag"] + m["map_audio_s"]
    for i in range(n):
        covered[i] = bars[i]["t0"] >= lo and bars[i]["t1"] <= hi
    for t, ty, v in m["events"]:
        i = int(np.searchsorted(edges, t, side="right")) - 1
        if not 0 <= i < n:
            continue
        if ty in LIGHTS:
            if v == 0:
                off[i] += 1
                continue
            lit[i] += 1
            groups[i, LIGHTS.index(ty)] += 1
            flash[i] += v in (2, 3, 6, 7, 10, 11)
            red[i] += v in (5, 6, 7, 8)
        elif ty in MOTION:
            motion[i] += 1
    return {"lit": lit, "flash": flash, "off": off, "groups": groups, "motion": motion, "red": red, "covered": covered}


def spear(x, y):
    if len(x) < 8 or np.std(x) == 0 or np.std(y) == 0:
        return np.nan
    return float(spearmanr(x, y).statistic)


def changes(groups, covered, thresh=0.35):
    """Bars where the human's use of the light groups changes: cosine distance between the 2 bars after and the 2 before, peak-picked."""
    n = len(groups)
    d = np.zeros(n)
    for i in range(2, n - 1):
        a, b = groups[i - 2:i].sum(0), groups[i:i + 2].sum(0)
        if covered[i] and a.sum() + b.sum() > 0:
            na, nb = np.linalg.norm(a), np.linalg.norm(b)
            d[i] = 1.0 if na == 0 or nb == 0 else 1 - float(a @ b) / (na * nb)
    return [i for i in range(n) if d[i] > thresh and d[i] >= d[max(0, i - 2):i + 3].max()]


def f1(pred, truth, tol=1):
    if not pred or not truth:
        return np.nan
    tp_p = sum(any(abs(p - t) <= tol for t in truth) for p in pred)
    tp_t = sum(any(abs(p - t) <= tol for p in pred) for t in truth)
    prec, rec = tp_p / len(pred), tp_t / len(truth)
    return 2 * prec * rec / (prec + rec) if prec + rec else 0.0


def features(bars):
    X = []
    for b in bars:
        X.append([b["mix"], b["drums"], b["bass"], b["other"], b["vocals"], b["hits"], b["hats"], b["stabs"], b["impacts"], b["notes"], b["energetic"], b["build"],
                  float(b["seg_start"]), float(b["drop"]), min(b["since_drop"], 32), min(b["to_drop"], 32), b["part"]] + [float(b["kind"] == k) for k in KINDS])
    return np.array(X, float)


def rank01(x):
    return (np.argsort(np.argsort(x)) + 0.5) / len(x)


def main():
    allmaps = load_maps()
    bars_of = {sid: json.loads((HERE / f"bars/{sid}.rules.json").read_text()) for sid in allmaps if (HERE / f"bars/{sid}.rules.json").exists()}
    kept = defaultdict(list)
    stats = {"maps_fetched": sum(len(v) for v in allmaps.values()), "songs_fetched": len(allmaps)}
    grid_rows = []
    for sid, ms in allmaps.items():
        if sid not in bars_of:
            continue
        grid = sixteenths(bars_of[sid])
        for m in ms:
            shift, share, chance = grid_fit(m["events"], grid)
            grid_rows.append((sid, m["map"], m["peak"], m["sharp"], round(shift, 3), round(share, 3), round(chance, 3)))
            if m["peak"] >= MIN_PEAK and m["sharp"] >= MIN_SHARP and share >= MIN_GRID:
                m["grid_share"], m["grid_chance"], m["shift"] = share, chance, shift
                m["events"] = [[e[0] + shift, e[1], e[2]] for e in m["events"]]
                kept[sid].append(m)
    stats.update(maps_kept=sum(len(v) for v in kept.values()), songs_kept=len(kept), songs_with_pairs=sum(len(v) >= 2 for v in kept.values()),
                 grid_share_median=float(np.median([m["grid_share"] for v in kept.values() for m in v])), grid_chance_median=float(np.median([m["grid_chance"] for v in kept.values() for m in v])))
    print(json.dumps(stats, indent=1))

    rows, per_map = [], []
    table = defaultdict(list)
    tab_change = defaultdict(list)
    pair_rows = []
    for sid, ms in kept.items():
        bars = bars_of[sid]
        arr = {k: np.array([b[k] for b in bars], float) for k in ("mix", "drums", "bright", "level", "steps", "bass", "hits")}
        seg_bars = [i for i, b in enumerate(bars) if b["seg_start"]]
        H = [per_bar(m, bars) for m in ms]
        for m, h in zip(ms, H):
            c = h["covered"]
            if c.sum() < 16:
                continue
            y = np.log1p(h["lit"][c])
            table["rules brightness"].append(spear(arr["bright"][c], y))
            table["rules level"].append(spear(arr["level"][c], y))
            table["rules step rate"].append(spear(arr["steps"][c], y))
            table["loudness (mix)"].append(spear(arr["mix"][c], y))
            table["drums + bass"].append(spear(arr["drums"][c] + arr["bass"][c], y))
            shift = rng.randint(16, max(17, int(c.sum()) - 16))
            table["rules brightness, shifted"].append(spear(np.roll(arr["bright"][c], shift), y))
            ch = changes(h["groups"], c)
            tab_change["rules: segment starts"].append(f1([i for i in seg_bars if c[i]], ch))
            k = len([i for i in seg_bars if c[i]])
            idx = np.flatnonzero(c)
            tab_change["random bars, same count"].append(f1(sorted(rng.sample(list(idx), min(k, len(idx)))), ch))
            per_map.append({"song": sid, "map": m["map"], "h": h, "covered": c})
        for i in range(len(ms)):
            for j in range(i + 1, len(ms)):
                c = H[i]["covered"] & H[j]["covered"]
                if c.sum() < 16:
                    continue
                r = spear(np.log1p(H[i]["lit"][c]), np.log1p(H[j]["lit"][c]))
                ci, cj = changes(H[i]["groups"], c), changes(H[j]["groups"], c)
                pair_rows.append({"song": sid, "a": ms[i]["map"], "b": ms[j]["map"], "r": r, "f1": f1(ci, cj)})

    # the learned reference: gradient boosting on the cues, predicting a human's per-bar activity on songs it hasn't seen
    Xs, ys, groups, keys = [], [], [], []
    for pm in per_map:
        bars = bars_of[pm["song"]]
        c = pm["covered"]
        X = features(bars)[c]
        y = rank01(np.log1p(pm["h"]["lit"][c]) + np.random.default_rng(1).normal(0, 1e-6, c.sum()))
        Xs.append(X)
        ys.append(y)
        groups += [pm["song"]] * len(y)
        keys += [(pm["song"], pm["map"])] * len(y)
    X, y = np.vstack(Xs), np.concatenate(ys)
    groups = np.array(groups)
    pred = np.zeros_like(y)
    folds = min(5, len(set(groups)))
    for tr, te in GroupKFold(folds).split(X, y, groups):
        gb = HistGradientBoostingRegressor(max_depth=4, max_iter=200, learning_rate=0.05, min_samples_leaf=20, random_state=0).fit(X[tr], y[tr])
        pred[te] = gb.predict(X[te])
    keys_arr = np.array([f"{a}|{b}" for a, b in keys])
    table["gradient boosting (unseen songs)"] = [spear(pred[keys_arr == k], y[keys_arr == k]) for k in sorted(set(keys_arr))]

    def med(v):
        v = [x for x in v if x == x]
        return {"median": round(float(np.median(v)), 3), "q25": round(float(np.percentile(v, 25)), 3), "q75": round(float(np.percentile(v, 75)), 3), "n": len(v)} if v else None

    res = {"stats": stats, "activity_spearman": {k: med(v) for k, v in table.items()}, "human_vs_human_activity": med([p["r"] for p in pair_rows]),
           "look_change_f1": {k: med(v) for k, v in tab_change.items()}, "human_vs_human_look_change_f1": med([p["f1"] for p in pair_rows])}

    # per-map comparison of the rules and the model on the same maps
    keys_u = sorted(set(keys_arr))
    gb_r = {k: spear(pred[keys_arr == k], y[keys_arr == k]) for k in keys_u}
    rb_r = {}
    for pm in per_map:
        c = pm["covered"]
        rb_r[f"{pm['song']}|{pm['map']}"] = spear(np.array([b["bright"] for b in bars_of[pm["song"]]], float)[c], np.log1p(pm["h"]["lit"][c]))
    both = [(gb_r[k], rb_r[k]) for k in keys_u if gb_r[k] == gb_r[k] and rb_r.get(k, np.nan) == rb_r.get(k, np.nan)]
    res["gb_minus_rules"] = {"median_diff": round(float(np.median([a - b for a, b in both])), 3), "share_gb_better": round(float(np.mean([a > b for a, b in both])), 3), "n": len(both)}

    # what people do at drops
    drop_rows = []
    within, across, same_letter, diff_letter = [], [], [], []
    vec_by_song = defaultdict(list)
    for sid, ms in kept.items():
        bars = bars_of[sid]
        di = [i for i, b in enumerate(bars) if b["drop"]]
        for m in ms:
            h = per_bar(m, bars)
            vecs = []
            for i in di:
                if i < 4 or i + 4 >= len(bars) or not (h["covered"][i - 2] and h["covered"][i + 3]):
                    continue
                before, after = h["lit"][i - 2:i].sum(), h["lit"][i:i + 2].sum()
                t0 = bars[i]["t0"]
                beat = (bars[i]["t1"] - t0) / 4
                ev = [e for e in m["events"] if e[1] in LIGHTS]
                first_beat = sum(1 for e in ev if t0 - 0.03 <= e[0] < t0 + 0.03 and e[2] != 0)
                last_half = [e for e in ev if t0 - beat / 2 <= e[0] < t0 - 0.03 and e[2] != 0]
                prebar = [e for e in ev if t0 - 4 * beat <= e[0] < t0 - 0.03 and e[2] != 0]
                rate_before, rate_after = before / 2, after / 2
                drop_rows.append({"rise": float(np.log2((after + 1) / (before + 1))), "hit_on_downbeat": first_beat > 0, "quiet_last_half_beat": len(last_half) == 0,
                                  "before": rate_before, "after": rate_after, "flash_after": float(h["flash"][i:i + 2].sum() / max(1, after)), "groups_after": float((h["groups"][i:i + 2].sum(0) > 0).sum()), "prebar": len(prebar)})
                # the treatment: which light group fires on which 16th of the bar, and how (flash or on), over the drop's first 4 bars
                v = np.zeros((len(LIGHTS), 16, 2))
                for bi in range(i, i + 4):
                    a0, a1 = bars[bi]["t0"], bars[bi]["t1"]
                    for e in ev:
                        if a0 - 0.03 <= e[0] < a1 - 0.03 and e[2] != 0:
                            pos = int(round((e[0] - a0) / (a1 - a0) * 16)) % 16
                            v[LIGHTS.index(e[1]), pos, int(e[2] in (2, 3, 6, 7, 10, 11))] += 1
                v = v.ravel()
                vecs.append((v / (np.linalg.norm(v) + 1e-9), bars[i]["part"]))
            for a in range(len(vecs)):
                for b in range(a + 1, len(vecs)):
                    s = float(vecs[a][0] @ vecs[b][0])
                    within.append(s)
                    (same_letter if vecs[a][1] == vecs[b][1] else diff_letter).append(s)
            vec_by_song[m["map"]] = [x[0] for x in vecs]
    maps_ = list(vec_by_song)
    for _ in range(4000):
        a, b = rng.sample(maps_, 2)
        if vec_by_song[a] and vec_by_song[b]:
            across.append(float(rng.choice(vec_by_song[a]) @ rng.choice(vec_by_song[b])))
    res["drops"] = {"n": len(drop_rows), "median_log2_rise": round(float(np.median([d["rise"] for d in drop_rows])), 3), "share_rise": round(float(np.mean([d["rise"] > 0 for d in drop_rows])), 3),
                    "share_hit_on_downbeat": round(float(np.mean([d["hit_on_downbeat"] for d in drop_rows])), 3), "share_quiet_last_half_beat": round(float(np.mean([d["quiet_last_half_beat"] for d in drop_rows])), 3),
                    "median_events_per_bar_before": float(np.median([d["before"] for d in drop_rows])), "median_events_per_bar_after": float(np.median([d["after"] for d in drop_rows])), "median_flash_share_after": round(float(np.median([d["flash_after"] for d in drop_rows])), 3),
                    "treatment_similarity_within_song": med(within), "same_letter": med(same_letter), "different_letter": med(diff_letter), "across_songs": med(across)}
    # do two lighters do the same thing on the same drop? The rhythm of the first 4 bars (all groups together, 16 positions), since
    # light groups differ between environments. If this isn't above the other-song level, the choice is the lighter's, not the music's
    def rhythm(m, bars, i):
        v = np.zeros(16)
        for bi in range(i, i + 4):
            a0, a1 = bars[bi]["t0"], bars[bi]["t1"]
            for e in m["events"]:
                if a0 - 0.03 <= e[0] < a1 - 0.03 and e[1] in LIGHTS and e[2] != 0:
                    v[int(round((e[0] - a0) / (a1 - a0) * 16)) % 16] += 1
        return v / (np.linalg.norm(v) + 1e-9) if v.sum() else None

    same_drop, other_song, same_map_other_drop = [], [], []
    rh = {}
    for sid, ms in kept.items():
        bars = bars_of[sid]
        di = [i for i, b in enumerate(bars) if b["drop"] and 4 <= i and i + 4 < len(bars)]
        for m in ms:
            rh[(sid, m["map"])] = {i: rhythm(m, bars, i) for i in di if per_bar(m, bars)["covered"][i] and per_bar(m, bars)["covered"][i + 3]}
        for a in range(len(ms)):
            ra = rh[(sid, ms[a]["map"])]
            ka = [k for k in ra if ra[k] is not None]
            for x in range(len(ka)):
                for y in range(x + 1, len(ka)):
                    same_map_other_drop.append(float(ra[ka[x]] @ ra[ka[y]]))
            for b in range(a + 1, len(ms)):
                rb = rh[(sid, ms[b]["map"])]
                same_drop += [float(ra[i] @ rb[i]) for i in ra if i in rb and ra[i] is not None and rb[i] is not None]
    keys_rh = [k for k in rh if any(v is not None for v in rh[k].values())]
    for _ in range(4000):
        a, b = rng.sample(keys_rh, 2)
        if a[0] == b[0]:
            continue
        va, vb = [v for v in rh[a].values() if v is not None], [v for v in rh[b].values() if v is not None]
        other_song.append(float(rng.choice(va) @ rng.choice(vb)))
    res["drop_rhythm"] = {"two_lighters_same_drop": med(same_drop), "same_lighter_other_drop": med(same_map_other_drop), "other_song": med(other_song)}
    (HERE / "bs/compare.json").write_text(json.dumps(res, indent=1))
    print(json.dumps(res, indent=1))
    print("worst grid shares:", sorted(grid_rows, key=lambda r: r[5])[:6])


if __name__ == "__main__":
    main()
