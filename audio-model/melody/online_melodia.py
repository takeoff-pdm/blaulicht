"""An online Melodia: Melodia's salience function per frame, then a fixed-lag Viterbi tracker.

Melodia (Salamon & Gomez 2012) is offline in three places: contours are tracked forwards and
backwards in time, contour salience is thresholded against statistics of the whole track, and
octave errors are fixed against a 5 s moving average of the melody pitch. This keeps its front
end, which is causal per frame (equal-loudness filter, 2048-sample frame, spectral peaks,
harmonic-summation salience over 10-cent bins from 55 Hz, Essentia's implementation), and
replaces the rest with
  * a Viterbi decoder over 300 pitch states (20-cent bins, the student's grid) plus "no melody",
    transitions: continue within +-5 bins per frame at a cost per bin, jump anywhere at a fixed
    cost, switch voicing at a fixed cost;
  * a voiced emission = salience / a causal running mean of the per-frame peak salience (~4 s);
    the unvoiced emission is a constant threshold;
  * a fixed lag: frame t is decided when frame t + lag arrives, by backtracking from the best
    state then. Latency = window/2 + lag frames, like the student's lookahead.

    python online_melodia.py tune songs_train.txt     # grid search vs offline Melodia
"""

import numpy as np

SR, HOP, N_FFT = 44100, 441, 2048
N_PITCH = 300
F_MIN = 55.0


