"""Align the Rekordbox beat grid to the drum hits in the analysed audio.

Rekordbox's grid is off against the audio the viewer plays: in most songs the kick lands about
29 ms before the grid beat (all 249 songs are mp3; Rekordbox apparently does not trim the mp3
encoder + decoder delay of 1105 samples = 25 ms, ffmpeg and the browser do), a few songs are off
by 40-60 ms, and live-played songs drift against the constant-tempo grid by up to 100 ms. Grid
driven animations then lag the music while the hit-driven pulses don't.

Per beat, the offset to the nearest strong drum hit (within a fifth of a beat, at most 90 ms) is
collected; over a window of +-8 beats the median offset is used where the hits agree (at least 6,
interquartile range at most 35 ms), interpolated in between, smoothed over +-4 beats and clamped
to a fifth of a beat; up to three passes, so drifting grids are followed further than the search
tolerance. Songs whose hits never agree with the grid (syncopated kicks) keep it. On all 249
songs the median kick-to-beat offset goes from -29 ms to 0 ms (no song left beyond 15 ms), and
the spread (IQR) of a drifting live song like Mr. Brightside from 80 ms to 11 ms.
Standard library only, like the rest of the server.
"""

import base64
import bisect
import statistics

WINDOW = 8  # beats on each side for the local offset
MIN_HITS = 6
MAX_IQR = 0.035
SMOOTH = 4


def drum_hits(e: bytes, lo: float, hi: float) -> list[tuple[float, float]]:
    """Onsets in the drums stem (uint8 dB track at 100 fps), as in club.js drumHits."""
    rise_min, out, last = 5 * 255 / 60, [], -10**9
    for f in range(2, len(e) - 2):
        rise = e[f] - e[f - 2]
        if rise < rise_min or rise < e[f + 1] - e[f - 1] or f - last < 6:
            continue
        amp = max(0.0, min(1.0, (max(e[f], e[f + 1], e[f + 2]) - lo) / max(1.0, hi - lo)))
        if amp >= 0.15:
            out.append(((f - 1) / 100, amp))
            last = f
    return out


def _iqr(xs: list[float]) -> float:
    q = statistics.quantiles(xs, n=4)
    return q[2] - q[0]


def align(doc: dict, beats: list[float], passes: int = 3) -> tuple[list[float], dict]:
    """Corrected beat times and a summary (median and range of the correction, in ms). Repeated
    passes catch up with grids that drift further than the search tolerance."""
    e = base64.b64decode(doc["energy"]["drums"])[: doc["frames"]]
    srt = sorted(e) or [0]
    lo, hi = srt[int(len(srt) * 0.2)], srt[int(len(srt) * 0.995)]
    times = [t for t, a in drum_hits(e, lo, hi) if a >= 0.5]
    new = beats
    for _ in range(passes):
        nxt = _align_once(times, new)
        moved = max(abs(a - b) for a, b in zip(nxt, new)) if new else 0
        new = nxt
        if moved < 0.002:
            break
    shifts = [o - b for o, b in zip(new, beats)]
    if not shifts:
        return beats, {"shift_ms": 0, "min_ms": 0, "max_ms": 0}
    return new, {"shift_ms": round(1000 * statistics.median(shifts)), "min_ms": round(1000 * min(shifts)),
                 "max_ms": round(1000 * max(shifts))}


def _align_once(times: list[float], beats: list[float]) -> list[float]:
    if len(beats) < 2 * WINDOW:
        return beats
    n = len(beats)
    res: list[float | None] = [None] * n
    for i, b in enumerate(beats):
        period = beats[min(i + 1, n - 1)] - beats[max(i - 1, 0)]
        tol = min(0.09, 0.2 * period / 2)
        j = bisect.bisect_left(times, b)
        cand = [times[k] - b for k in (j - 1, j) if 0 <= k < len(times)]
        if cand:
            d = min(cand, key=abs)
            if abs(d) <= tol:
                res[i] = d
    local: list[float | None] = [None] * n
    for i in range(n):
        xs = [r for r in res[max(0, i - WINDOW): i + WINDOW + 1] if r is not None]
        if len(xs) >= MIN_HITS and _iqr(xs) <= MAX_IQR:
            local[i] = statistics.median(xs)
    known = [i for i in range(n) if local[i] is not None]
    if not known:
        return beats
    corr = [0.0] * n
    for i in range(n):  # interpolate between known beats, hold at the ends
        k = bisect.bisect_left(known, i)
        if k < len(known) and known[k] == i:
            corr[i] = local[i]
        elif k == 0:
            corr[i] = local[known[0]]
        elif k == len(known):
            corr[i] = local[known[-1]]
        else:
            a, b = known[k - 1], known[k]
            corr[i] = local[a] + (local[b] - local[a]) * (i - a) / (b - a)
    smooth = [sum(corr[max(0, i - SMOOTH): i + SMOOTH + 1]) / len(corr[max(0, i - SMOOTH): i + SMOOTH + 1]) for i in range(n)]
    out = []
    for i, b in enumerate(beats):
        period = beats[min(i + 1, n - 1)] - beats[max(i - 1, 0)]
        c = max(-0.1 * period, min(0.1 * period, smooth[i]))  # a fifth of a beat
        out.append(round(b + c, 4))
    for i in range(1, n):  # keep it strictly increasing
        if out[i] <= out[i - 1]:
            out[i] = out[i - 1] + 1e-3
    return out


def warp(t: float | None, old: list[float], new: list[float]) -> float | None:
    """Move a time given on the old grid (a phrase start, say) onto the new one."""
    if t is None or len(old) < 2:
        return t
    i = min(max(bisect.bisect_right(old, t) - 1, 0), len(old) - 2)
    a, b = old[i], old[i + 1]
    return round(new[i] + (t - a) * (new[i + 1] - new[i]) / (b - a), 4)
