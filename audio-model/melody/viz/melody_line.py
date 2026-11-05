"""Clean, beat-synced melody and bass lines for the light show, from the offline transcriptions.

The raw notes are honest transcriptions but poor light cues: in the electronic songs 19-27% of
the Melodia notes (mix and lead stem) are shorter than a sixteenth, GAME splits a fifth of its
held vocal notes into same-pitch pieces, and nothing sits on the beat grid. Per source (GAME on
the vocals, Melodia on the "other" stem for the lead, Melodia on the bass stem):

  1. merge same-pitch pieces unless the stem has a new onset at the joint (rise of 4 dB or more);
  2. fold octave jumps: a note 11+ semitones away from the median of its neighbours (within a
     second) moves an octave towards them;
  3. snap onsets and ends to the sixteenth grid of the aligned beat grid when within 40% of a
     sixteenth;
  4. drop blips: shorter than 60% of a sixteenth, out of the key and shorter than an eighth, or
     with the stem nearly silent at the onset; then make the line monophonic;
  5. velocity from the stem's loudness right after the onset (0..1 in the song's range).

The main line takes, bar by bar, the vocal where GAME covers at least 30% of the bar and the lead
otherwise (a change must hold for two bars). The bass line gives the chord root per beat (the
longest sounding bass pitch class, single-beat passing notes removed). Hooks: two-bar blocks of
the main line are compared by their onset grid and scale degrees (Jaccard); blocks that recur
form a hook (A, B, ...), so the lights can give the same music the same look. Standard library.
"""

import array
import base64
import bisect
import math
import statistics

NAMES = {"C": 0, "D": 2, "E": 4, "F": 5, "G": 7, "A": 9, "B": 11}
MAJOR = (0, 2, 4, 5, 7, 9, 11)
STEM = {"game": "vocals", "lead": "other", "bass": "bass"}


def parse_key(name: str | None) -> tuple[int, bool] | None:
    """'F#m' -> (6, True); tonic pitch class and minor."""
    if not name or name[0] not in NAMES:
        return None
    pc, i = NAMES[name[0]], 1
    if len(name) > 1 and name[1] in "#♯b♭":
        pc += 1 if name[1] in "#♯" else -1
        i = 2
    return pc % 12, name[i:i + 1] == "m"


def scale_of(key: tuple[int, bool] | None) -> set[int] | None:
    if key is None:
        return None
    major_tonic = (key[0] + 3) % 12 if key[1] else key[0]  # natural minor = relative major's notes
    return {(major_tonic + s) % 12 for s in MAJOR}


def sixteenths(beats: list[float]) -> list[float]:
    out = []
    for a, b in zip(beats, beats[1:]):
        out += [a + (b - a) * k / 4 for k in range(4)]
    out.append(beats[-1])
    return out


class Stem:
    """A stem's loudness track (uint8 dB at 100 fps), in dB and normalised to the song's range."""

    def __init__(self, b64: str, n: int):
        self.v = base64.b64decode(b64)[:n]
        s = sorted(self.v) or [0]
        self.lo, self.hi = s[int(len(s) * 0.2)], s[int(len(s) * 0.995)]

    def db(self, f: int) -> float:
        return self.v[min(max(f, 0), len(self.v) - 1)] / 255 * 60 - 60

    def level(self, t: float, span: float = 0.06) -> float:
        f0 = int(t * 100)
        peak = max(self.v[min(max(f, 0), len(self.v) - 1)] for f in range(f0, f0 + max(1, int(span * 100))))
        return max(0.0, min(1.0, (peak - self.lo) / max(1, self.hi - self.lo)))

    def onset_at(self, t: float) -> float:
        """dB rise across t: loudest 50 ms after minus quietest 50 ms before."""
        f = int(t * 100)
        return max(self.db(g) for g in range(f, f + 5)) - min(self.db(g) for g in range(f - 5, f))


def contour(doc: dict, name: str) -> array.array | None:
    """A source's pitch contour: MIDI cents per 10 ms frame, 0 where unvoiced."""
    if name not in doc.get("contours", {}):
        return None
    a = array.array("h")
    a.frombytes(base64.b64decode(doc["contours"][name]))
    return a


def tuning(doc: dict) -> tuple[float, float]:
    """The song's tuning against A = 440 Hz in cents (-50..50) and how concentrated the pitch is
    around it (0..1): circular mean of the contours' cents within the semitone. Most songs sit
    within 8 cents; a few are a quarter tone off (L'Amour Toujours -47, Modjo +42), where every
    rounding to a semitone becomes a coin toss between two neighbours."""
    sx = sy = 0.0
    n = 0
    for name in ("melodia", "lead", "bass"):
        for c in contour(doc, name) or ():
            if c:
                th = 2 * math.pi * (c % 100) / 100
                sx += math.cos(th)
                sy += math.sin(th)
                n += 1
    if not n:
        return 0.0, 0.0
    return math.atan2(sy, sx) * 100 / (2 * math.pi), math.hypot(sx, sy) / n


