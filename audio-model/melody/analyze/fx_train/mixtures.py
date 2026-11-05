"""Synthetic strongly labelled mixtures, the way DCASE builds them with Scaper: effects pasted into real
songs with exact labels, then embedded once and cached.

    CUDA_DEVICE_ORDER=PCI_BUS_ID CUDA_VISIBLE_DEVICES=3 python -m analyze.fx_train.mixtures <set> ...
    # sets: train_gen train_yt val_gen val_yt test_gen test_yt test_rb, or smoke

A mixture is 50 s of a song's effect-free bars (spans.py), of which the central 40 s are kept, so the
pieces at the edges see the same context as inside a song. Effects land musically: risers and
reverse cymbals end on a bar line, impacts, downlifters and sirens start on one, horns on a beat. A
tenth land anywhere, so the grid alone can't give them away.

Half the mixtures mute drums and bass for some bars and bring them back on a bar line, sometimes after
a silent gap: a fake drop made from stems. Effects are placed largely independently of this, and many
fake drops carry no effect, so the head can't learn "no kick means riser" or "the kick came back
means impact". That is what would make the real test (risers before drops) meaningless.
"""

import json
import sys
import time
from multiprocessing import Pool

import numpy as np

from analyze import common
from analyze.fx_train import backbone as bb, effects as fx, spans
from analyze.fx_train.backbone import FPS, FX_SR, HOP
from analyze.fx_train.effects import CLASSES, Effect

OUT = common.ROOT / "tmp/fx/mix"
CUTS_NPY = common.ROOT / "models/fx_head/sources/cuts.npy"
CUTS_JSON = common.ROOT / "models/fx_head/sources/cuts.json"
TOTAL_S, MARGIN_S = 50, 5
KEEP = (40 * FPS)
WEIGHTS = {"riser": 0.3, "downlifter": 0.15, "impact": 0.25, "horn": 0.1, "siren": 0.1, "revcym": 0.1}
LEVELS = {"riser": (-12, 6), "downlifter": (-15, 3), "impact": (-8, 8), "horn": (-8, 8), "siren": (-12, 4), "revcym": (-12, 4)}
SETS = {"train_gen": (6000, "gen", "train"), "train_yt": (6000, "yt", "train"), "val_gen": (300, "gen", "val"), "val_yt": (300, "yt", "val"),
        "test_gen": (400, "gen", "test"), "test_yt": (400, "yt", "test"), "test_rb": (400, "rb", "test"), "smoke": (40, "gen", "train")}

_w = {}


def _init():
    _w["splits"] = spans.splits()
    _w["meta"] = spans.meta()
    _w["runs"] = {}
    try:
        info = json.loads(CUTS_JSON.read_text())
        _w["cuts"] = {}
        for c in info["cuts"]:
            _w["cuts"].setdefault((c["class"], c["split"]), []).append(c)
        _w["cut_data"] = np.load(CUTS_NPY, mmap_mode="r")
    except OSError:
        _w["cuts"] = {}
    z = np.load(spans.HERE / "rekordbox_oneshots.npz")
    rows = json.loads(str(z["rows"]))
    rb = {}
    for r in rows:
        x = z[f"a{r['i']}"].astype(np.float32) / 32768
        x = _trim(x)
        rb.setdefault(r["class"], []).append((x, r["title"]))
        # the classes Rekordbox doesn't bundle: reversing its downlifters and cymbals is what producers do
        if r["class"] == "downlifter":
            rb.setdefault("riser", []).append((x[::-1].copy(), r["title"] + " reversed"))
        if r["title"] in ("CYMBAL", "CLASH CYMBAL"):
            rb.setdefault("revcym", []).append((x[::-1].copy(), r["title"] + " reversed"))
    _w["rb"] = rb


def _trim(x, floor=1e-3):
    on = np.nonzero(np.abs(x) > floor * np.abs(x).max())[0]
    return x[on[0]: on[-1] + 1] if len(on) else x


def _song_pool(split):
    m = _w["meta"]
    ids = _w["splits"][split]
    el = [s for s in ids if m[s]["genre"] == "electronic"]
    pop = [s for s in ids if m[s]["genre"] != "electronic"]
    return el, pop


def _runs(sid):
    if sid not in _w["runs"]:
        a, b, r = spans.clean_runs(sid)
        _w["runs"][sid] = (a, b, r)
    return _w["runs"][sid]


