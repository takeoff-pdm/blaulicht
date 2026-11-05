"""Song structure from self-similarity: where the music changes and which parts repeat.

Per half second: chroma (harmony), a 13-coefficient cepstrum of the mix (timbre) and the five stem
levels (what plays), each standardised. Boundaries: novelty from a 16 s Gaussian checkerboard
kernel along the diagonal of the self-similarity matrix (cosine), peaks at least 8 s apart and
above the median + 1 std. Segments between boundaries are clustered by their mean features
(average-linkage, cosine distance below 0.35 joins) and named A, B, C ... in order of first
appearance, so "the chorus drop came back" is the same letter. Repetition: per frame, how well the
best 8 s-aligned match elsewhere in the song (lag at least 8 s) agrees, from the diagonal-smoothed
matrix. Times are seconds; the compiler snaps boundaries to bar lines. No external model.
"""

import numpy as np
from scipy.fft import dct
from scipy.ndimage import uniform_filter1d

from analyze import common

NAME, VERSION = "structure", 1
FPS, N_FFT = 2, 8192
HOP = common.SR // FPS


def _features(S):
    freqs = np.fft.rfftfreq(N_FFT, 1 / common.SR)
    X = np.abs(common.stft(S["mix"], N_FFT, HOP)) ** 2
    # 40 mel-ish bands (log-spaced 40 Hz .. 16 kHz) -> log -> DCT
    edges = np.geomspace(40, 16000, 41)
    mel = np.stack([X[:, (freqs >= a) & (freqs < b)].sum(1) for a, b in zip(edges, edges[1:])], 1)
    ceps = dct(np.log(mel + 1e-9), axis=1, norm="ortho")[:, 1:14]
    H = np.abs(common.stft(S["bass"] + S["other"] + S["vocals"], N_FFT, HOP))
    band = (freqs >= 55) & (freqs <= 5000)
    pc = np.round(69 + 12 * np.log2(freqs[band] / 440)).astype(int) % 12
    ch = np.stack([np.log1p(H[:, band][:, pc == k]).sum(1) for k in range(12)], 1)
    ch /= np.linalg.norm(ch, axis=1, keepdims=True) + 1e-9
    lv = np.stack([common.db((np.abs(common.stft(S[k], N_FFT, HOP)) ** 2).sum(1)) for k in ("drums", "bass", "other", "vocals", "mix")], 1)
    n = min(len(ceps), len(ch), len(lv))
    z = lambda a: (a[:n] - a[:n].mean(0)) / (a[:n].std(0) + 1e-9)
    return np.concatenate([z(ch) * 1.2, z(ceps), z(lv) * 1.5], 1)


def segment(F, fps):
    """Boundaries, lettered segments, novelty and repetition from frame features F (n x d) at fps."""
    n = len(F)
    Fn = F / (np.linalg.norm(F, axis=1, keepdims=True) + 1e-9)
    SSM = Fn @ Fn.T
    # novelty: checkerboard kernel, 16 s
    L = int(16 * fps) // 2
    g = np.exp(-0.5 * (np.linspace(-2, 2, 2 * L)) ** 2)
    K = np.outer(g, g) * np.outer(np.sign(np.arange(2 * L) - L + 0.5), np.sign(np.arange(2 * L) - L + 0.5))
    P = np.pad(SSM, L, mode="edge")
    nov = np.array([(P[i: i + 2 * L, i: i + 2 * L] * K).sum() for i in range(n)])
    nov = np.maximum(nov, 0) / (nov.max() + 1e-9)
    thr = float(np.median(nov) + nov.std())
    bounds = [i for i in common.peaks(nov, thr, int(8 * fps))]
    edges = [0] + bounds + [n]
    segs = [(a, b) for a, b in zip(edges, edges[1:]) if b > a]
    means = np.array([F[a:b].mean(0) for a, b in segs])
    means /= np.linalg.norm(means, axis=1, keepdims=True) + 1e-9
    # average-linkage clustering by cosine distance
    groups = [[i] for i in range(len(segs))]
    D = 1 - means @ means.T
    while len(groups) > 1:
        best, pair = 1e9, None
        for i in range(len(groups)):
            for j in range(i + 1, len(groups)):
                d = D[np.ix_(groups[i], groups[j])].mean()
                if d < best:
                    best, pair = d, (i, j)
        if best > 0.35:
            break
        i, j = pair
        groups[i] += groups.pop(j)
    lab = {s_: gi for gi, gr in enumerate(groups) for s_ in gr}
    names = {}
    for s_ in range(len(segs)):
        if lab[s_] not in names:
            names[lab[s_]] = chr(65 + len(names)) if len(names) < 26 else f"Z{len(names)}"
    segments = [[round(a / fps, 1), round(b / fps, 1), names[lab[s_]]] for s_, (a, b) in enumerate(segs)]
    # repetition: diagonal-smoothed similarity, best match at lag >= 8 s
    rep = np.zeros(n)
    w = max(1, int(8 * fps))
    for lag in range(w, n):
        d = np.diagonal(SSM, lag)
        sm = uniform_filter1d(d, w, mode="nearest")
        rep[:n - lag] = np.maximum(rep[:n - lag], sm)
        rep[lag:] = np.maximum(rep[lag:], sm)
    return {"fps": fps, "boundaries": [round(b / fps, 1) for b in bounds], "segments": segments,
            "novelty": common.track(nov), "repetition": common.track(np.clip(rep, 0, 1)),
            "summary": {"segments": len(segments), "distinct": len(names)}}


def analyse(song, state=None):
    return segment(_features(common.stems(song)), FPS)
