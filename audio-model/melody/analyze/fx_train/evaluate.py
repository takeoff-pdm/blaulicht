"""How to judge a head. There is no ground truth for the real songs, so the real test is about agreement
with musical structure against a null, not accuracy.

    python -m analyze.fx_train.evaluate cache dev test        # embed real songs, forward and reversed
    python -m analyze.fx_train.evaluate synth a.pt b.pt       # 1. synthetic hold-out: frame and event F1
    python -m analyze.fx_train.evaluate calibrate a.pt b.pt   # riser, impact thresholds from dev songs' firing rate
    python -m analyze.fx_train.evaluate real --split test a.pt b.pt   # 2. plausibility, against the DSP baseline
    python -m analyze.fx_train.evaluate curve --split dev a.pt  # real drops, empty fake drops and null per threshold
    python -m analyze.fx_train.evaluate list a.pt             # 3. cases for a human to check
    python -m analyze.fx_train.evaluate labels <fxhead features dir> <labels dir>   # numpy only

The real test: of the drops in show.drops, how many have a riser ending in the last bar before them,
against the same measure at bar lines of the same song away from any drop. A head that only says
"this is a breakdown" would pass that, so there's a stricter null (bar lines after a Breakdown bar with
no drop coming) and a reversal check: played backwards the context stays (no kick before a drop) but a
rising sweep falls, so a head hearing sweeps should lose its lift, and one reading the context keeps it.
Reversed music looks riser-like everywhere (a reversed kick is a tiny swell), so it's the lift that
counts there, not the level.
"""

import json
import sys
from pathlib import Path

import numpy as np

from analyze import common
from analyze.fx_train import spans
from analyze.fx_train.effects import CLASSES

CACHE = common.ROOT / "tmp/fx/songs"
RESULTS = common.ROOT / "models/fx_head/results"
MODELS = common.ROOT / "models/fx_head"
TOL_IMPACT = 0.16  # s, about the head's time resolution
LABEL_KIND = {"riser": "riser", "downlifter": "downlifter", "impact": "impact", "airhorn": "horn", "horn": "horn", "siren": "siren", "other": "revcym", "revcym": "revcym"}


# ---- the structure of a song

def structure(sid):
    v = spans.meta()[sid]
    db = spans.downbeats(v)
    if len(db) < 8:
        return None
    bar = float(np.median(np.diff(db)))
    drops = [d["time"] for d in v["drops"]]
    near = lambda t: any(-8 * bar <= t - d <= 4 * bar for d in drops)
    far = np.array([t for t in db if not near(t)])
    bd = [(s["start"], s["end"]) for s in v["sections"] if s["kind"] == "Breakdown"]
    quiet = np.array([t for t in far if any(a <= t - bar / 2 < b for a, b in bd)])
    return {"bar": bar, "drops": np.array(drops), "far": far, "quiet": quiet, "dur": v["duration"] or (db[-1] + bar)}


def riser_hit(ends, line, bar):
    """A riser that ends in the last bar before the line (a quarter bar after it counts: frames blur)."""
    return bool(((ends >= line - bar) & (ends <= line + bar / 4)).any())


def impact_hit(onsets, line):
    return bool((np.abs(onsets - line) <= TOL_IMPACT).any())


def auc(pos, neg):
    if not len(pos) or not len(neg):
        return None
    pos, neg = np.asarray(pos)[:, None], np.asarray(neg)[None, :]
    return float((pos > neg).mean() + 0.5 * (pos == neg).mean())


def pooled(rows, key):
    """Share over all lines of the kind."""
    hits, n = sum(r[key][0] for r in rows), sum(r[key][1] for r in rows)
    return hits / max(n, 1)


def bootstrap(rows, a, b, n=1000, seed=5):
    """95% interval of a's share minus b's and of their ratio, resampling whole songs."""
    rng = np.random.default_rng(seed)
    d, r = [], []
    for _ in range(n):
        pick = [rows[i] for i in rng.integers(0, len(rows), len(rows))]
        x, y = pooled(pick, a), pooled(pick, b)
        d.append(x - y)
        r.append(x / y if y > 0 else np.nan)
    return [round(float(np.percentile(d, q)), 3) for q in (2.5, 97.5)], [round(float(np.nanpercentile(r, q)), 2) if np.isfinite(r).any() else None for q in (2.5, 97.5)]