def _compose(sid, rng, total):
    """Bars of clean runs one after another: stems (4, n) at 32 kHz and the bar lines in samples."""
    a, b, runs = _runs(sid)
    s = np.load(common.ROOT / "stems_songs" / f"{sid}.npy", mmap_mode="r")
    w = np.array([r[1] - r[0] for r in runs], float)
    parts, lines, n = [], [], 0
    fade = int(0.003 * common.SR)
    ramp = np.linspace(0, 1, fade, dtype=np.float32)
    need = int(total * common.SR) + common.SR
    while n < need:
        r = runs[rng.choice(len(runs), p=w / w.sum())]
        i0 = int(rng.integers(r[0], max(r[0] + 1, r[1] - 1)))
        s0 = int(a[i0] * common.SR)
        s1 = int(b[r[1] - 1] * common.SR)
        s1 = min(s1, s.shape[1], s0 + need - n + common.SR)
        if s1 - s0 < common.SR:
            continue
        chunk = np.asarray(s[:, s0: s1], np.float32)
        chunk[:, :fade] *= ramp
        chunk[:, -fade:] *= ramp[::-1]
        for i in range(i0, r[1]):
            t = int(a[i] * common.SR) - s0 + n
            if t < n + (s1 - s0):
                lines.append(t)
        parts.append(chunk)
        n += s1 - s0
    x = np.concatenate(parts, axis=1)[:, :need]
    y = np.stack([bb.to_32k(c) for c in x])
    k = FX_SR / common.SR
    return y[:, : int(total * FX_SR)], np.array([int(t * k) for t in lines if t * k < total * FX_SR])


def _masks(n, lines, rng):
    """Mute stems for some bars before a bar line: (gains per stem drums, bass, vocals, all), fake-drop sample or None."""
    g = np.ones((4, n), np.float32)
    lo, hi = MARGIN_S * FX_SR, n - MARGIN_S * FX_SR
    cand = [t for t in lines if lo + 2 * FX_SR < t < hi - FX_SR]
    if not cand or rng.uniform() > 0.5:
        return g, None
    t = int(rng.choice(cand))
    idx = int(np.searchsorted(lines, t))
    bars = int(rng.choice([2, 4, 8, 16]))
    s = int(lines[max(idx - bars, 0)])
    mode = rng.choice(["db", "d", "b", "dbv"], p=[0.7, 0.15, 0.05, 0.1])
    r = int(0.005 * FX_SR)
    env = np.ones(n, np.float32)
    env[s: t] = 0
    if r:
        env = np.convolve(env, np.ones(r) / r, mode="same").astype(np.float32)
    if "d" in mode:
        g[0] = env
    if "b" in mode:
        g[1] = env
    if "v" in mode:
        g[2] = env
    if rng.uniform() < 0.25:  # the pull-up: everything stops just before the bar line
        beat = (lines[idx] - lines[max(idx - 1, 0)]) / 4 if idx else FX_SR / 2
        k = int(beat * rng.choice([0.5, 1, 2]))
        g[3, max(t - k, 0): t] = 0
    return g, t


def _from_cut(x, kind, c, rng, src, sid):
    k = rng.uniform(0.9, 1.1)  # a little speed change: pitch and length together
    n = int(len(x) / k)
    y = np.interp(np.linspace(0, len(x) - 1, n), np.arange(len(x)), x).astype(np.float32)
    hit, a, b = int(c["hit"] / k), int(c["a"] / k), int(c["b"] / k)
    return Effect(kind, f"{src}_{kind}", y, hit, "end" if kind in ("riser", "revcym") else "start", a, b, src, sid)


def _pick(kind, source, split, rng, beat):
    if source == "gen":
        return fx.make(kind, rng, beat)
    if source == "yt":
        # in training a riser is often a reversed downlifter and the other way round, as producers make
        # them: the same timbre under both labels, so only the direction tells them apart
        flip = split == "train" and kind in ("riser", "downlifter") and rng.uniform() < 0.4
        from_kind = {"riser": "downlifter", "downlifter": "riser"}[kind] if flip else kind
        cs = _w["cuts"].get((from_kind, split)) or []
        if not cs:
            raise ValueError(f"no YouTube cuts for {from_kind} in {split}")
        c = cs[int(rng.integers(len(cs)))]
        x = np.asarray(_w["cut_data"][c["offset"]: c["offset"] + c["n"]], np.float32) / 32768
        if flip:
            from analyze.fx_train.cuts import align

            x = x[::-1].copy()
            hit, a, b = align(kind, x)
            c = {"hit": hit, "a": a, "b": b, "video": c["video"], "t": c["t"]}
        return _from_cut(x, kind, c, rng, "yt", f"{c['video']}@{c['t']}{'r' if flip else ''}")
    from analyze.fx_train.cuts import align

    x, title = _w["rb"][kind][int(rng.integers(len(_w["rb"][kind])))]
    hit, a, b = align(kind, x)
    return _from_cut(x, kind, {"hit": hit, "a": a, "b": b}, rng, "rb", title)


