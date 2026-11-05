"""Turn the YouTube downloads into labelled one-shots, and filter hard.

    python -m analyze.fx_train.cuts            # cut, check, split: writes sources/cuts.npy, cuts.json
    python -m analyze.fx_train.cuts audit      # a contact sheet of random cuts per class, for the eye

Steps: decode to mono 32 kHz; cut on silence; keep what fits the class's duration; a spectral check
per class (a riser has to end louder or brighter than it starts, an impact has to hit at its start,
a horn has to be tonal and so on); drop speech and singing using PretrainedSED's own scores; then
split by video, channel and near-identical title so nothing leaks, and drop test or val cuts that
are near-copies of a train cut (uploaders re-post the same packs).
"""

import hashlib
import json
import re
import subprocess
import sys
from pathlib import Path

import numpy as np

from analyze import common
from analyze.fx_train.backbone import FX_SR, mel_power

SRC = common.ROOT / "models/fx_head/sources"
AUDIO = SRC / "audio"
OUT_NPY, OUT_JSON = SRC / "cuts.npy", SRC / "cuts.json"
HOP = int(0.02 * FX_SR)  # 20 ms analysis frames
# seconds a cut of each class may last; a siren that keeps going gets chopped
DUR = {"riser": (1.0, 45.0), "downlifter": (0.4, 25.0), "impact": (0.5, 12.0), "horn": (0.2, 12.0), "siren": (0.8, 16.0), "revcym": (0.25, 10.0)}
SPEECH = ["Speech", "Male speech, man speaking", "Female speech, woman speaking", "Narration, monologue", "Conversation", "Singing",
          "Male singing", "Female singing", "Child speech, kid speaking", "Rapping"]


def decode(path):
    pcm = subprocess.run(["ffmpeg", "-v", "error", "-i", str(path), "-ac", "1", "-ar", str(FX_SR), "-f", "f32le", "-"],
                         check=True, capture_output=True).stdout
    return np.frombuffer(pcm, np.float32).copy()


def split_on_silence(x, floor_db=-42.0, gap_s=0.12):
    """[start, end) sample ranges between silences, relative to the file's loud level."""
    n = len(x) // HOP
    if n < 5:
        return []
    e = np.sqrt((x[: n * HOP].reshape(n, HOP) ** 2).mean(1) + 1e-12)
    db = 20 * np.log10(e)
    ref = np.percentile(db, 98)
    on = db > ref + floor_db
    runs = common.runs(on, 2, max_gap=int(gap_s / 0.02) - 1)
    return [(a * HOP, b * HOP) for a, b in runs]


