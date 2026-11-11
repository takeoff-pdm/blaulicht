"""What a compiled song should get a second look for, worked out from the numbers alone. No LLM, no GPU.

Flags say where to look. They change nothing: a person answers them with the labelling mode or `song.py fix`.
"""

import bisect

import grid

# What the compiler can use, so what an analysis should have produced. Viewer lanes and unused analyzers aren't here.
CORE = ("adtof", "allinone", "beats", "chords", "drumparts", "embed", "events", "fxhead", "lyrics", "risers", "sed", "stereo",
        "structure", "tags", "timbre", "transcribe", "vocals")
ON_KICK = 0.04  # s
LONG_OUTRO = 0.3  # of the song; the section rule calls everything after the last detected drop an outro
WEAK_GRID = 0.7  # of the trackers' beats on the grid


def run(doc: dict, song: dict, rekordbox_grid: bool, labels: dict | None, fixes: dict | None, review: dict | None) -> dict:
    features, beats, show = doc["features"], doc.get("beats"), doc["show"]
    b = (beats or {}).get("beats") or []
    numbers = grid.report(doc, b, (beats or {}).get("beat_in_bar") or [], features)
    has_trackers = bool(features.get("beats") and features.get("allinone"))
    fix = (fixes or {}).get("grid") or {}
    status = ("none" if not b else "hand" if fix else "corrected" if beats.get("corrected") else "rekordbox" if has_trackers else "unchecked")
    flags = []

    def flag(kind: str, text: str, at: float | None = None) -> None:
        flags.append({"kind": kind, "text": text, **({"at": round(at, 2)} if at is not None else {})})

    if not b:
        flag("grid", "No beat grid, and the trackers can't make one: the song has no bars, sections or drops")
    if status == "unchecked":
        flag("grid", "Rekordbox's grid is unchecked: the beat trackers haven't run on this song")
    share = numbers["trackers_on_grid"]
    if status in ("rekordbox", "corrected") and share is not None and share < WEAK_GRID:
        flag("grid", f"Only {round(100 * share)}% of the beat trackers' beats are on the {'new ' if status == 'corrected' else ''}grid")
    if numbers["bar_start_vote"]:
        flag("bars", f"Both trackers start the bar {numbers['bar_start_vote']} beat{'s' if numbers['bar_start_vote'] > 1 else ''} later than the grid does")
    kicks = sorted(h[0] for h in ((doc.get("cues") or {}).get("hits") or {}).get("kick") or [])
    drops = [d["time"] for d in show["drops"]]
    missed = 0
    if len(kicks) >= 16:
        for t in drops:
            i = bisect.bisect_left(kicks, t)
            if not any(0 <= j < len(kicks) and abs(kicks[j] - t) <= ON_KICK for j in (i - 1, i)):
                missed += 1
                flag("drop", "This drop has no kick on it", t)
    if b and not drops and song.get("genre") == "electronic":
        flag("drop", "No drop found in an electronic song")
    for s in show["sections"]:
        if s["kind"] == "Outro" and s["end"] - s["start"] > LONG_OUTRO * doc["duration"]:
            flag("sections", f"The outro is {round(100 * (s['end'] - s['start']) / doc['duration'])}% of the song, so a drop may have been missed", s["start"])
    missing = [a for a in CORE if a not in features]
    if missing:
        flag("analysis", "Not analysed by: " + ", ".join(missing))
    for f in (review or {}).get("flags") or []:
        flag("review", f"{f.get('problem', '')} (LLM, {f.get('sure', 'unsure')})", f.get("at"))
    return {"grid": status, "trackers_on_grid": share, "kicks_on_beat": numbers["kicks_on_beat"], "drops": len(drops), "drops_on_kick": len(drops) - missed if len(kicks) >= 16 else None,
            "labels": (labels or {}).get("status"), "corrected_by_hand": sorted(fix), "reviewed": bool(review), "flags": flags, "had_rekordbox_grid": rekordbox_grid}