def _music_sweep(other, end, bars_len, rng):
    """A filter sweep over the music itself, the DSP analyzer's other kind of riser."""
    start = max(end - bars_len, 0)
    n = end - start
    x = other[start: end].copy()
    c = fx.curve(n, rng)
    f = fx.glide(float(rng.uniform(300, 1500)), float(rng.uniform(5000, 14000)), c)
    y = fx.tv_band(x, f * rng.uniform(0.4, 0.8), None, res=rng.choice([0, 2, 5])) * (0.3 + 0.7 * c)
    return Effect("riser", "riser_music_sweep", fx.norm(y), n, "end", 0, n), start


def _grid_point(kind, lines, beat, n, rng, fake):
    lo, hi = MARGIN_S * FX_SR, n - MARGIN_S * FX_SR
    if rng.uniform() < 0.1:
        return int(rng.integers(lo, hi))
    if fake is not None and kind in ("riser", "impact", "revcym", "downlifter") and rng.uniform() < 0.3:
        t = fake
    else:
        cand = lines[(lines > lo) & (lines < hi)]
        if not len(cand):
            return int(rng.integers(lo, hi))
        t = int(rng.choice(cand))
        if kind == "horn":
            t += int(int(rng.integers(0, 4)) * beat * FX_SR)
    return int(np.clip(t + rng.normal(0, 0.008) * FX_SR, 0, n - 1))