def describe(x):
    """Numbers the spectral checks use, computed on 40 ms mel frames."""
    P = mel_power(x)
    bands = np.arange(P.shape[1])
    e = P.sum(1) + 1e-9
    level = 10 * np.log10(e)
    level = np.convolve(np.pad(level, 2, mode="edge"), np.ones(5) / 5, mode="valid")
    cen = (np.log2(np.arange(1, P.shape[1] + 1)) * P).sum(1) / e  # log mel position, about octaves
    hot = level > level.max() - 30
    n = len(level)
    third = max(n // 3, 1)
    pk = int(np.argmax(level))
    flat = np.exp(np.log(P + 1e-9).mean(1)) / (P.mean(1) + 1e-9)
    # how much the loudest band moves while it is loud: a siren's pitch, in bands
    top = P.argmax(1)
    motion = float(np.std(top[hot])) if hot.sum() > 3 else 0.0
    weights = 10 ** ((level - level.max()) / 10)
    a, b = cen[: third], cen[-third:]
    wa, wb = weights[: third] + 1e-6, weights[-third:] + 1e-6
    return {"dur": len(x) / FX_SR, "peak_pos": pk / max(n - 1, 1), "peak_t": pk / 25, "d_level": float(level[-third:].mean() - level[: third].mean()),
            "d_cen": float((b * wb).sum() / wb.sum() - (a * wa).sum() / wa.sum()), "flat": float(flat[hot].mean()) if hot.any() else 0.0,
            "motion": motion, "cen": float((cen * weights).sum() / weights.sum()), "low": float(P[:, :10].sum() / P.sum()),
            "attack": float(max(pk, 0) / 25), "band": bands[0] * 0 + float((P[hot].mean(0).argmax() if hot.any() else 0))}


def passes(cls, f):
    """The class's spectral check: loose on purpose, the title already says something."""
    if cls == "riser":
        return f["peak_pos"] >= 0.5 and (f["d_cen"] >= 0.15 or f["d_level"] >= 5)
    if cls == "downlifter":
        return f["peak_pos"] <= 0.5 and (f["d_cen"] <= -0.15 or f["d_level"] <= -5)
    if cls == "impact":
        return f["peak_t"] <= 0.3 and f["d_level"] <= -6 and f["attack"] <= 0.3
    if cls == "horn":
        return f["flat"] < 0.35 and f["cen"] > 1.5
    if cls == "siren":
        return f["flat"] < 0.15 and f["motion"] >= 1.0
    if cls == "revcym":
        return f["peak_pos"] >= 0.55 and f["d_level"] >= 6 and f["flat"] > 0.05 and f["cen"] > 3.0
    return False


def align(cls, x):
    """(hit, a, b): the sample that goes on the grid and the span the label covers."""
    e = np.abs(x)
    n = len(x)
    block = max(int(0.02 * FX_SR), 1)
    m = n // block
    if m == 0:
        return (0, 0, n)
    env = e[: m * block].reshape(m, block).max(1)
    pk = int(np.argmax(env))
    if cls in ("riser", "revcym"):  # the top of the rise lands on the bar line; what's after is tail
        late = np.nonzero(env >= env[pk] * 0.5)[0]
        end = int((late[-1] + 1) * block)
        return (min(end, n), 0, min(end, n))
    if cls == "impact":
        below = np.nonzero(env[pk:] < env[pk] * 0.3)[0]
        b = int(np.clip((pk + (below[0] if len(below) else 40)) * block, 0.16 * FX_SR, 0.8 * FX_SR))
        return (0, 0, min(b, n))
    return (0, 0, n)  # downlifter, horn, siren: starts on the grid, whole span


def _title_key(t):
    t = re.sub(r"[^a-z0-9 ]", " ", t.lower())
    t = re.sub(r"\b(free|download|no|copyright|royalty|sound|effect|effects|sfx|fx|hd|hq|by|the|a|of|for|and|with)\b", " ", t)
    return " ".join(sorted(set(t.split())))


def assign_splits(rows):
    """Connect videos that share a channel or a near-identical title, and split the groups."""
    parent = {}

    def find(x):
        parent.setdefault(x, x)
        while parent[x] != x:
            parent[x] = parent[parent[x]]
            x = parent[x]
        return x

    for r in rows:
        v = "v:" + r["id"]
        parent[find(v)] = find("c:" + (r.get("channel_id") or r.get("channel") or r["id"]))
        k = _title_key(r["title"])
        if len(k) >= 12:
            parent[find(v)] = find("t:" + k)
    return {r["id"]: find("v:" + r["id"]) for r in rows}


def stratify(cuts, groups, target=(("train", 0.65), ("val", 0.12), ("test", 0.23)), seed=3):
    """Whole groups go to one split, chosen so that every class ends up near the target shares. A plain
    hash left validation without risers: a few groups hold most of a class's cuts."""
    per = {}
    for c in cuts:
        g = groups[c["video"]]
        per.setdefault(g, {}).setdefault(c["class"], 0)
        per[g][c["class"]] += 1
    total = {cls: sum(p.get(cls, 0) for p in per.values()) for cls in DUR}
    have = {k: {cls: 0 for cls in DUR} for k, _ in target}
    out, rng = {}, np.random.default_rng(seed)
    order = sorted(per, key=lambda g: (-max(per[g].values()), rng.uniform()))
    for g in order:
        def cost(k):
            err = 0.0
            for name, want in target:
                for cls in DUR:
                    n = have[name][cls] + (per[g].get(cls, 0) if name == k else 0)
                    err += (n / max(total[cls], 1) - want) ** 2
            return err

        k = min((name for name, _ in target), key=cost)
        out[g] = k
        for cls, n in per[g].items():
            have[k][cls] += n
    return out


def fingerprint(x):
    P = mel_power(x)
    lv = np.log(P + 1e-6)
    t = np.array_split(lv, 8, axis=0)
    fp = np.concatenate([c.mean(0)[::4] for c in t if len(c)] or [np.zeros(16)])
    fp = np.pad(fp, (0, max(0, 128 - len(fp))))[:128]
    fp = fp - fp.mean()
    return (fp / (np.linalg.norm(fp) + 1e-9)).astype(np.float32)


def build():
    from analyze.fx_train import backbone as bb

    from analyze.fx_train.yt_sources import drop_unwanted

    drop_unwanted()
    rows = [json.loads(p.read_text()) for p in sorted(AUDIO.glob("*.json"))]
    groups = assign_splits(rows)
    print(len(rows), "videos in", len(set(groups.values())), "channel and title groups", flush=True)
    beats = bb.Beats()
    sys.path.insert(0, str(bb.SRC))
    from data_util import audioset_classes

    names = list(audioset_classes.as_strong_train_classes)
    speech_idx = [names.index(n) for n in SPEECH if n in names]
    music_idx = [names.index("Music")] if "Music" in names else []
    cuts, audio, counts, pos = [], [], {}, 0

    def tally(cls, key, n=1):
        counts.setdefault(cls, {}).setdefault(key, 0)
        counts[cls][key] += n

    for vi, r in enumerate(rows):
        cls, vid = r["class"], r["id"]
        files = [p for p in AUDIO.glob(vid + ".*") if p.suffix != ".json"]
        if not files:
            continue
        x = decode(files[0])
        tally(cls, "videos")
        lo, hi = DUR[cls]
        pieces = []
        for a, b in split_on_silence(x):
            if cls == "siren" and (b - a) / FX_SR > hi:  # one long wail: several pieces of it
                step = int(8 * FX_SR)
                pieces += [(s, s + step) for s in range(a, b - step, step)][:6]
            else:
                pieces.append((a, b))
        tally(cls, "segments", len(pieces))
        keep = []
        for a, b in pieces:
            d = (b - a) / FX_SR
            if not (lo <= d <= hi):
                tally(cls, "rejected: duration")
                continue
            c = x[a: b]
            f = describe(c)
            if not passes(cls, f):
                tally(cls, "rejected: spectral")
                continue
            keep.append((a, b, c, f))
        if not keep:
            continue
        # speech and singing, from the frozen SED head; one piece per cut, padded
        batch = np.stack([np.pad(bb.to_16k(c)[: bb.SEG], (0, max(0, bb.SEG - len(bb.to_16k(c))))) for _, _, c, _ in keep]).astype(np.float32)
        S = beats.strong(batch)
        for (a, b, c, f), s in zip(keep, S):
            frames = min(max(int((b - a) / FX_SR * 25), 3), S.shape[2])
            sp = float(s[speech_idx][:, : frames].max(0).mean())
            mu = float(s[music_idx][:, : frames].mean()) if music_idx else 0.0
            if sp > 0.25:
                tally(cls, "rejected: speech")
                continue
            peak = float(np.abs(c).max())
            if peak < 1e-3:
                continue
            hit, la, lb = align(cls, c)
            q = (c / peak * 32767).astype(np.int16)
            cuts.append({"video": vid, "channel": r.get("channel", ""), "class": cls, "offset": pos, "n": len(q),
                         "t": round(a / FX_SR, 2), "hit": int(hit), "a": int(la), "b": int(lb), "speech": round(sp, 3), "music": round(mu, 3),
                         "licence": r.get("licence", ""), **{k: round(float(v), 3) for k, v in f.items()}})
            audio.append(q)
            pos += len(q)
            tally(cls, "kept")
        if vi % 25 == 0:
            print(f"{vi}/{len(rows)} videos, {len(cuts)} cuts", flush=True)
    data = np.concatenate(audio)
    which = stratify(cuts, groups)
    for c in cuts:
        c["split"] = which[groups[c["video"]]]
    # near-copies across splits: uploaders re-post the same packs under other channels
    fps = np.stack([fingerprint(data[c["offset"]: c["offset"] + c["n"]].astype(np.float32) / 32768) for c in cuts])
    idx = {k: [i for i, c in enumerate(cuts) if c["split"] == k] for k in ("train", "val", "test")}
    dropped = 0
    if idx["train"]:
        tr = fps[idx["train"]]
        trd = np.array([cuts[i]["dur"] for i in idx["train"]])
        for k in ("val", "test"):
            for i in idx[k]:
                sim = tr @ fps[i]
                j = np.nonzero((sim > 0.985) & (np.abs(trd - cuts[i]["dur"]) < 0.05 * cuts[i]["dur"] + 0.02))[0]
                if len(j):
                    cuts[i]["split"] = "dropped_copy"
                    dropped += 1
    # one video must not dominate a class: at most 60 cuts each, chosen at random
    rng = np.random.default_rng(7)
    per = {}
    for i in rng.permutation(len(cuts)):
        c = cuts[i]
        per[c["video"]] = per.get(c["video"], 0) + 1
        if per[c["video"]] > 60:
            c["split"] = "dropped_cap"
    np.save(OUT_NPY, data)
    OUT_JSON.write_text(json.dumps({"cuts": cuts, "funnel": counts, "copies_dropped": dropped}, ensure_ascii=False))
    print("funnel", json.dumps(counts, indent=0), flush=True)
    print("copies dropped", dropped, flush=True)
    for cls in DUR:
        for k in ("train", "val", "test"):
            cs = [c for c in cuts if c["class"] == cls and c["split"] == k]
            print(f"{cls:11s} {k:5s} {len(cs):5d} cuts from {len({c['video'] for c in cs}):3d} videos, {len({c['channel'] for c in cs}):3d} channels, "
                  f"median {np.median([c['dur'] for c in cs]) if cs else 0:.1f} s, {sum(c['n'] for c in cs) / FX_SR / 60:.1f} min", flush=True)
    print(f"{data.nbytes / 1e6:.0f} MB in {len(cuts)} cuts", flush=True)


def _img(P, h=72, w=170):
    """A small spectrogram: 8-bit, time across, low frequencies at the bottom."""
    db = 10 * np.log10(P + 1e-9)
    db = np.clip((db - db.max() + 60) / 60, 0, 1)
    ti = np.linspace(0, len(db) - 1, w).astype(int)
    fi = np.linspace(0, db.shape[1] - 1, h).astype(int)
    return (db[ti][:, fi].T[::-1] * 255).astype(np.uint8)


def _png(path, a):
    import struct
    import zlib

    h, w = a.shape
    raw = b"".join(b"\0" + a[i].tobytes() for i in range(h))

    def chunk(t, d):
        return struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d) & 0xffffffff)

    Path(path).write_bytes(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 0, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b""))


