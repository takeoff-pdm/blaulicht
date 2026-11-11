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

align() only follows a grid that is already close. Rekordbox's own grid can be further off: a 16th
early for the whole song (Lean On), half a beat off after slipping somewhere (Firestone), or the
wrong tempo (Strobe: 88 against 128 BPM). correct() compares the aligned grid with the two beat
trackers (Beat This!, All-In-One) and ADTOF's kicks. A song is a candidate if both trackers miss the
first 16th of the beat overall, or for 16 bars in a row they agree with each other but not with the
grid, without the kicks backing the grid. It only gets a new grid if that puts at least 10 points
more of the trackers' beats on it: a best path through the trackers and kicks at the song's best
constant tempo, then aligned like any other grid. Bars are numbered by a vote of both trackers'
downbeats and where the low end jumps. A song with no Rekordbox grid gets one the same way. Rekordbox's bar numbers
stay on all other songs, even where the trackers disagree with them, because nothing here says which is right:
checks.py flags those for a person to decide, and `song.py fix` applies the answer.
Standard library only, like the rest of the server.
"""

import base64
import bisect
import math
import statistics

WINDOW = 8  # beats on each side for the local offset
MIN_HITS = 6
MAX_IQR = 0.035
SMOOTH = 4
STRETCH = 32  # beats: correct() compares the grid and the trackers per stretch
MIN_RUN = 2  # stretches in a row against the grid: 16 bars
MIN_GAIN = 0.10  # more of the trackers' beats on a new grid before it replaces the old one


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


def _times(items) -> list[float]:
    return sorted(x[0] if isinstance(x, list) else x for x in items or [])


def _distance(times: list[float], t: float) -> float:
    i = bisect.bisect_left(times, t)
    return min((abs(times[j] - t) for j in (i - 1, i) if 0 <= j < len(times)), default=math.inf)


def _on_first_16th(times: list[float], beats: list[float]) -> float:
    """Of the times on a 16th of the grid (within 40 ms), the share on the beat itself."""
    slots = [0, 0, 0, 0]
    for t in times:
        i = bisect.bisect_right(beats, t) - 1
        if 0 <= i < len(beats) - 1:
            step = beats[i + 1] - beats[i]
            x = (t - beats[i]) / step * 4
            if abs(x - round(x)) * step / 4 <= 0.04:
                slots[round(x) % 4] += 1
    return slots[0] / max(1, sum(slots))


def _longest_run(beats: list[float], tracker: list[float], allinone: list[float], kicks: list[float]) -> int:
    """The most stretches in a row where both trackers agree with each other and not with the grid. A stretch
    the trackers mostly skip doesn't end a run, and one whose kicks are on the grid does."""
    best = run = 0
    for a in range(0, len(beats) - STRETCH, STRETCH):
        lo, hi = beats[a], beats[a + STRETCH]
        tol = 0.12 * (hi - lo) / STRETCH
        t, b, k = [[x for x in s[bisect.bisect_left(s, lo):bisect.bisect_left(s, hi)]] for s in (tracker, allinone, kicks)]
        if len(t) < STRETCH / 2 or len(b) < STRETCH / 2:
            continue
        on = lambda xs: sum(_distance(beats, x) <= tol for x in xs) / len(xs)
        agree = sum(_distance(tracker, x) <= 0.06 for x in b) / len(b)
        against = on(t) < 0.5 and on(b) < 0.5 and agree >= 0.6 and not (len(k) >= 8 and on(k) >= 0.7)
        run = run + 1 if against else 0
        best = max(best, run)
    return best


def _best_period(times: list[float], start: float) -> float:
    """The beat period, within 3% of start, that gathers the most times around one phase."""
    best = (-1, start)
    for k in range(-60, 61):
        period, bins = start * (1 + k * 0.0005), [0] * 60
        for t in times:
            bins[int(t / period % 1 * 60) % 60] += 1
        score = max(sum(bins[(i + j) % 60] for j in range(-2, 3)) for i in range(60))
        if score > best[0]:
            best = (score, period)
    return best[1]


