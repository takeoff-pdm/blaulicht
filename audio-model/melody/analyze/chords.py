"""Chords and key over time, by chroma templates and Viterbi decoding (no external model, so no
licence strings attached; EDM harmony is mostly triads, which this handles well).

Chroma: the harmonic part of the song (bass + other, no drums or vocals) in an 8192-point STFT at
10 fps, 55 Hz to 5 kHz, log-compressed magnitudes folded onto 12 pitch classes after taking out
the song's tuning (circular mean of the spectral peaks' cents); a separate bass chroma from the
bass stem (40-250 Hz) adds weight to the chord root. Chords: 24 major / minor triads plus "N" (no
chord) scored by cosine similarity with the chroma, decoded with a sticky HMM (stay 0.94 per
frame); segments shorter than 0.5 s are merged into their neighbours. Key: Krumhansl-Kessler
profiles against 4 s chroma histograms, decoded with a very sticky HMM (stay 0.995), so only real
modulations change it. Output chords [[start, end, "F#m"]], keys [[start, end, "F#m"]], the
song's main key, and the tuning in cents.
"""

import numpy as np

from analyze import common

NAME, VERSION = "chords", 1
FPS, N_FFT = 10, 8192
HOP = common.SR // FPS
NAMES = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"]
MAJOR_KK = np.array([6.35, 2.23, 3.48, 2.33, 4.38, 4.09, 2.52, 5.19, 2.39, 3.66, 2.29, 2.88])
MINOR_KK = np.array([6.33, 2.68, 3.52, 5.38, 2.60, 3.53, 2.54, 4.75, 3.98, 2.69, 3.34, 3.17])


def _chroma(x, freqs, lo, hi, tuning):
    X = np.abs(common.stft(x, N_FFT, HOP))
    band = (freqs >= lo) & (freqs <= hi)
    midi = 69 + 12 * np.log2(freqs[band] / 440) - tuning / 100
    pc = np.round(midi).astype(int) % 12
    w = np.log1p(100 * X[:, band] / (X[:, band].max() + 1e-12))
    C = np.zeros((len(X), 12))
    for k in range(12):
        C[:, k] = w[:, pc == k].sum(1)
    return C, X


def _tuning(X, freqs):
    band = (freqs >= 80) & (freqs <= 2000)
    M = X[:, band]
    f = freqs[band]
    peak = (M[:, 1:-1] > M[:, :-2]) & (M[:, 1:-1] > M[:, 2:]) & (M[:, 1:-1] > M.max(1, keepdims=True) * 0.1)
    rows, cols = np.nonzero(peak)
    if not len(cols):
        return 0.0
    cents = (1200 * np.log2(f[cols + 1] / 440)) % 100
    w = M[rows, cols + 1]
    ang = 2 * np.pi * cents / 100
    return float(np.angle((w * np.exp(1j * ang)).sum()) / (2 * np.pi) * 100)


def _viterbi(logp, stay):
    n, k = logp.shape
    ls, lm = np.log(stay), np.log((1 - stay) / (k - 1))
    score, back = logp[0].copy(), np.zeros((n, k), int)
    for t in range(1, n):
        best = int(np.argmax(score))
        stay_s = score + ls
        move_s = score[best] + lm
        back[t] = np.where(stay_s >= move_s, np.arange(k), best)
        score = np.maximum(stay_s, move_s) + logp[t]
    path = np.zeros(n, int)
    path[-1] = int(np.argmax(score))
    for t in range(n - 1, 0, -1):
        path[t - 1] = back[t, path[t]]
    return path


def _segments(path, labels, fps, min_len):
    segs = []
    for i, p in enumerate(path):
        if segs and segs[-1][2] == p:
            segs[-1][1] = i + 1
        else:
            segs.append([i, i + 1, p])
    changed = True
    while changed and len(segs) > 1:  # fold short segments into the longer neighbour
        changed = False
        for j, s in enumerate(segs):
            if s[1] - s[0] < min_len:
                nb = j - 1 if j > 0 and (j + 1 >= len(segs) or segs[j - 1][1] - segs[j - 1][0] >= segs[j + 1][1] - segs[j + 1][0]) else j + 1
                segs[nb][0], segs[nb][1] = min(segs[nb][0], s[0]), max(segs[nb][1], s[1])
                del segs[j]
                changed = True
                break
    merged = []
    for s in segs:
        if merged and merged[-1][2] == s[2]:
            merged[-1][1] = s[1]
        else:
            merged.append(list(s))
    return [[round(a / fps, 2), round(b / fps, 2), labels[p]] for a, b, p in merged]


def analyse(song, state=None):
    S = common.stems(song)
    freqs = np.fft.rfftfreq(N_FFT, 1 / common.SR)
    harm = S["bass"] + S["other"]
    X0 = np.abs(common.stft(harm[: common.SR * 120], N_FFT, HOP))
    tuning = _tuning(X0, freqs)
    C, X = _chroma(harm, freqs, 55, 5000, tuning)
    B, _ = _chroma(S["bass"], freqs, 40, 250, tuning)
    energy = X.sum(1)
    active = common.db(energy ** 2) > common.db(energy.max() ** 2) - 40
    Cn = C / (np.linalg.norm(C, axis=1, keepdims=True) + 1e-9)
    Bn = B / (B.sum(1, keepdims=True) + 1e-9)
    # templates: root 1, third 0.8, fifth 1
    labels, T = [], []
    for root in range(12):
        for name, third in (("", 4), ("m", 3)):
            t = np.zeros(12)
            t[root], t[(root + third) % 12], t[(root + 7) % 12] = 1, 0.8, 1
            labels.append(NAMES[root] + name)
            T.append(t / np.linalg.norm(t))
    T = np.array(T)
    score = Cn @ T.T + 0.35 * Bn[:, [i // 2 for i in range(24)]]
    no_chord = np.where(active, 0.25, 2.0)[:, None]
    logp = 12 * np.concatenate([score, no_chord], 1)
    path = _viterbi(logp - logp.max(1, keepdims=True), 0.94)
    chords = _segments(path, labels + ["N"], FPS, int(0.5 * FPS))
    # key over 4 s windows (2 s hop)
    win, hop = 4 * FPS, 2 * FPS
    keys_l, prof = [], []
    for root in range(12):
        for name, p in (("", MAJOR_KK), ("m", MINOR_KK)):
            keys_l.append(NAMES[root] + name)
            prof.append(np.roll((p - p.mean()) / p.std(), root))
    prof = np.array(prof)
    H = np.array([C[a: a + win].sum(0) for a in range(0, max(1, len(C) - win + 1), hop)])
    Hn = (H - H.mean(1, keepdims=True)) / (H.std(1, keepdims=True) + 1e-9)
    corr = Hn @ prof.T / 12
    kpath = _viterbi(8 * corr, 0.995)
    keys = _segments(kpath, keys_l, FPS / hop, 4)
    keys = [[a, b + win / FPS / 2 if i == len(keys) - 1 else b, k] for i, (a, b, k) in enumerate(keys)]
    dur = {}
    for a, b, k in keys:
        dur[k] = dur.get(k, 0) + b - a
    return {"tuning_cents": round(tuning, 1), "chords": chords, "keys": keys, "key": max(dur, key=dur.get) if dur else None,
            "summary": {"chords": len(chords), "chord_changes_per_min": round(len(chords) / max(1e-3, len(C) / FPS / 60), 1), "keys": len(keys)}}