RATES = (0.01, 0.03, 0.05, 0.10)


def _at_rate(rows, rate):
    """The share of drops scored above the level that only `rate` of the far bar lines reach; None if the
    detector never fires that often (the DSP analyzer's score is 0 at most lines)."""
    far = np.concatenate([r["scores"]["far"] for r in rows]) if rows else np.zeros(0)
    if not len(far) or (far > 0).mean() < rate * 0.8:
        return None
    t = float(np.quantile(far, 1 - rate))
    pos = np.concatenate([r["scores"]["drop"] for r in rows])
    quiet = np.concatenate([r["scores"]["quiet"] for r in rows])
    return float((pos > t).mean()), float((quiet > t).mean()) if len(quiet) else 0.0


def at_null_rates(rows, n=400, seed=9):
    """The fair comparison: how many drops each detector marks when it fires equally often elsewhere."""
    rng = np.random.default_rng(seed)
    out = {}
    for rate in RATES:
        x = _at_rate(rows, rate)
        if x is None:
            out[str(rate)] = None
            continue
        boots = []
        for _ in range(n):
            pick = [rows[i] for i in rng.integers(0, len(rows), len(rows))]
            b = _at_rate(pick, rate)
            if b is not None:
                boots.append(b[0])
        out[str(rate)] = {"drop": round(x[0], 3), "quiet_null": round(x[1], 3), "drop_ci": [round(float(np.percentile(boots, q)), 3) for q in (2.5, 97.5)] if boots else None}
    return out


def summarise(rows, name):
    """rows: one dict per song with hit counts per line kind for a riser or impact detector."""
    out = {"drop": round(pooled(rows, "drop"), 3), "null": round(pooled(rows, "far"), 3), "quiet_null": round(pooled(rows, "quiet"), 3),
           "n_drops": sum(r["drop"][1] for r in rows), "n_null": sum(r["far"][1] for r in rows), "n_quiet": sum(r["quiet"][1] for r in rows)}
    out["lift"] = round(out["drop"] / out["null"], 2) if out["null"] > 0 else None
    out["lift_quiet"] = round(out["drop"] / out["quiet_null"], 2) if out["quiet_null"] > 0 else None
    di, ri = bootstrap(rows, "drop", "far")
    diq, riq = bootstrap(rows, "drop", "quiet")
    out.update(diff_ci=di, lift_ci=ri, diff_quiet_ci=diq, lift_quiet_ci=riq)
    aucs = [r["auc"] for r in rows if r.get("auc") is not None]
    if aucs:
        out["auc"] = round(float(np.mean(aucs)), 3)
    aq = [r["auc_quiet"] for r in rows if r.get("auc_quiet") is not None]
    if aq:
        out["auc_quiet"] = round(float(np.mean(aq)), 3)
    out["at_null_rate"] = at_null_rates(rows)
    return out


# ---- the DSP baseline

def dsp_rows(ids):
    risers, impacts, fires = [], [], []
    for sid in ids:
        st = structure(sid)
        if st is None:
            continue
        p = common.FEATURES / "risers" / f"{sid}.json"
        if not p.exists():
            continue
        r = json.loads(p.read_text())
        ends = np.array([e[1] for e in r["risers"]], float)
        strength = np.array([e[2] for e in r["risers"]], float)
        on = np.array([e[0] for e in r["impacts"]], float)
        power = np.array([e[1] for e in r["impacts"]], float)

        def score_r(L):
            k = (ends >= L - st["bar"]) & (ends <= L + st["bar"] / 4)
            return float(strength[k].max()) if k.any() else 0.0

        def score_i(L):
            k = np.abs(on - L) <= TOL_IMPACT
            return float(power[k].max()) if k.any() else 0.0

        risers.append(_row(st, lambda L: riser_hit(ends, L, st["bar"]), score_r))
        impacts.append(_row(st, lambda L: impact_hit(on, L), score_i))
        fires.append({"riser": len(ends) / st["dur"] * 60, "impact": len(on) / st["dur"] * 60})
    return risers, impacts, fires


