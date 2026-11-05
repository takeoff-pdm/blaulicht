"""Bar sections and drops for the light show, from the stem loudness and the beat grid.

Drops use the hindsight rule of audio-model/drops.py (kick and bass come back after a
breakdown), with the drums + bass stem loudness as the per-bar low-end energy instead of the mel
low band. Bars are labelled with the kinds of crates/setlist (Intro, Groove, Breakdown, Build,
Drop, Outro). Standard library only, so serve.py stays dependency free.
"""

import base64

# Thresholds from audio-model/drops.py (per-bar energy relative to the track's 90th percentile).
FULL_LEVEL = 0.7
MIN_RISE = 0.4
MAX_LEVEL_BEFORE = 0.6
MIN_JUMP = 0.25
BARS_AFTER = 4
MIN_BARS_APART = 4
BUILD_BARS = 4  # at most this many quiet bars before a drop are its build
QUIET = 0.45  # below this a bar has no real kick/bass (breakdown)
DROP_HOLD = FULL_LEVEL - 0.25  # a drop section lasts while bars stay above this


def _power(b64: str, fps_frames: int) -> list[float]:
    raw = base64.b64decode(b64)
    # uint8 maps -60..0 dBFS linearly to 0..255 (build_viz_data.py)
    return [10 ** (((v / 255) * 60 - 60) / 10) for v in raw[:fps_frames]]


def _percentile(xs: list[float], q: float) -> float:
    s = sorted(xs)
    return s[min(len(s) - 1, max(0, int(round(q / 100 * (len(s) - 1)))))]


def analyse(doc: dict, beats: list[float], beat_in_bar: list[int]) -> dict:
    """doc: the song's analysis (energy at 100 fps). Returns bar energies, drops, sections."""
    fps, n = doc["fps"], doc["frames"]
    drums, bass = _power(doc["energy"]["drums"], n), _power(doc["energy"]["bass"], n)
    low = [d + b for d, b in zip(drums, bass)]
    downbeats = [t for t, k in zip(beats, beat_in_bar) if k == 1]
    if len(downbeats) < 2 * BARS_AFTER + 1:
        return {"bars": [], "drops": [], "sections": []}
    bars = []
    for a, b in zip(downbeats[:-1], downbeats[1:]):
        fa, fb = int(a * fps), max(int(a * fps) + 1, int(b * fps))
        seg = low[max(0, fa): min(n, fb)]
        bars.append(sum(seg) / len(seg) if seg else 0.0)
    ref = max(_percentile(bars, 90), 1e-12)
    e = [x / ref for x in bars]

    found: list[int] = []
    for b in range(4, len(e) - BARS_AFTER + 1):
        before, after = e[b - 4: b], e[b: b + BARS_AFTER]
        if (sum(after) / 4 > FULL_LEVEL and min(after) > FULL_LEVEL - 0.2
                and sum(after) / 4 - sum(before) / 4 > MIN_RISE and e[b - 1] < MAX_LEVEL_BEFORE
                and e[b] - e[b - 1] > MIN_JUMP and (not found or b - found[-1] >= MIN_BARS_APART)):
            found.append(b)

    kinds = ["Groove" if x >= QUIET else "Breakdown" for x in e]
    drops = []
    for b in found:
        quiet = 0
        while b - quiet - 1 >= 0 and e[b - quiet - 1] < MAX_LEVEL_BEFORE:
            quiet += 1
        build = min(BUILD_BARS, quiet)
        for i in range(b - build, b):
            kinds[i] = "Build"
        i = b
        while i < len(e) and (i == b or e[i] >= DROP_HOLD) and (i == b or i not in found):
            kinds[i] = "Drop"
            i += 1
        drops.append({"time": downbeats[b], "bar": b, "build_start": downbeats[b - build], "breakdown_bars": quiet})
    # Groove/breakdown runs shorter than two bars are a fill or a dropped kick, not a section.
    i = 0
    while i < len(kinds):
        j = i
        while j < len(kinds) and kinds[j] == kinds[i]:
            j += 1
        if kinds[i] in ("Groove", "Breakdown") and j - i < 2 and (i > 0 or j < len(kinds)):
            for k in range(i, j):
                kinds[k] = kinds[i - 1] if i > 0 else kinds[j]
        i = j
    # Intro: from the start until the first build or drop, or a breakdown after the kick came in;
    # a quiet opening is an intro, not a breakdown. Outro: after the last drop section.
    first, seen_groove = len(kinds), False
    for i, k in enumerate(kinds):
        if k in ("Build", "Drop") or (k == "Breakdown" and seen_groove):
            first = i
            break
        seen_groove |= k == "Groove"
    for i in range(first):
        kinds[i] = "Intro"
    if found:
        last_drop_end = max(i for i, k in enumerate(kinds) if k == "Drop") + 1
        if not any(k == "Drop" for k in kinds[last_drop_end:]):
            for i in range(last_drop_end, len(kinds)):
                if kinds[i] in ("Groove", "Breakdown"):
                    kinds[i] = "Outro"
    sections = []
    for i, k in enumerate(kinds):
        if sections and sections[-1]["kind"] == k:
            sections[-1]["end"] = downbeats[i + 1]
        else:
            sections.append({"kind": k, "start": downbeats[i], "end": downbeats[i + 1]})
    return {"bars": [round(x, 3) for x in e], "drops": drops, "sections": sections}