def salience(x: np.ndarray) -> np.ndarray:
    """(frames, 300) salience on the student grid (frame g centred on sample g * 441)."""
    import essentia.standard as es

    x = es.EqualLoudness(sampleRate=SR)(x.astype(np.float32))
    pad = np.zeros(N_FFT // 2, np.float32)
    x = np.concatenate([pad, x, pad])
    win = es.Windowing(type="hann", size=N_FFT, zeroPadding=0)
    spec = es.Spectrum(size=N_FFT)
    peaks = es.SpectralPeaks(sampleRate=SR, magnitudeThreshold=0, maxPeaks=100, minFrequency=40,
                             maxFrequency=5000, orderBy="magnitude")
    sal = es.PitchSalienceFunction(binResolution=10, referenceFrequency=F_MIN, harmonicWeight=0.8,
                                   magnitudeThreshold=40, numberHarmonics=20)
    n = (len(x) - N_FFT) // HOP + 1
    out = np.zeros((n, N_PITCH), np.float32)
    for g in range(n):
        frame = x[g * HOP: g * HOP + N_FFT]
        f, m = peaks(spec(win(frame)))
        if len(f):
            s = np.asarray(sal(f, m))  # 600 bins of 10 cents
            out[g] = s[: 2 * N_PITCH].reshape(N_PITCH, 2).max(1)
    return out


def track(sal: np.ndarray, lag: int, tau=0.35, step_cost=0.08, jump_cost=1.2, voice_cost=1.0,
          decay_s=4.0, max_step=5) -> np.ndarray:
    """Fixed-lag Viterbi over salience frames -> f0 in Hz per frame (0 = unvoiced)."""
    n = len(sal)
    a = 1 - 1 / (decay_s * 100)
    level = None
    delta = np.zeros(N_PITCH + 1)
    back = np.zeros((n, N_PITCH + 1), np.int32)
    out = np.zeros(n, np.int32)
    k = np.arange(N_PITCH)
    shifts = range(-max_step, max_step + 1)
    for t in range(n):
        peak = sal[t].max()
        level = peak if level is None else a * level + (1 - a) * peak
        emit = np.empty(N_PITCH + 1)
        emit[:N_PITCH] = sal[t] / (level + 1e-9)
        emit[N_PITCH] = tau
        if t == 0:
            delta = emit.copy()
            back[t] = np.arange(N_PITCH + 1)
        else:
            v = delta[:N_PITCH]
            best = np.full(N_PITCH, -np.inf)
            arg = np.zeros(N_PITCH, np.int32)
            for s in shifts:  # stay near: from bin k - s
                src = k - s
                ok = (src >= 0) & (src < N_PITCH)
                cand = np.where(ok, v[np.clip(src, 0, N_PITCH - 1)] - step_cost * abs(s), -np.inf)
                better = cand > best
                best = np.where(better, cand, best)
                arg = np.where(better, np.clip(src, 0, N_PITCH - 1), arg)
            j = int(v.argmax())
            jump = v[j] - jump_cost
            from_u = delta[N_PITCH] - voice_cost
            better = jump > best
            best, arg = np.where(better, jump, best), np.where(better, j, arg)
            better = from_u > best
            best, arg = np.where(better, from_u, best), np.where(better, N_PITCH, arg)
            u_stay, u_from_v = delta[N_PITCH], v[j] - voice_cost
            new = np.empty(N_PITCH + 1)
            new[:N_PITCH] = best + emit[:N_PITCH]
            new[N_PITCH] = max(u_stay, u_from_v) + emit[N_PITCH]
            back[t, :N_PITCH] = arg
            back[t, N_PITCH] = N_PITCH if u_stay >= u_from_v else j
            delta = new - new.max()
        if t >= lag:  # decide frame t - lag
            s = int(delta.argmax())
            for u in range(t, t - lag, -1):
                s = back[u, s]
            out[t - lag] = s
    s = int(delta.argmax())  # flush the last lag frames at the end of the stream
    for u in range(n - 1, max(n - lag, 0) - 1, -1):
        out[u] = s
        s = back[u, s]
    return np.where(out < N_PITCH, F_MIN * 2.0 ** (out / 60.0), 0.0).astype(np.float32)


# -- batch: cache salience, tune on training songs, evaluate on held-out sets ---------------------

def _cache(job):
    audio, out = job
    if not out.exists():
        np.save(out, salience(np.load(audio).astype(np.float32)).astype(np.float16))
    return out


def _score(job):
    from melody_common import metrics

    sal_path, lab_path, lag, params = job
    return metrics(np.load(lab_path), track(np.load(sal_path).astype(np.float32), lag, **params))


def main():
    import argparse
    import itertools
    import json
    import os
    from multiprocessing import Pool
    from pathlib import Path

    from melody_common import ROOT, is_test_song, summarize

    ap = argparse.ArgumentParser()
    ap.add_argument("mode", choices=["tune", "eval"])
    ap.add_argument("--lag", type=int, default=30)
    ap.add_argument("--lags", default="0,10,30,100")
    ap.add_argument("--params", default="{}")
    ap.add_argument("--grid", default='{"tau": [0.8, 1.0, 1.2], "jump_cost": [1.2, 2.4], "voice_cost": [0.25, 0.5]}')
    ap.add_argument("--jobs", type=int, default=24)
    ap.add_argument("--tune-songs", type=int, default=24)
    args = ap.parse_args()
    os.nice(10)
    cache = ROOT / "data/salience"
    sets = {"songs_train": [], "songs": [], "musdb_test": [], "musdb_test_vocals": []}
    for lab in sorted((ROOT / "data/labels/songs").glob("*.npy")):
        sets["songs" if is_test_song(lab.stem) else "songs_train"].append((ROOT / "data/audio/songs" / lab.name, lab))
    for lab in sorted((ROOT / "data/labels/musdb_test").glob("*.npy")):
        audio = ROOT / "data/audio/musdb_test" / lab.name
        sets["musdb_test"].append((audio, lab))
        sets["musdb_test_vocals"].append((audio, ROOT / "data/labels/musdb_test_vocals" / lab.name))
    sets["songs_train"] = sets["songs_train"][: args.tune_songs]
    use = ["songs_train"] if args.mode == "tune" else ["songs", "musdb_test", "musdb_test_vocals"]
    cache.mkdir(parents=True, exist_ok=True)
    with Pool(args.jobs) as pool:
        jobs = {(a, cache / a.name) for s in use for a, _ in sets[s]}
        list(pool.imap_unordered(_cache, sorted(jobs)))
        if args.mode == "tune":
            grid = json.loads(args.grid)
            configs = [dict(zip(grid, vals)) for vals in itertools.product(*grid.values())]
            songs = sets["songs_train"]
            flat = pool.map(_score, [(cache / a.name, l, args.lag, p) for p in configs for a, l in songs])
            results = []
            for i, p in enumerate(configs):
                r = summarize(flat[i * len(songs): (i + 1) * len(songs)])
                results.append((r["OA"], p, r))
                print(json.dumps({"params": p, **{k: round(v, 4) for k, v in r.items()}}), flush=True)
            best = max(results, key=lambda r: r[0])
            print("best", json.dumps(best[1]), json.dumps(best[2]), flush=True)
        else:
            p = json.loads(args.params)
            for lag in map(int, args.lags.split(",")):
                rec = {"lag": lag}
                for s in use:
                    ms = pool.map(_score, [(cache / a.name, l, lag, p) for a, l in sets[s] if l.exists()])
                    rec.update({f"{s}/{k}": round(v, 4) for k, v in summarize(ms).items()})
                print(json.dumps(rec), flush=True)


if __name__ == "__main__":
    main()