def _row(st, hit, score):
    row = {"id": None}
    for k, lines in (("drop", st["drops"]), ("far", st["far"]), ("quiet", st["quiet"])):
        row[k] = (sum(hit(L) for L in lines), len(lines))
    pos = [score(L) for L in st["drops"]]
    far = [score(L) for L in st["far"]]
    quiet = [score(L) for L in st["quiet"]]
    row["auc"] = auc(pos, far)
    row["auc_quiet"] = auc(pos, quiet)
    row["scores"] = {"drop": pos, "far": far, "quiet": quiet}
    return row


# ---- the head on real songs

def cached(sid):
    return np.load(CACHE / f"{sid}.npz")


def cache_songs(split_names):
    import time

    from analyze.fx_train import backbone as bb

    CACHE.mkdir(parents=True, exist_ok=True)
    ids = [s for k in split_names for s in spans.splits()[k]]
    beats = bb.Beats()
    t0 = time.time()
    for i, sid in enumerate(ids):
        p = CACHE / f"{sid}.npz"
        if p.exists():
            continue
        x = bb.to_32k(np.asarray(np.load(common.ROOT / "stems_songs" / f"{sid}.npy", mmap_mode="r")[0], np.float32))
        e, m = bb.features(beats, x)
        er, mr = bb.features(beats, x, reverse=True)
        tmp = p.with_suffix(".tmp.npz")
        np.savez(tmp, emb=e.astype(np.float16), mel=m.astype(np.float16), emb_r=er.astype(np.float16), mel_r=mr.astype(np.float16))
        tmp.replace(p)
        print(f"[{i + 1}/{len(ids)}] {sid} {len(e) / 25:.0f} s, {time.time() - t0:.0f} s so far", flush=True)


def _song_events(P, ck):
    from analyze.fx_train.head import events

    return events(P, ck["thresholds"])


def model_rows(path, ids, reverse=True):
    """Everything test 2 needs for one head."""
    from analyze.fx_train import head as H

    head, ck = H.load(path)
    thr = ck["thresholds"]
    jr, ji, jd = CLASSES.index("riser"), CLASSES.index("impact"), CLASSES.index("downlifter")
    risers, impacts, fires, rev, seam, keep = [], [], [], [], [], {}
    for sid in ids:
        st = structure(sid)
        if st is None or not (CACHE / f"{sid}.npz").exists():
            continue
        z = cached(sid)
        P = H.predict(head, ck, z["emb"], z["mel"])
        ev = H.events(P, thr)
        ends = np.array([e[1] for e in ev if e[2] == "riser"], float)
        on = np.array([e[0] for e in ev if e[2] == "impact"], float)
        bar = st["bar"]

        def score_r(L):
            a, b = int((L - bar) * 25), int((L + bar / 4) * 25)
            return float(P[max(a, 0): max(b, 1), jr].max())

        def score_i(L):
            a, b = int((L - TOL_IMPACT) * 25), int((L + TOL_IMPACT) * 25) + 1
            return float(P[max(a, 0): max(b, 1), ji].max())

        risers.append(_row(st, lambda L: riser_hit(ends, L, bar), score_r))
        impacts.append(_row(st, lambda L: impact_hit(on, L), score_i))
        minutes = len(P) / 25 / 60
        f = {}
        for j, c in enumerate(CLASSES):
            es = [e for e in ev if e[2] == c]
            f[c] = {"per_min": len(es) / minutes, "active": float((P[:, j] >= thr[j]).mean()), "dur": [e[1] - e[0] for e in es]}
        fires.append(f)
        seam += [e[0] for e in ev]
        if reverse:
            # lift with the audio reversed: the context (no kick before a drop) survives, a rising sweep doesn't
            Pr = H.predict(head, ck, z["emb_r"], z["mel_r"])[::-1]
            row = {}
            for k, lines in (("drop", st["drops"]), ("far", st["far"]), ("quiet", st["quiet"])):
                hf, hr, hd = 0, 0, 0
                for L in lines:
                    a, b = max(int((L - bar) * 25), 0), max(int((L + bar / 4) * 25), 1)
                    hf += bool((P[a: b, jr] >= thr[jr]).any())
                    hr += bool((Pr[a: b, jr] >= thr[jr]).any())
                    hd += bool((Pr[a: b, jd] >= thr[jd]).any())
                row[k] = {"fwd": (hf, len(lines)), "rev": (hr, len(lines)), "rev_down": (hd, len(lines))}
            rev.append(row)
        keep[sid] = {"events": ev}
    return risers, impacts, fires, rev, seam, keep, ck