def clean(notes: list, grid: list[float], stem: Stem, scale: set[int] | None,
          cont: array.array | None = None, tau: float = 0.0, detuned: bool = False) -> tuple[list, dict]:
    ns = sorted([float(n[0]), float(n[1]), int(n[2])] for n in notes if n[1] > n[0])
    raw = len(ns)
    # 0. pitch: from the source's contour with the song's tuning taken out, where there is one;
    #    without a contour (GAME) and in a detuned song, an out-of-key note moves to its in-key
    #    neighbour on the side the detuning pushed it from
    repitched = 0
    for n in ns:
        p = n[2]
        if cont is not None:
            vals = sorted(c for c in cont[int(n[0] * 100): max(int(n[0] * 100) + 1, int(n[1] * 100))] if c)
            if len(vals) >= 3:
                p = round((vals[len(vals) // 2] - tau) / 100)
        elif detuned and scale is not None and p % 12 not in scale:
            q = p + (1 if tau < 0 else -1)
            if q % 12 in scale:
                p = q
        repitched += p != n[2]
        n[2] = p
    # the key filter only for a source that mostly fits the key: where it doesn't, the key (or
    # the source) is off and the filter would remove real notes
    if scale is not None:
        tot = sum(n[1] - n[0] for n in ns) or 1
        if sum(n[1] - n[0] for n in ns if n[2] % 12 in scale) / tot < 0.75:
            scale = None
    # 1. merge same-pitch pieces without a new onset in the stem
    merged = []
    for n in ns:
        if merged and n[2] == merged[-1][2] and n[0] - merged[-1][1] < 0.045 and stem.onset_at(n[0]) < 4:
            merged[-1][1] = max(merged[-1][1], n[1])
        else:
            merged.append(n)
    # 2. octave jumps towards the neighbours
    for i, n in enumerate(merged):
        nb = [m[2] for m in merged[max(0, i - 4): i + 5] if m is not n and abs(m[0] - n[0]) < 1.0]
        if len(nb) >= 2:
            med = statistics.median(nb)
            while n[2] - med >= 11:
                n[2] -= 12
            while med - n[2] >= 11:
                n[2] += 12
    # 3. snap to the sixteenth grid
    on_grid = 0
    for n in merged:
        for j in (0, 1):
            k = bisect.bisect_left(grid, n[j])
            cand = [grid[x] for x in (k - 1, k) if 0 <= x < len(grid)]
            if not cand:
                continue
            g = min(cand, key=lambda x: abs(x - n[j]))
            k = bisect.bisect_left(grid, g)
            six = (grid[min(k + 1, len(grid) - 1)] - grid[max(k - 1, 0)]) / 2 or 0.1
            if abs(g - n[j]) <= 0.4 * six:
                n[j] = g
                on_grid += j == 0
    # 4. drop blips, then monophonic
    kept = []
    for n in merged:
        k = bisect.bisect_left(grid, n[0])
        six = (grid[min(k + 1, len(grid) - 1)] - grid[max(k - 1, 0)]) / 2 or 0.1
        dur = n[1] - n[0]
        vel = stem.level(n[0])
        if dur < 0.6 * six or vel < 0.08:
            continue
        if scale is not None and n[2] % 12 not in scale and dur < 2 * six:
            continue
        kept.append([round(n[0], 4), round(n[1], 4), n[2], round(vel, 2)])
    for a, b in zip(kept, kept[1:]):
        if b[0] < a[1]:
            a[1] = b[0]
    kept = [n for n in kept if n[1] > n[0]]
    return kept, {"raw": raw, "kept": len(kept), "on_grid": round(on_grid / max(1, len(merged)), 2),
                  "repitched": repitched, "key_filter": scale is not None}


def main_line(vocal: list, lead: list, downbeats: list[float]) -> list:
    def cover(notes, a, b):
        return sum(max(0.0, min(n[1], b) - max(n[0], a)) for n in notes) / (b - a)

    bars = list(zip(downbeats, downbeats[1:]))
    choice = ["v" if cover(vocal, a, b) >= 0.3 else "l" for a, b in bars]
    for i in range(1, len(choice) - 1):  # a change must hold for two bars
        if choice[i] != choice[i - 1] and choice[i + 1] != choice[i]:
            choice[i] = choice[i - 1]
    out = []
    for src, notes in (("v", vocal), ("l", lead)):
        for n in notes:
            i = bisect.bisect_right(downbeats, n[0]) - 1
            if 0 <= i < len(choice) and choice[i] == src:
                out.append(n + [src])
    out.sort()
    for a, b in zip(out, out[1:]):
        if b[0] < a[1]:
            a[1] = b[0]
    return [n for n in out if n[1] > n[0]]


def roots(bass: list, beats: list[float]) -> list:
    per_beat = []
    for a, b in zip(beats, beats[1:]):
        tally = {}
        for n in bass[max(0, bisect.bisect_left([x[1] for x in bass], a)):]:
            if n[0] >= b:
                break
            ov = min(n[1], b) - max(n[0], a)
            if ov > 0:
                tally[n[2] % 12] = tally.get(n[2] % 12, 0) + ov
        per_beat.append(max(tally, key=tally.get) if tally else None)
    for i in range(1, len(per_beat) - 1):  # A B A -> A A A: passing notes
        if per_beat[i - 1] is not None and per_beat[i - 1] == per_beat[i + 1] != per_beat[i]:
            per_beat[i] = per_beat[i - 1]
    for i in range(1, len(per_beat)):  # a gap keeps the last root
        if per_beat[i] is None:
            per_beat[i] = per_beat[i - 1]
    segs = []
    for i, pc in enumerate(per_beat):
        if pc is None:
            continue
        if segs and segs[-1][2] == pc:
            segs[-1][1] = beats[i + 1]
        else:
            segs.append([beats[i], beats[i + 1], pc])
    return segs


def hooks(line: list, downbeats: list[float], tonic: int) -> dict:
    blocks = downbeats[::2]
    sigs = []
    for a, b in zip(blocks, blocks[1:]):
        six = (b - a) / 32
        ns = [n for n in line if a <= n[0] < b]
        rhythm = {round((n[0] - a) / six) for n in ns}
        sigs.append((rhythm, {(round((n[0] - a) / six), (n[2] - tonic) % 12) for n in ns}) if len(ns) >= 3 else None)
    jac = lambda x, y: len(x & y) / max(1, len(x | y))
    reps, ids = [], []
    for s in sigs:
        best, bi = 0.0, -1
        if s is not None:
            for ci, r in enumerate(reps):
                sim = 0.4 * jac(s[0], r[0]) + 0.6 * jac(s[1], r[1])
                if sim > best:
                    best, bi = sim, ci
        if s is None:
            ids.append(-1)
        elif best >= 0.55:
            ids.append(bi)
        else:
            reps.append(s)
            ids.append(len(reps) - 1)
    counts = {i: ids.count(i) for i in set(ids) if i >= 0}
    # only recurring blocks are hooks; name them A, B, ... in order of first appearance
    names, out = {}, []
    for i in ids:
        if i >= 0 and counts[i] >= 2:
            names.setdefault(i, len(names))
            out.append(names[i])
        else:
            out.append(-1)
    return {"blocks": blocks[: len(out)], "ids": out, "count": len(names)}


def analyse(doc: dict, beats: list[float], beat_in_bar: list[int], key_name: str | None) -> dict:
    """Adds doc['notes']['line'] and ['bassline'] ([start, end, pitch, velocity(, src)]) and
    returns {'roots': [[start, end, pc]], 'hooks': {...}, 'stats': {...}}."""
    if len(beats) < 8:
        return {"roots": [], "hooks": {"blocks": [], "ids": [], "count": 0}, "stats": {}}
    n = doc["frames"]
    key = parse_key(key_name)
    scale = scale_of(key)
    grid = sixteenths(beats)
    stems = {k: Stem(doc["energy"][v], n) for k, v in STEM.items()}
    tau, conc = tuning(doc)
    detuned = abs(tau) > 20 and conc >= 0.3
    cleaned, stats = {}, {"tuning_cents": round(tau, 1)}
    for src in ("game", "lead", "bass"):
        cleaned[src], stats[src] = clean(doc["notes"].get(src) or [], grid, stems[src], scale,
                                         contour(doc, src), tau, detuned)
    downbeats = [t for t, k in zip(beats, beat_in_bar) if k == 1]
    line = main_line(cleaned["game"], cleaned["lead"], downbeats)
    doc["notes"]["line"], doc["notes"]["bassline"] = line, cleaned["bass"]
    tonic = key[0] if key else 0
    hk = hooks(line, downbeats, tonic)
    stats["line"] = {"notes": len(line), "vocal_share": round(sum(x[4] == "v" for x in line) / max(1, len(line)), 2), "hooks": hk["count"]}
    return {"roots": roots(cleaned["bass"], beats), "hooks": hk, "stats": stats}