def _retrack(tracker: list[float], allinone: list[float], kicks: list[float], duration: float) -> list[float]:
    """Beat times: the best path through the evidence at 100 fps, close to one tempo, so stretches without
    evidence just keep going. A straight line through +-8 beats then takes out the frame jitter."""
    gaps = sorted(b - a for a, b in zip(allinone, allinone[1:]))
    period = _best_period(tracker + allinone, gaps[len(gaps) // 2])
    n, frames = int(duration * 100) + 2, period * 100
    salience = [0.0] * n
    for times, weight in ((tracker, 1.0), (allinone, 1.0), (kicks, 0.5)):
        for t in times:
            c = round(t * 100)
            for d in range(-5, 6):
                if 0 <= c + d < n:
                    salience[c + d] += weight * math.exp(-((d / 2.5) ** 2))
    lo, hi = int(frames * 0.9), int(frames * 1.1) + 1
    penalty = {d: 400 * math.log(d / frames) ** 2 for d in range(lo, hi + 1)}
    score, back = [0.0] * n, [-1] * n
    for f in range(n):
        for d in range(lo, min(hi, f) + 1):
            s = score[f - d] - penalty[d]
            if back[f] < 0 or s > score[f]:
                score[f], back[f] = s, f - d
        score[f] += salience[f]
    f = max(range(max(0, n - hi), n), key=score.__getitem__)
    path = []
    while f >= 0:
        path.append(f / 100)
        f = back[f]
    path.reverse()
    while path[0] - period > 0:
        path.insert(0, path[0] - period)
    while path[-1] + period < duration:
        path.append(path[-1] + period)
    out = []
    for i in range(len(path)):
        a, b = max(0, i - 8), min(len(path), i + 9)
        mx, my = (a + b - 1) / 2, sum(path[a:b]) / (b - a)
        slope = sum((x - mx) * (path[x] - my) for x in range(a, b)) / sum((x - mx) ** 2 for x in range(a, b))
        out.append(round(my + slope * (i - mx), 4))
    return out


def _bar_votes(doc: dict, beats: list[float], tracker_downs: list[float], allinone_downs: list[float]) -> dict:
    """For each beat index modulo 4, the votes that it starts a bar: both trackers' downbeats and the 12 strongest jumps of the
    bass and drums (the level over the next 4 beats against the last 4). `total` adds their shares, the jumps counting double."""
    tol = 0.2 * (beats[-1] - beats[0]) / (len(beats) - 1)

    def of_downbeats(downs: list[float]) -> list[int]:
        votes = [0] * 4
        for t in downs:
            i = bisect.bisect_left(beats, t)
            near = [j for j in (i - 1, i) if 0 <= j < len(beats)]
            j = min(near, key=lambda j: abs(beats[j] - t))
            if abs(beats[j] - t) <= tol:
                votes[j % 4] += 1
        return votes

    bass, drums = base64.b64decode(doc["energy"]["bass"]), base64.b64decode(doc["energy"]["drums"])
    frames = min(len(bass), len(drums))
    cum = [0.0] * (frames + 1)
    for f in range(frames):
        cum[f + 1] = cum[f] + bass[f] + drums[f]
    jumps = []
    for i in range(4, len(beats) - 4):
        a, b, c = (min(frames, int(beats[i + d] * 100)) for d in (-4, 0, 4))
        if a < b < c:
            jumps.append(((cum[c] - cum[b]) / (c - b) - (cum[b] - cum[a]) / (b - a), i))
    low, used = [0] * 4, []
    for jump, i in sorted(jumps, reverse=True):
        if jump <= 0 or len(used) == 12:
            break
        if all(abs(i - u) >= 8 for u in used):
            used.append(i)
            low[i % 4] += 1
    tracker, allinone = of_downbeats(tracker_downs), of_downbeats(allinone_downs)
    total = [tracker[k] / max(1, sum(tracker)) + allinone[k] / max(1, sum(allinone)) + 2 * low[k] / max(1, sum(low)) for k in range(4)]
    return {"tracker": tracker, "allinone": allinone, "low": low, "total": total}


def correct(doc: dict, beats: list[float], features: dict, force: bool = False) -> dict | None:
    """A new grid if the aligned one is wrong or missing (see the top of this file): beats, beat_in_bar, bpm, aligned and a
    summary. None if it holds, or if the trackers are missing. force skips the two tests, for a hand correction."""
    bt, ai = features.get("beats") or {}, features.get("allinone") or {}
    tracker, allinone = _times(bt.get("beats")), _times(ai.get("beats"))
    if len(tracker) < 16 or len(allinone) < 16:
        return None
    kicks = _times(((features.get("adtof") or {}).get("hits") or {}).get("kick"))
    if not force and _longest_run(beats, tracker, allinone, kicks) < MIN_RUN and not (_on_first_16th(tracker, beats) < 0.6 and _on_first_16th(allinone, beats) < 0.6):
        return None
    new, shift = align(doc, _retrack(tracker, allinone, kicks, doc["frames"] / 100))
    evidence = tracker + allinone
    fit = lambda grid: sum(_distance(grid, t) <= 0.04 for t in evidence) / len(evidence)
    before, after = fit(beats), fit(new)
    if not force and after - before < MIN_GAIN:
        return None
    votes = _bar_votes(doc, new, _times(bt.get("downbeats")), _times(ai.get("downbeats")))
    phase = max(range(4), key=votes["total"].__getitem__)
    bpm = round(60 * (len(new) - 1) / (new[-1] - new[0]), 2)
    return {"beats": new, "beat_in_bar": [(i - phase) % 4 + 1 for i in range(len(new))], "bpm": bpm, "aligned": shift,
            "summary": {"trackers_on_rekordbox": round(before, 3), "trackers_on_new": round(after, 3)}}


def shift_grid(beats: list[float], sixteenths: int) -> list[float]:
    """A hand correction: the whole grid moved by this many 16ths of the local beat. Later is positive."""
    if len(beats) < 2:
        return beats
    return [round(t + sixteenths / 4 * (beats[min(i + 1, len(beats) - 1)] - beats[max(min(i, len(beats) - 2), 0)]), 4) for i, t in enumerate(beats)]


def rotate_bars(beat_in_bar: list[int], beats: int) -> list[int]:
    """A hand correction: bars start this many beats later."""
    return [(k - 1 - beats) % 4 + 1 for k in beat_in_bar]


def report(doc: dict, beats: list[float], beat_in_bar: list[int], features: dict) -> dict:
    """How the final grid compares with the trackers and kicks, for checks.py. None where an analyzer is missing."""
    bt, ai = features.get("beats") or {}, features.get("allinone") or {}
    tracker, allinone = _times(bt.get("beats")), _times(ai.get("beats"))
    kicks = _times(((features.get("adtof") or {}).get("hits") or {}).get("kick"))
    out = {"trackers_on_grid": None, "kicks_on_beat": None, "bar_start_vote": None}
    if len(beats) < 8:
        return out
    if tracker and allinone:
        evidence = tracker + allinone
        out["trackers_on_grid"] = round(sum(_distance(beats, t) <= 0.04 for t in evidence) / len(evidence), 3)
    if len(kicks) >= 16:
        out["kicks_on_beat"] = round(_on_first_16th(kicks, beats), 3)
    ones = [i % 4 for i, k in enumerate(beat_in_bar[:len(beats)]) if k == 1]
    if ones and bt.get("downbeats") and ai.get("downbeats"):
        current = max(range(4), key=ones.count)
        v = _bar_votes(doc, beats, _times(bt["downbeats"]), _times(ai["downbeats"]))
        strong = lambda votes: (lambda k: k if votes[k] >= 0.6 * max(1, sum(votes)) else None)(max(range(4), key=votes.__getitem__))
        a, b = strong(v["tracker"]), strong(v["allinone"])
        # how many beats later both trackers would start the bar, if they agree with each other and not with the grid
        out["bar_start_vote"] = (a - current) % 4 if a is not None and a == b else 0
    return out


def snap_phrases(phrases: list[dict], beats: list[float], beat_in_bar: list[int]) -> list[dict]:
    """Rekordbox's phrases on a new grid: starts and ends on the nearest bar, fill-ins on the nearest beat. Their beat
    numbers belong to the old grid, so warp() can't move them."""
    bars = [t for t, k in zip(beats, beat_in_bar) if k == 1]

    def nearest(times: list[float], t: float | None) -> float | None:
        if t is None:
            return None
        i = bisect.bisect_left(times, t)
        return min((times[j] for j in (i - 1, i) if 0 <= j < len(times)), key=lambda x: abs(x - t))

    moved = [{**p, "start": nearest(bars, p.get("start")), "end": nearest(bars, p.get("end")), "fill": nearest(beats, p.get("fill"))} for p in phrases]
    return [p for p in moved if p["start"] is not None and p["end"] is not None and p["end"] > p["start"]]