def real(paths, split):
    ids = spans.splits()[split]
    ids = [s for s in ids if spans.meta()[s]["genre"] == "electronic"]
    dr, di, df = dsp_rows(ids)
    res = {"split": split, "songs": len(ids), "dsp": {"riser": summarise(dr, "dsp"), "impact": summarise(di, "dsp"),
                                                      "per_min": {"riser": round(float(np.median([f["riser"] for f in df])), 2), "impact": round(float(np.median([f["impact"] for f in df])), 2)}}}
    print(f"split {split}: {len(ids)} electronic songs, {res['dsp']['riser']['n_drops']} drops")
    print("DSP riser ", json.dumps(res["dsp"]["riser"]))
    print("DSP impact", json.dumps(res["dsp"]["impact"]))
    print("DSP per minute", res["dsp"]["per_min"], flush=True)
    for path in paths:
        name = Path(path).stem
        r, i, f, rev, seam, keep, ck = model_rows(path, ids)
        fire = {}
        for c in CLASSES:
            d = [x for s in f for x in s[c]["dur"]]
            fire[c] = {"per_min": round(float(np.median([s[c]["per_min"] for s in f])), 2), "active": round(float(np.median([s[c]["active"] for s in f])), 3),
                       "dur": round(float(np.median(d)), 2) if d else 0.0, "silent_songs": sum(s[c]["per_min"] == 0 for s in f)}
        seam = np.array(seam)
        near5 = float((np.minimum(seam % 5, 5 - seam % 5) <= 0.2).mean()) if len(seam) else 0.0
        def share(what, k):
            n = sum(x[k][what][1] for x in rev)
            return sum(x[k][what][0] for x in rev) / max(n, 1)

        revs = {}
        for what in ("fwd", "rev", "rev_down"):
            d, f_, q = share(what, "drop"), share(what, "far"), share(what, "quiet")
            revs[what] = {"drop": round(d, 3), "null": round(f_, 3), "quiet_null": round(q, 3), "lift": round(d / f_, 2) if f_ else None, "lift_quiet": round(d / q, 2) if q else None}
        res[name] = {"sources": ck["sources"], "features": ck["features"], "thresholds": [round(t, 2) for t in ck["thresholds"]], "riser": summarise(r, name), "impact": summarise(i, name), "fire": fire,
                     "reversal": revs, "share_of_events_within_0.2s_of_5s_multiple": round(near5, 3)}
        print(f"\n{name} ({ck['features']}, {','.join(ck['sources'])}) thresholds {res[name]['thresholds']}")
        print(" riser ", json.dumps(res[name]["riser"]))
        print(" impact", json.dumps(res[name]["impact"]))
        print(" fire  ", json.dumps(fire))
        print(" reversal", json.dumps(revs), "seam share (chance 0.08)", res[name]["share_of_events_within_0.2s_of_5s_multiple"], flush=True)
    RESULTS.mkdir(parents=True, exist_ok=True)
    (RESULTS / f"real_{split}.json").write_text(json.dumps(res, indent=1))


# ---- thresholds for real songs

TARGET = {"riser": 0.03, "impact": 0.005}  # about one riser end a minute and one impact every seven away from drops