def audit(out_dir, n=24, seed=23):
    """Contact sheets: 24 random usable cuts per class, 6 across. Left to right is time (each stretched
    to its width), bottom to top is frequency on a mel scale. Index in the .txt gives title and time."""
    meta = json.loads(OUT_JSON.read_text())
    data = np.load(OUT_NPY, mmap_mode="r")
    rng = np.random.default_rng(seed)
    out_dir = Path(out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    titles = {json.loads(p.read_text())["id"]: json.loads(p.read_text())["title"] for p in AUDIO.glob("*.json")}
    for cls in DUR:
        cs = [c for c in meta["cuts"] if c["class"] == cls and c["split"] in ("train", "val", "test")]
        if not cs:
            continue
        pick = rng.choice(len(cs), size=min(n, len(cs)), replace=False)
        rows, line, lines = [], [], []
        for k, i in enumerate(pick):
            c = cs[i]
            x = np.asarray(data[c["offset"]: c["offset"] + c["n"]], np.float32) / 32768
            im = _img(mel_power(x))
            im = np.pad(im, 2, constant_values=255)
            line.append(im)
            lines.append(f"{k:2d} ({k // 6},{k % 6}) {c['dur']:5.1f} s peak_pos {c['peak_pos']:.2f} d_lvl {c['d_level']:5.1f} d_cen {c['d_cen']:5.2f} flat {c['flat']:.2f} motion {c['motion']:.1f} {c['video']} @{c['t']:.1f} {titles.get(c['video'], '')[:54]}")
            if len(line) == 6:
                rows.append(np.concatenate(line, axis=1))
                line = []
        if line:
            rows.append(np.concatenate(line + [np.full_like(line[0], 255)] * (6 - len(line)), axis=1))
        _png(out_dir / f"{cls}.png", np.concatenate(rows, axis=0))
        (out_dir / f"{cls}.txt").write_text("\n".join(lines) + "\n")


if __name__ == "__main__":
    if len(sys.argv) > 1 and sys.argv[1] == "audit":
        audit(sys.argv[2] if len(sys.argv) > 2 else str(common.ROOT / "tmp/fx/audit"))
    else:
        build()
