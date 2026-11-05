"""Build-ups and their payoffs, by signal processing on the stems:

  risers   noise sweeps / uplifters / filter sweeps: stretches of 2 s or more where the brightness
           (spectral centroid, log Hz) of the "other" stem or of the drums stem's top end (white
           noise often lands there) keeps rising while its level does not fall; strength from the
           octaves climbed, the level gained and how noisy it is;
  rolls    snare rolls: onsets in the drums stem's snare band (150 Hz - 2 kHz, below the hats) at
           6 per second or more for 1 s or more, reaching 8 per second or speeding up by 2, with
           the top rate (a 16th roll at 128 BPM is 8.5 per second);
  impacts  big broadband hits: the mix jumping 10 dB or more in both the low (< 200 Hz) and the
           high (> 4 kHz) band at once, with the tail it leaves (s);
  gaps     silences: the mix falling 18 dB or more below its level of the 2 s before, for 60 ms
           to 2 s (the pull-up before a drop, stops);
  density  drum onsets per second at 10 fps.
The compiler snaps these onto the beat grid and ties them to the drops.
"""

import numpy as np

from analyze import common

NAME, VERSION = "risers", 3
FPS = 20


def _sweep(P, freqs, fps):
    e = P.sum(1)
    level = common.db(e)
    cen = np.log2(np.maximum((P * freqs).sum(1) / (e + 1e-12), 30))
    band = (freqs >= 100) & (freqs <= 16000)
    Pb = P[:, band] + 1e-12
    flat = np.exp(np.log(Pb).mean(1)) / Pb.mean(1)
    return common.smooth(level, fps), common.smooth(cen, fps), common.smooth(flat, fps // 2)


def _slope(x, w):
    """Least-squares slope per frame over a centred window of w frames (units per frame)."""
    t = np.arange(w) - (w - 1) / 2
    k = t / (t ** 2).sum()
    return np.convolve(np.pad(x, (w // 2, w - 1 - w // 2), mode="edge"), k[::-1], mode="valid")


def analyse(song, state=None):
    S = common.stems(song)
    hop, n_fft = common.SR // FPS, 4096
    freqs = np.fft.rfftfreq(n_fft, 1 / common.SR)
    risers = []
    for src in ("other", "drums_top"):
        x = S["other"] if src == "other" else S["drums"]
        P = np.abs(common.stft(x, n_fft, hop)) ** 2
        if src == "drums_top":
            P = P * (freqs >= 3000)
        level, cen, flat = _sweep(P, freqs, FPS)
        sc, sl = _slope(cen, 2 * FPS) * FPS, _slope(level, 2 * FPS) * FPS  # octaves / s, dB / s
        active = level > level.max() - 40
        rising = (sc > 0.06) & (sl > -0.3) & active
        for a, b in common.runs(rising, 2 * FPS, max_gap=FPS // 2):
            d_oct = float(cen[b - 1] - cen[a])
            d_db = float(level[b - 1] - level[a])
            noisy = float(flat[a:b].mean())
            strength = d_oct * (0.6 + 2 * noisy) + max(d_db, 0) / 12
            if d_oct >= 0.7 and strength >= 0.8:
                risers.append([round(a / FPS, 2), round(b / FPS, 2), round(strength, 2), src])
    risers.sort()
    merged = []  # the same sweep found in both sources: keep the stronger, span the union
    for r in risers:
        if merged and r[0] < merged[-1][1] - 1.0:
            m = merged[-1]
            merged[-1] = [m[0], max(m[1], r[1]), max(m[2], r[2]), m[3] if m[2] >= r[2] else r[3]]
        else:
            merged.append(r)
    # 100 fps bands of the mix and the drums
    hop1 = common.SR // 100
    M = np.abs(common.stft(S["mix"], 2048, hop1)) ** 2
    f1 = np.fft.rfftfreq(2048, 1 / common.SR)
    mix_db = common.db(M.sum(1))
    low_db, high_db = common.db(M[:, f1 < 200].sum(1)), common.db(M[:, f1 > 4000].sum(1))
    lag = lambda x, k: np.concatenate([[x[0]] * k, x[:-k]])
    from scipy.ndimage import maximum_filter1d, percentile_filter
    # an impact rises above the loudest moment of the half second before it in both bands (a kick in
    # a running groove doesn't: the kick before it was as loud), and leaves a tail
    prev_max = lambda x: lag(maximum_filter1d(x, 50, origin=24), 3)
    rise_low, rise_high = low_db - prev_max(low_db), high_db - prev_max(high_db)
    loud = mix_db > mix_db.max() - 30
    score = np.minimum(rise_low, rise_high) * loud
    impacts = []
    for i in common.peaks(score, 8.0, 100):
        tail_level = mix_db[i: i + 300]
        tail = int(np.argmax(tail_level < mix_db[i] - 20)) if (tail_level < mix_db[i] - 20).any() else len(tail_level)
        if tail >= 30:
            impacts.append([round(i / 100, 2), round(float(score[i]), 1), round(tail / 100, 2)])
    # gaps: the mix 18 dB below the quiet end (20th percentile) of the 2 s before, i.e. a hole in
    # sustained sound, not the space between two kicks
    ref = lag(percentile_filter(mix_db, 20, size=200, origin=99), 2)
    quiet = (mix_db < ref - 18) & (ref > mix_db.max() - 30)
    gaps = [[round(a / 100, 2), round(b / 100, 2), round(float(ref[a] - mix_db[a:b].mean()), 1)] for a, b in common.runs(quiet, 8) if b - a <= 200]
    # drum onsets: flux of the drums stem, peaks at least 40 ms apart; the density counts all of
    # them, rolls only the snare band (150 Hz - 3 kHz), so steady 16th hi-hats aren't a roll
    D = np.abs(common.stft(S["drums"], 2048, hop1)) ** 2
    ddb = common.db(D, -80)
    dlev = common.db(D.sum(1))

    def onset_rate(cols):
        flux = np.maximum(0, ddb[:, cols] - lag(ddb[:, cols], 2)).mean(1)
        lev = common.db(D[:, cols].sum(1))
        on = np.zeros(len(flux))
        on[[i for i in common.peaks(flux, max(2.0, float(np.percentile(flux, 90)) * 0.6), 4) if lev[i] > lev.max() - 35]] = 1
        return common.smooth(on, 100) * 100  # onsets per second, over 1 s

    rate = onset_rate(slice(None))
    snare = onset_rate((f1 >= 150) & (f1 <= 2000))
    rolls = []
    for a, b in common.runs(snare >= 6, 100, 20):
        top, speedup = float(snare[a:b].max()), float(snare[max(a, b - 50):b].mean() - snare[a:a + 50].mean())
        if top >= 8 or speedup >= 2:
            rolls.append([round(a / 100, 2), round(b / 100, 2), round(top, 1)])
    density = rate[::10]
    return {"risers": merged, "impacts": impacts, "gaps": gaps, "rolls": rolls,
            "density": {"fps": 10, "track": common.track(density, 0, 16)},
            "summary": {"risers": len(merged), "impacts": len(impacts), "gaps": len(gaps), "rolls": len(rolls)}}