def calibrate(paths, split="dev"):
    """Set the riser and impact thresholds so they fire at the target share of bar lines away from any drop,
    on songs no test number comes from. It never looks at the drops, so it can't favour the measure the head is
    judged on. The other classes keep the thresholds from synthetic validation. Writes them into the checkpoint."""
    import torch

    from analyze.fx_train import head as H

    ids = [s for s in spans.splits()[split] if spans.meta()[s]["genre"] == "electronic"]
    grid = np.arange(0.02, 0.985, 0.02)
    for path in paths:
        head, ck = H.load(path)
        tracks = {}
        for sid in ids:
            if (CACHE / f"{sid}.npz").exists():
                z = cached(sid)
                tracks[sid] = H.predict(head, ck, z["emb"], z["mel"])
        thr = list(ck.get("thresholds_synth", ck["thresholds"]))
        chosen = {}
        for kind, target in TARGET.items():
            j = CLASSES.index(kind)
            best, gap = thr[j], 9.0
            for t in grid:
                probe = list(thr)
                probe[j] = float(t)
                hits, n = 0, 0
                for sid, P in tracks.items():
                    st = structure(sid)
                    if st is None:
                        continue
                    ev = [e for e in H.events(P, probe) if e[2] == kind]
                    if kind == "riser":
                        ends = np.array([e[1] for e in ev], float)
                        hits += sum(riser_hit(ends, L, st["bar"]) for L in st["far"])
                    else:
                        on = np.array([e[0] for e in ev], float)
                        hits += sum(impact_hit(on, L) for L in st["far"])
                    n += len(st["far"])
                if abs(hits / max(n, 1) - target) < gap:
                    gap, best = abs(hits / max(n, 1) - target), float(t)
            thr[j] = best
            chosen[kind] = round(best, 2)
        raw = torch.load(path, map_location="cpu", weights_only=False)
        raw.setdefault("thresholds_synth", raw["thresholds"])
        raw["thresholds"] = thr
        torch.save(raw, path)
        print(Path(path).stem, "thresholds", chosen, "from", [round(x, 2) for x in raw["thresholds_synth"]], flush=True)


# ---- is it the effect, or just the drop?

def curve(paths, split="dev"):
    """At each threshold: real drops, bar lines away from drops, and fake drops with nothing pasted. A head that
    only reacts to kick and bass returning scores the same on real and empty fake drops at every threshold."""
    import torch

    from analyze.fx_train import head as H
    from analyze.fx_train.mixtures import OUT

    sets = ("val_gen", "val_yt") if split == "dev" else ("test_gen", "test_yt")
    ids = [s for s in spans.splits()[split] if spans.meta()[s]["genre"] == "electronic"]
    grid = (0.02, 0.05, 0.1, 0.15, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9)
    res = {}
    for path in paths:
        name = Path(path).stem
        head, ck = H.load(path)
        tracks = {sid: H.predict(head, ck, cached(sid)["emb"], cached(sid)["mel"]) for sid in ids if (CACHE / f"{sid}.npz").exists()}
        empties = []
        for s in sets:
            E, M0 = (np.load(OUT / f"{s}.{k}.npy", mmap_mode="r") for k in ("emb", "mel"))
            metas = json.loads((OUT / f"{s}.json").read_text())
            for i, meta in enumerate(metas):
                t = meta["fake_drop"]
                if t is None or not 2 < t < 39 or any(e["kind"] in ("riser", "impact", "revcym") and e["start"] - 2 < t < e["end"] + 2 for e in meta["events"]):
                    continue
                x = H.inputs(E[i], M0[i], ck["features"], ck["mean"], ck["std"])
                with torch.no_grad():
                    p = torch.sigmoid(head(torch.from_numpy(x[None]).cuda()))[0].cpu().numpy()
                bar = 4 * float(np.median(np.diff(spans.meta()[meta["song"]]["beats"])))
                empties.append((p, t, bar))
        out = {}
        for kind in ("riser", "impact"):
            j = CLASSES.index(kind)
            rows = []
            for th in grid:
                probe = list(ck["thresholds"])
                probe[j] = th

                def hit(ev, L, bar):
                    ev = [e for e in ev if e[2] == kind]
                    if kind == "riser":
                        return riser_hit(np.array([e[1] for e in ev], float), L, bar)
                    return impact_hit(np.array([e[0] for e in ev], float), L)

                d, n_d, f, n_f, minutes, count = 0, 0, 0, 0, 0.0, 0
                for sid, P in tracks.items():
                    st = structure(sid)
                    if st is None:
                        continue
                    ev = H.events(P, probe)
                    d += sum(hit(ev, L, st["bar"]) for L in st["drops"])
                    n_d += len(st["drops"])
                    f += sum(hit(ev, L, st["bar"]) for L in st["far"])
                    n_f += len(st["far"])
                    minutes += len(P) / 25 / 60
                    count += sum(e[2] == kind for e in ev)
                em = sum(hit(H.events(p, probe), t, bar) for p, t, bar in empties)
                rows.append({"t": th, "drop": round(d / max(n_d, 1), 3), "null": round(f / max(n_f, 1), 3), "empty": round(em / max(len(empties), 1), 3), "per_min": round(count / max(minutes, 1e-6), 2)})
            out[kind] = rows
            print(name, kind, f"({len(empties)} empty fake drops)")
            for r in rows:
                print(f"   t {r['t']:.2f} real drops {r['drop']:.3f} empty fake drops {r['empty']:.3f} null lines {r['null']:.3f} per min {r['per_min']}")
        res[name] = out
    RESULTS.mkdir(parents=True, exist_ok=True)
    (RESULTS / f"curve_{split}.json").write_text(json.dumps(res, indent=1))


