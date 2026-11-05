"""Slim per-song documents for the decision experiments, built with the viewer's own code.

Calls viz/serve.py's Handler.analysis (no server, nothing written in viz/) for every electronic song
with audio and beats, and keeps what club.js's prepare() and the plan generators read. Run with
PYTHONDONTWRITEBYTECODE=1 so viz/ stays untouched:

    PYTHONDONTWRITEBYTECODE=1 python3 audio-model/melody/decision/build_cache.py --jobs 6
"""

import argparse
import json
import multiprocessing as mp
import sys
import time
from pathlib import Path
from types import SimpleNamespace

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent / "viz"))
sys.dont_write_bytecode = True
import serve  # noqa: E402

CACHE = HERE / "cache"
KEEP = ("id", "name", "duration", "beats", "key", "mood", "phrases", "melody", "energy", "cues")
NOTES = ("line", "bassline", "bass", "game", "lead")

_songs, _meta = None, None


def _init():
    global _songs, _meta
    _songs, _meta = serve.load_songs()


def one(sid):
    out = CACHE / f"{sid}.json"
    if out.exists():
        return sid, 0.0
    t0 = time.time()
    song = next(s for s in _songs if s["id"] == sid)
    fake = SimpleNamespace(cache={}, meta=_meta, similar=lambda _sid, n=5: [])
    doc = json.loads(serve.Handler.analysis(fake, song))
    slim = {k: doc.get(k) for k in KEEP}
    slim["id"], slim["name"] = song["id"], song["name"]
    slim["duration"] = doc.get("duration") or song.get("duration")
    slim["notes"] = {k: v for k, v in (doc.get("notes") or {}).items() if k in NOTES}
    show = doc.get("show") or {}
    slim["show"] = {"sections": show.get("sections") or [], "drops": show.get("drops") or []}
    tags = (doc.get("features") or {}).get("tags") or {}
    slim["tags"] = {"moods": tags.get("moods"), "styles": (tags.get("styles") or [])[:5]}
    tmp = out.with_suffix(".tmp")
    tmp.write_text(json.dumps(slim, separators=(",", ":")))
    tmp.rename(out)
    return sid, time.time() - t0


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--jobs", type=int, default=4)
    args = ap.parse_args()
    CACHE.mkdir(exist_ok=True)
    songs, _ = serve.load_songs()
    todo = [s["id"] for s in songs if s.get("genre") == "electronic" and s.get("has_audio") and s.get("has_beats")]
    print(len(todo), "electronic songs with audio and beats", flush=True)
    with mp.Pool(args.jobs, initializer=_init) as pool:
        for i, (sid, dt) in enumerate(pool.imap_unordered(one, todo), 1):
            if i % 10 == 0 or i == len(todo):
                print(i, sid, f"{dt:.1f}s", flush=True)


if __name__ == "__main__":
    main()
