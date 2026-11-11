"""Compile one analysed song into the file the viewer plays, data/melody/compiled/<id>.json.

Takes what ae-charlie measured (data/melody/viz/<id>.json, data/melody/features/*/<id>.json) and what Rekordbox
knows (rekordbox.json), and works out the show's facts: the beat grid (grid.py), bar sections and drops (sections.py),
the clean melody and bass lines (melody_line.py) and the merged cue list (show_compile.py). No GPU and only the
standard library, so a change here reaches every song in a few minutes. Use it through `song.py compile`.

Finished hand labels and `song.py fix` corrections are applied here, kept in their own files so a recompile can't lose them
and it stays clear what a person changed. An LLM review only ever adds flags. `check` in the file is what checks.py found,
and `compiled` says what the file was built from, so `song.py status` can tell when it has gone stale.
"""

import base64
import hashlib
import json
import math
import struct
from datetime import datetime, timezone
from pathlib import Path

import checks
import grid
import library as lib
import melody_line
import sections
import show_compile

VERSION = 1
CODE = ("compile_song.py", "checks.py", "grid.py", "sections.py", "melody_line.py", "show_compile.py")


def code_hash() -> str:
    h = hashlib.sha256()
    for name in CODE:
        h.update((lib.HERE / name).read_bytes())
    return h.hexdigest()[:16]


def input_files(sid: str) -> list[Path]:
    return [lib.DATA / f"{sid}.json", lib.DATA / "lightshows" / f"{sid}.json", lib.LABELS / f"{sid}.json", lib.CORRECTIONS / f"{sid}.json",
            lib.REVIEWS / f"{sid}.json", *sorted(lib.FEATURES.glob(f"*/{sid}.json"))]


def inputs_hash(sid: str, song: dict, rekordbox: dict) -> str:
    """What the song was built from: the files' sizes and times, and its Rekordbox row. The Sounds Like list also
    depends on every other song's embedding: that doesn't count, or one new song would make all others stale."""
    h = hashlib.sha256(json.dumps([rekordbox, song.get("genre")], sort_keys=True).encode())
    for p in input_files(sid):
        if p.exists():
            st = p.stat()
            h.update(f"{p.name}:{p.parent.name}:{st.st_size}:{st.st_mtime_ns};".encode())
    return h.hexdigest()[:16]


def load_embeddings() -> dict:
    """The song embeddings (CLAP, analyze/embed.py), normalised, for the Sounds Like list."""
    emb = {}
    for p in lib.FEATURES.glob("embed/*.json"):
        raw = base64.b64decode(json.loads(p.read_text()).get("embedding") or "")
        v = struct.unpack(f"<{len(raw) // 2}e", raw)
        norm = math.sqrt(sum(x * x for x in v)) or 1.0
        emb[p.stem] = [x / norm for x in v]
    return emb


def similar(sid: str, by_id: dict, emb: dict, n: int = 5) -> list:
    """The songs that sound most alike (cosine of the song embeddings)."""
    me = emb.get(sid)
    if not me:
        return []
    sims = sorted(((sum(a * b for a, b in zip(me, v)), k) for k, v in emb.items() if k != sid), reverse=True)[:n]
    return [[k, by_id[k]["name"], round(s, 3)] for s, k in sims if k in by_id]


def read_json(path: Path) -> dict | None:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return None


def compile_doc(song: dict, meta: dict, by_id: dict, emb: dict, labels: dict | None = None, fixes: dict | None = None) -> dict:
    sid = song["id"]
    labels = labels if labels is not None else read_json(lib.LABELS / f"{sid}.json")
    fixes = fixes if fixes is not None else read_json(lib.CORRECTIONS / f"{sid}.json")
    doc = json.loads((lib.DATA / f"{sid}.json").read_text())
    m = meta.get(song["path"], {})
    rb_beats, inbar, bpm, phrases = m.get("beats") or [], m.get("beat_in_bar") or [], m.get("bpm"), m.get("phrases") or []
    features = {p.parent.name: json.loads(p.read_text()) for p in sorted(lib.FEATURES.glob(f"*/{sid}.json"))}
    # Rekordbox's grid, moved onto the drum hits of the audio we play, or replaced where the beat trackers show it's wrong
    fix = (fixes or {}).get("grid") or {}
    beats, shift = grid.align(doc, rb_beats) if rb_beats else ([], None)
    fixed = None if fix.get("source") == "rekordbox" else grid.correct(doc, beats, features, force=fix.get("source") == "trackers")
    if fixed:
        beats, inbar, bpm, shift = fixed["beats"], fixed["beat_in_bar"], fixed["bpm"], fixed["aligned"]
        phrases = grid.snap_phrases(phrases, beats, inbar)
    elif rb_beats:
        phrases = [{**p, **{k: grid.warp(p[k], rb_beats, beats) for k in ("start", "end", "fill")}} for p in phrases]
    if beats and fix.get("shift_16ths"):
        beats = grid.shift_grid(beats, fix["shift_16ths"])
    if beats and fix.get("bar_shift"):
        inbar = grid.rotate_bars(inbar, fix["bar_shift"])
    if beats and (fix.get("shift_16ths") or fix.get("bar_shift")):
        phrases = grid.snap_phrases(phrases, beats, inbar)
    doc["beats"] = {"bpm": bpm, "beats": beats, "beat_in_bar": inbar, "aligned": shift} if beats else None
    if fixed:
        doc["beats"]["corrected"] = fixed["summary"]
    doc["key"], doc["mood"], doc["phrases"] = m.get("key"), m.get("mood"), phrases
    doc["show"] = sections.analyse(doc, beats, inbar) if beats else {"bars": [], "drops": [], "sections": []}
    if beats and labels and labels.get("status") == "done":
        doc["show"] = sections.from_labels(doc["show"], labels, beats, inbar)
    doc["melody"] = melody_line.analyse(doc, beats, inbar, m.get("key")) if beats else None
    doc["features"] = features
    (doc["features"].get("mt3") or {}).pop("notes", None)  # the viewer only draws when each instrument plays
    (doc["features"].get("fxhead") or {}).pop("tracks", None)  # and only the effect head's events
    show = lib.DATA / "lightshows" / f"{sid}.json"  # decision/bs_lightshow.py, for the Human Lighter look
    if show.exists():
        doc["lightshow"] = json.loads(show.read_text())
    doc["cues"] = show_compile.compile_show(doc)
    doc["similar"] = similar(sid, by_id, emb)
    doc["check"] = checks.run(doc, song, bool(rb_beats), labels, fixes, read_json(lib.REVIEWS / f"{sid}.json"))
    return doc


def compile_to_file(song: dict, meta: dict, by_id: dict, emb: dict, code: str) -> dict:
    """Writes the song's compiled file and returns what the song list should say about it."""
    doc = compile_doc(song, meta, by_id, emb)
    doc["compiled"] = {"version": VERSION, "at": datetime.now(timezone.utc).isoformat(timespec="seconds"), "code": code,
                       "inputs": inputs_hash(song["id"], song, meta.get(song["path"], {}))}
    lib.write_atomic(lib.COMPILED / f"{song['id']}.json", json.dumps(doc, separators=(",", ":")))
    return {"bpm": (doc["beats"] or {}).get("bpm", song.get("bpm")), "has_beats": bool(doc["beats"]), "check": doc["check"]}