# ---- synthetic hold-out

def _match(kind, ref, pred):
    """Greedy one-to-one matching of events; returns true positives."""
    used, tp = set(), 0
    for r in sorted(ref, key=lambda x: x[0]):
        for i, p in enumerate(pred):
            if i in used:
                continue
            if kind == "impact":
                ok = abs(p[0] - r[0]) <= 0.2
            else:
                inter = max(0.0, min(p[1], r[1]) - max(p[0], r[0]))
                union = max(p[1], r[1]) - min(p[0], r[0])
                edge = abs(p[1] - r[1]) <= 0.5 if kind in ("riser", "revcym") else abs(p[0] - r[0]) <= 0.5
                ok = inter / max(union, 1e-6) >= 0.3 or (inter > 0 and edge)
            if ok:
                used.add(i)
                tp += 1
                break
    return tp


def synth(paths, sets=("test_gen", "test_yt", "test_rb")):
    import torch

    from analyze.fx_train import head as H
    from analyze.fx_train.mixtures import OUT

    res = {}
    for path in paths:
        name = Path(path).stem
        head, ck = H.load(path)
        res[name] = {}
        for s in sets:
            if not (OUT / f"{s}.json").exists():  # written last, so a set still being made is skipped
                continue
            E, M0, Y, M = (np.load(OUT / f"{s}.{k}.npy", mmap_mode="r") for k in ("emb", "mel", "y", "m"))
            metas = json.loads((OUT / f"{s}.json").read_text())
            Ps = []
            for k in range(0, len(E), 16):
                x = np.stack([H.inputs(E[i], M0[i], ck["features"], ck["mean"], ck["std"]) for i in range(k, min(k + 16, len(E)))])
                with torch.no_grad():
                    Ps.append(torch.sigmoid(head(torch.from_numpy(x).cuda())).cpu().numpy())
            P = np.concatenate(Ps)
            thr = ck.get("thresholds_synth", ck["thresholds"])  # the synthetic task, at its own thresholds
            row = {}
            for j, c in enumerate(CLASSES):
                keep = np.asarray(M[..., j]) > 0
                p, y = P[..., j][keep] >= thr[j], np.asarray(Y[..., j])[keep] > 0
                tp, fp, fn = int((p & y).sum()), int((p & ~y).sum()), int((~p & y).sum())
                e_tp, e_ref, e_pred = 0, 0, 0
                for i, meta in enumerate(metas):
                    ref = [(e["start"], e["end"]) for e in meta["events"] if e["kind"] == c and e["audible"]]
                    pred = [(a, b) for a, b, k, _ in H.events(P[i], thr) if k == c]
                    e_tp += _match(c, ref, pred)
                    e_ref += len(ref)
                    e_pred += len(pred)
                row[c] = {"frame_f1": round(2 * tp / max(2 * tp + fp + fn, 1), 3), "event_f1": round(2 * e_tp / max(e_ref + e_pred, 1), 3), "events": e_ref}
            # fake drops with nothing pasted near them: does the context alone fire it? At the thresholds used
            # on real songs and with the same measure as the real test, so the two can be read together.
            real_thr = ck["thresholds"]
            hr, hi, nf = 0, 0, 0
            for i, meta in enumerate(metas):
                t = meta["fake_drop"]
                if t is None or not 2 < t < 39:
                    continue
                if any(e["kind"] in ("riser", "impact", "revcym") and e["start"] - 2 < t < e["end"] + 2 for e in meta["events"]):
                    continue
                v = spans.meta()[meta["song"]]
                bar = 4 * float(np.median(np.diff(v["beats"])))
                ev = H.events(P[i], real_thr)
                nf += 1
                hr += riser_hit(np.array([e[1] for e in ev if e[2] == "riser"], float), t, bar)
                hi += impact_hit(np.array([e[0] for e in ev if e[2] == "impact"], float), t)
            row["empty_fake_drops"] = {"n": nf, "riser_fires": round(hr / max(nf, 1), 3), "impact_fires": round(hi / max(nf, 1), 3)}
            row["mean_event_f1"] = round(float(np.mean([row[c]["event_f1"] for c in CLASSES if row[c]["events"]])), 3)
            res[name][s] = row
            print(name, s, "mean event F1", row["mean_event_f1"], " ".join(f"{c}:{row[c]['frame_f1']:.2f}/{row[c]['event_f1']:.2f}" for c in CLASSES), "empty drops", row["empty_fake_drops"], flush=True)
    RESULTS.mkdir(parents=True, exist_ok=True)
    (RESULTS / "synth.json").write_text(json.dumps(res, indent=1))