def _rms_loud(x):
    """RMS of the loudest quarter of a signal, on 50 ms frames."""
    m = max(int(0.05 * FX_SR), 1)
    k = max(len(x) // m, 1)
    e = (x[: k * m].reshape(k, -1) ** 2).mean(1) if len(x) >= m else np.array([(x ** 2).mean()])
    return float(np.sqrt(np.sort(e)[-max(k // 4, 1):].mean()) + 1e-9)


def _rms(x):
    return float(np.sqrt((x ** 2).mean()) + 1e-9) if len(x) else 1e-9


def make(args):
    """One mixture. Returns audio at 32 kHz, labels and the ignore mask (kept frames), metadata."""
    seed, source, split = args
    rng = np.random.default_rng(seed)
    el, pop = _song_pool(split)
    while True:
        sid = str(rng.choice(pop if pop and rng.uniform() < 0.15 else el))
        if sum(r[1] - r[0] for r in _runs(sid)[2]) >= 16:
            break
    v = _w["meta"][sid]
    beat = float(np.median(np.diff(v["beats"])))
    n = TOTAL_S * FX_SR
    S, lines = _compose(sid, rng, TOTAL_S)
    mix, drums, bass, vocals = S
    other = mix - drums - bass - vocals
    g, fake = _masks(n, lines, rng)
    bg = (mix - (1 - g[0]) * drums - (1 - g[1]) * bass - (1 - g[2]) * vocals) * g[3]
    M_bg = bb.mel_power(bg)
    frames = M_bg.shape[0]
    y, m = np.zeros((frames, len(CLASSES)), np.uint8), np.ones((frames, len(CLASSES)), np.uint8)
    track = np.zeros(n, np.float32)
    meta = {"song": sid, "source": source, "fake_drop": None if fake is None else round((fake - MARGIN_S * FX_SR) / FX_SR, 3), "events": [], "distractors": []}
    count = int(rng.choice([0, 1, 2, 3], p=[0.15, 0.4, 0.3, 0.15]))
    kinds = list(rng.choice(CLASSES, size=count, p=[WEIGHTS[c] for c in CLASSES]))
    for kind in kinds:
        kind = str(kind)
        t = _grid_point(kind, lines, beat, n, rng, fake)
        if source == "gen" and kind == "riser" and rng.uniform() < 0.15:
            e, _ = _music_sweep(other, t, int(rng.choice([2, 4, 8]) * 4 * beat * FX_SR), rng)
        else:
            e = _pick(kind, source, split, rng, beat)
        start = t - e.hit
        s0, s1 = max(start, 0), min(start + len(e.audio), n)
        if s1 - s0 < 16:
            continue
        a = e.audio[s0 - start: s1 - start]
        lo, hi = LEVELS[kind]
        level = float(rng.uniform(lo, hi))
        ls0, ls1 = max(start + e.a, 0), min(start + e.b, n)
        ref = max(_rms(bg[ls0: ls1]), 10 ** (-40 / 20))
        gain = 10 ** (level / 20) * ref / _rms_loud(e.audio[e.a: e.b] if e.b > e.a else e.audio)
        a = a * gain
        one = np.zeros(n, np.float32)
        one[s0: s1] = a
        track += one
        # the label covers what can be heard over this background: any three mel bands within 3 dB of it
        M_fx = bb.mel_power(one)
        hear = (M_fx > 0.5 * M_bg).sum(1) >= 3
        f0, f1 = int(ls0 / HOP), min(int(np.ceil(ls1 / HOP)), frames)
        f1 = max(f1, f0 + 1)
        j = CLASSES.index(kind)
        inside = np.nonzero(hear[f0: f1])[0]
        audible = len(inside) >= max(2, 0.15 * (f1 - f0))
        tail_end = min(f1 + int(2.0 * FPS), frames, int(np.ceil(s1 / HOP)))
        if audible:
            h0, h1 = f0 + int(inside[0]), f0 + int(inside[-1]) + 1
            m[f0: h0, j] = 0
            y[h0: h1, j] = 1
            m[h1: f1, j] = 0
            m[f1: tail_end, j] = 0
        else:
            m[f0: tail_end, j] = 0
            h0, h1 = f0, f1
        c0 = MARGIN_S * FPS
        meta["events"].append({"kind": kind, "family": e.family, "source": e.source, "id": e.source_id, "level": round(level, 1), "audible": bool(audible),
                               "start": round((h0 - c0) / FPS, 2), "end": round((h1 - c0) / FPS, 2)})
    # things that aren't effects
    extra = [fx.distractor(rng, beat)] if rng.uniform() < 0.3 else []
    if source == "rb" and rng.uniform() < 0.5:
        x, title = _w["rb"]["none"][int(rng.integers(len(_w["rb"]["none"])))]
        extra.append(Effect("none", "rb_none", x, 0, "start", 0, 0, "rb", title))
    for e in extra:
        t = _grid_point("horn", lines, beat, n, rng, None)
        s1 = min(t + len(e.audio), n)
        a = e.audio[: s1 - t]
        if len(a) < 16:
            continue
        track[t: s1] += a * 10 ** (rng.uniform(-14, 2) / 20) * max(_rms(bg[t: s1]), 0.01) / _rms_loud(e.audio)
        meta["distractors"].append(e.family if e.source != "rb" else e.source_id)
    x = bg + track
    pk = float(np.abs(x).max())
    if pk > 0.98:  # a master limiter, more or less
        x = np.tanh(x / pk * 1.4) / np.tanh(1.4) * 0.98
    c0 = MARGIN_S * FPS
    return x.astype(np.float32), y[c0: c0 + KEEP], m[c0: c0 + KEEP], meta


def generate(name, jobs=14):
    n, source, split = SETS[name]
    OUT.mkdir(parents=True, exist_ok=True)
    emb = np.lib.format.open_memmap(OUT / f"{name}.emb.npy", mode="w+", dtype=np.float16, shape=(n, KEEP, bb.DIM))
    mel = np.lib.format.open_memmap(OUT / f"{name}.mel.npy", mode="w+", dtype=np.float16, shape=(n, KEEP, bb.N_MELS))
    Y = np.lib.format.open_memmap(OUT / f"{name}.y.npy", mode="w+", dtype=np.uint8, shape=(n, KEEP, len(CLASSES)))
    M = np.lib.format.open_memmap(OUT / f"{name}.m.npy", mode="w+", dtype=np.uint8, shape=(n, KEEP, len(CLASSES)))
    beats = bb.Beats()
    base = abs(hash(name)) % 100000 if False else sum(ord(c) * 131 ** i for i, c in enumerate(name)) % 10 ** 8
    args = [(base + i, source, split) for i in range(n)]
    t0, metas = time.time(), []
    c0 = MARGIN_S * FPS
    with Pool(jobs, initializer=_init) as pool:
        for i, (x, y, m, meta) in enumerate(pool.imap(make, args, chunksize=2)):
            e, ml = bb.features(beats, x)
            emb[i] = e[c0: c0 + KEEP].astype(np.float16)
            mel[i] = ml[c0: c0 + KEEP].astype(np.float16)
            Y[i], M[i] = y, m
            metas.append(meta)
            if (i + 1) % 200 == 0 or i + 1 == n:
                print(f"{name}: {i + 1}/{n}, {(i + 1) / (time.time() - t0):.1f} mixtures/s", flush=True)
    for a in (emb, mel, Y, M):
        a.flush()
    (OUT / f"{name}.json").write_text(json.dumps(metas, ensure_ascii=False))
    print(f"{name}: done in {time.time() - t0:.0f} s", flush=True)


if __name__ == "__main__":
    for s in sys.argv[1:]:
        generate(s)