# ---- cases for a human

def _fmt(t):
    return f"{int(t // 60)}:{t % 60:04.1f}"


def cases(path, split="test", top=14):
    from analyze.fx_train import head as H

    head, ck = H.load(path)
    m = spans.meta()
    ids = [s for s in spans.splits()[split] if m[s]["genre"] == "electronic"]
    rows = []
    for sid in ids:
        if not (CACHE / f"{sid}.npz").exists():
            continue
        z = cached(sid)
        P = H.predict(head, ck, z["emb"], z["mel"])
        rp = common.FEATURES / "risers" / f"{sid}.json"
        d = json.loads(rp.read_text()) if rp.exists() else {"risers": [], "impacts": []}
        st = structure(sid)
        for a, b, k, pk in H.events(P, ck["thresholds"]):
            if k == "riser":
                seen = any(min(b, r[1]) > max(a, r[0]) for r in d["risers"])
            elif k == "impact":
                seen = any(abs(a - i[0]) <= 0.25 for i in d["impacts"])
            else:
                seen = None
            before_drop = bool(st and any(0 <= L - b <= st["bar"] for L in st["drops"]))
            on_drop = bool(st and any(abs(L - a) <= TOL_IMPACT for L in st["drops"]))
            rows.append({"id": sid, "name": m[sid]["name"], "start": a, "end": b, "kind": k, "peak": pk, "dsp_saw": seen, "before_drop": before_drop, "on_drop": on_drop})
    out = {"top": {}, "dsp_missed": {}, "named": {}}
    for k in CLASSES:
        c = sorted([r for r in rows if r["kind"] == k], key=lambda r: -r["peak"])
        out["top"][k] = c[:top]
        out["dsp_missed"][k] = [r for r in c if r["dsp_saw"] is False][:top]
    for sid in ("58e556af6855", "fe0b3c607b1b"):
        out["named"][sid] = [r for r in rows if r["id"] == sid]
    for title, group in (("top confidence", out["top"]), ("DSP missed", out["dsp_missed"])):
        print(f"\n== {title}")
        for k, rs in group.items():
            for r in rs[:top]:
                print(f"{k:10s} {r['peak']:.2f} {r['id']} {_fmt(r['start'])}-{_fmt(r['end'])} {'before-drop' if r['before_drop'] else ('on-drop' if r['on_drop'] else '')} {r['name'][:40]}")
    # horns and sirens have a second opinion: the sound-event analyzer
    agree = {}
    for k, names in (("horn", ("Air horn, truck horn",)), ("siren", ("Siren", "Civil defense siren"))):
        head_ev, head_seen, sed_ev, sed_seen = 0, 0, 0, 0
        for sid in ids:
            p = common.FEATURES / "sed" / f"{sid}.json"
            if not p.exists():
                continue
            sed = [e for e in json.loads(p.read_text()).get("events", []) if e[2] in names]
            mine = [r for r in rows if r["id"] == sid and r["kind"] == k]
            head_ev += len(mine)
            head_seen += sum(any(min(r["end"], e[1]) > max(r["start"], e[0]) for e in sed) for r in mine)
            sed_ev += len(sed)
            sed_seen += sum(any(min(r["end"], e[1]) > max(r["start"], e[0]) for r in mine) for e in sed)
        agree[k] = {"head_events": head_ev, "of_those_sed_too": head_seen, "sed_events": sed_ev, "of_those_head_too": sed_seen}
    out["agree_with_sed"] = agree
    print("\n== agreement with SED", json.dumps(agree))
    print("\n== named songs")
    for sid, rs in out["named"].items():
        print(m[sid]["name"], "drops", [_fmt(d["time"]) for d in m[sid]["drops"]])
        for r in rs:
            print(f"  {r['kind']:10s} {r['peak']:.2f} {_fmt(r['start'])}-{_fmt(r['end'])} dsp_saw={r['dsp_saw']} {'before-drop' if r['before_drop'] else ('on-drop' if r['on_drop'] else '')}")
    RESULTS.mkdir(parents=True, exist_ok=True)
    (RESULTS / f"cases_{Path(path).stem}.json").write_text(json.dumps(out, indent=1, ensure_ascii=False))


# ---- hand labels, once they exist

def against_labels(features_dir, labels_dir):
    """Reads audio-model/data/melody/labels/<id>.json, keeping files with status "done"; kinds riser, downlifter,
    impact (start == end), airhorn, siren, other. Scores fxhead's events against them."""
    labels = [json.loads(p.read_text()) | {"_id": p.stem} for p in sorted(Path(labels_dir).glob("*.json"))]
    labels = [d for d in labels if d.get("status") == "done" and "fx" in d]
    if not labels:
        print("no labelled songs with status done")
        return
    tally = {c: [0, 0, 0] for c in CLASSES}
    for d in labels:
        p = Path(features_dir) / f"{d['_id']}.json"
        if not p.exists():
            continue
        doc = json.loads(p.read_text())
        for c in CLASSES:
            out_kind = {"horn": "airhorn"}.get(c, c)
            ref = [(float(e["start"]), float(e["end"])) for e in d["fx"] if LABEL_KIND.get(e["kind"]) == c]
            pred = [(a, b) for a, b, k, _ in doc["events"] if k == out_kind]
            tally[c][0] += _match(c, ref, pred)
            tally[c][1] += len(ref)
            tally[c][2] += len(pred)
    print(len(labels), "labelled songs")
    for c, (tp, nr, np_) in tally.items():
        print(f"{c:11s} labelled {nr:4d} found {np_:4d} matched {tp:4d} precision {tp / max(np_, 1):.2f} recall {tp / max(nr, 1):.2f} F1 {2 * tp / max(nr + np_, 1):.2f}")


def main():
    cmd, args = sys.argv[1], sys.argv[2:]
    if cmd == "cache":
        cache_songs(args)
    elif cmd == "synth":
        synth(args)
    elif cmd == "real":
        split = args[1] if args[0] == "--split" else "test"
        real(args[2:] if args[0] == "--split" else args, split)
    elif cmd == "calibrate":
        calibrate(args)
    elif cmd == "curve":
        split = args[1] if args[0] == "--split" else "dev"
        curve(args[2:] if args[0] == "--split" else args, split)
    elif cmd == "list":
        cases(args[0], args[1] if len(args) > 1 else "test")
    elif cmd == "labels":
        against_labels(args[0], args[1])
    else:
        sys.exit(__doc__)


if __name__ == "__main__":
    main()
