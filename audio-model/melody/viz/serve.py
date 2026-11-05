"""Melody Lab: play a pre-analysed song in the browser with its analysis and a club light show.

Serves `viewer.html` and `club.js`, the per-song analysis (`audio-model/data/melody/viz/<id>.json`,
built on ae-charlie by `build_viz_data.py`), the song audio straight from its Rekordbox path,
and what Rekordbox knows about the song (`rekordbox.json` from `rekordbox_show.py`: beat grid,
BPM, key, phrases; the grid and phrases moved onto the drum hits of the audio, `grid.py`), plus
bar sections and drops computed from the stem loudness (`sections.py`), and clean beat-synced
melody and bass lines with chord roots and hooks (`melody_line.py`). Standard library only, bound to localhost; it only serves songs listed in
songs.json. The viewer's labelling mode (`label.js`) reads and writes the hand labels in
`audio-model/data/melody/labels/<id>.json` through `/api/labels/<id>`, in the order of `/api/labels/queue`.

    python3 audio-model/melody/viz/serve.py            # http://127.0.0.1:8765
    python3 audio-model/melody/viz/serve.py --open --port 8800
"""

import argparse
import base64
import json
import math
import struct
import mimetypes
import os
import random
import sys
import tempfile
import threading
import webbrowser
from datetime import datetime, timezone
from http import HTTPStatus
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import grid  # noqa: E402
import melody_line  # noqa: E402
import sections  # noqa: E402
import show_compile  # noqa: E402

MODEL_DIR = HERE.parent.parent  # audio-model/
DATA = MODEL_DIR / "data/melody/viz"
REKORDBOX = DATA / "rekordbox.json"  # rekordbox_show.py
LABELS = MODEL_DIR / "data/rekordbox_labels.json"  # older grid export, fallback
# electronic or pop per song id: labelled by hand, since only 13 of the first 249 songs carry a
# genre tag; edit the file to move a song. Songs not in it fall back to their genre tag, then to
# Rekordbox's phrase mood ("high" is its club/EDM phrase set).
GENRES = DATA / "genres.json"
FEATURES = MODEL_DIR / "data/melody/features"  # <analyzer>/<id>.json, from analyze/sync.sh
ELECTRONIC_TAGS = ("techno", "house", "trance", "dance", "electro", "edm", "hardstyle", "drum", "dubstep", "bass", "rave", "hard")
POP_TAGS = ("pop", "rock", "schlager", "soul", "funk", "disco", "r&b", "hip hop", "hip-hop", "rap", "country", "folk", "jazz")
# tags like "Music" or "People & Blogs" (YouTube categories) say nothing and fall through to the mood
STATIC = {"/": "viewer.html", "/index.html": "viewer.html", "/club.js": "club.js", "/label.js": "label.js", "/club_looks.mjs": "club_looks.mjs"}
# Hand labels, the project's ground truth. The queue is a fixed random order over the electronic songs,
# so that a prefix of it is an unbiased sample; not a .json, so a glob over the labels finds only labels.
HAND_LABELS = MODEL_DIR / "data/melody/labels"
QUEUE = HAND_LABELS / "queue.txt"
QUEUE_SEED = 20261105
QUEUE_TARGET = 100  # how many songs the sample should reach; the queue itself holds every eligible song
SECTION_LABELS = ("intro", "buildup", "drop", "breakdown", "outro", "silence")
FX_KINDS = ("riser", "downlifter", "impact", "airhorn", "siren", "other")
MAX_LABEL_BYTES = 1_000_000
DURATION_TOLERANCE = 0.1  # songs.json rounds the duration
queue_lock = threading.Lock()


def load_songs() -> tuple[list[dict], dict]:
    songs = json.loads((DATA / "songs.json").read_text())
    meta = {}
    if LABELS.exists():
        for t in json.loads(LABELS.read_text()):
            meta[t["path"]] = {"bpm": t.get("bpm"), "beats": t.get("beats_s") or [], "beat_in_bar": t.get("beat_in_bar") or []}
    if REKORDBOX.exists():
        for t in json.loads(REKORDBOX.read_text()):
            m = meta.setdefault(t["path"], {})
            m.update({k: t[k] for k in ("bpm", "key", "mood", "genre", "phrases", "beats", "beat_in_bar") if t.get(k) is not None})
    labels = json.loads(GENRES.read_text()) if GENRES.exists() else {}
    for s in songs:
        m = meta.get(s["path"], {})
        s["has_beats"] = bool(m.get("beats"))
        s["has_audio"] = Path(s["path"]).exists()
        s["key"], s["bpm"] = m.get("key"), m.get("bpm")
        tag = (m.get("genre") or "").lower()
        s["genre"] = (labels.get(s["id"], {}).get("genre")
                      or ("electronic" if any(k in tag for k in ELECTRONIC_TAGS) else "pop" if any(k in tag for k in POP_TAGS) else None)
                      or ("electronic" if m.get("mood") == "high" else "pop"))
    return songs, meta


def _is_time(x) -> bool:
    return isinstance(x, (int, float)) and not isinstance(x, bool) and math.isfinite(x)


def _check_sections(sections, duration: float, extra: tuple = ()):
    """Sorted, contiguous, and covering the song from exactly 0 to its duration."""
    if not isinstance(sections, list) or not sections:
        raise ValueError("sections must be a non-empty list")
    pos = 0.0
    for s in sections:
        if not isinstance(s, dict) or not {"start", "end", "label"} <= s.keys() <= {"start", "end", "label", *extra}:
            raise ValueError(f"a section needs exactly start, end and label: {s!r}")
        if not (_is_time(s["start"]) and _is_time(s["end"]) and s["start"] < s["end"]):
            raise ValueError(f"a section needs start < end: {s!r}")
        if s["label"] not in SECTION_LABELS:
            raise ValueError(f"unknown section label: {s['label']!r}")
        if not all(isinstance(s[k], str) for k in extra if k in s):
            raise ValueError(f"{extra} must be strings: {s!r}")
        if abs(s["start"] - pos) > 1e-6:
            raise ValueError(f"sections must run contiguously from 0, but one starts at {s['start']} after {pos}")
        pos = s["end"]
    if abs(pos - duration) > DURATION_TOLERANCE:
        raise ValueError(f"sections must end at the song's duration {duration}, not {pos}")


def validate_labels(doc, song: dict) -> dict:
    """A POST /api/labels/<id> body, checked (ValueError if bad) and ready to store. `updated` is set here."""
    required = {"id", "name", "version", "status", "labeller", "sections", "fx", "suggested"}
    if not isinstance(doc, dict) or not required <= doc.keys() <= required | {"updated"}:
        raise ValueError(f"the body must be an object with exactly {sorted(required)}")
    if doc["id"] != song["id"]:
        raise ValueError(f"id {doc['id']!r} is not this song's")
    if not isinstance(doc["name"], str):
        raise ValueError("name must be a string")
    if doc["version"] != 1 or isinstance(doc["version"], bool):
        raise ValueError("version must be 1")
    if doc["status"] not in ("partial", "done"):
        raise ValueError("status must be partial or done")
    if doc["labeller"] != "human":
        raise ValueError("labeller must be human")
    duration = song["duration"]
    _check_sections(doc["sections"], duration)
    if not isinstance(doc["fx"], list):
        raise ValueError("fx must be a list")
    for f in doc["fx"]:
        if not isinstance(f, dict) or f.keys() != {"start", "end", "kind"}:
            raise ValueError(f"an fx mark needs exactly start, end and kind: {f!r}")
        if not (_is_time(f["start"]) and _is_time(f["end"]) and 0 <= f["start"] <= f["end"] <= duration + DURATION_TOLERANCE):
            raise ValueError(f"an fx mark needs 0 <= start <= end <= duration: {f!r}")
        if f["kind"] not in FX_KINDS:
            raise ValueError(f"unknown fx kind: {f['kind']!r}")
    suggested = doc["suggested"]
    if not isinstance(suggested, dict) or suggested.keys() != {"sections", "source"} or not isinstance(suggested["source"], str):
        raise ValueError("suggested must be an object with exactly sections and source")
    _check_sections(suggested["sections"], duration, extra=("via", "rule"))
    return {"id": doc["id"], "name": doc["name"], "version": 1, "status": doc["status"], "labeller": "human",
            "updated": datetime.now(timezone.utc).isoformat(timespec="seconds"),
            "sections": doc["sections"], "fx": doc["fx"], "suggested": suggested}


def write_atomic(path: Path, text: str):
    """Temp file in the same folder, then a rename: a crash leaves the old file or the new one."""
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, tmp = tempfile.mkstemp(dir=path.parent, prefix=f".{path.stem}-", suffix=".tmp")
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as f:
            f.write(text)
            f.flush()
            os.fsync(f.fileno())
        os.chmod(tmp, 0o644)
        os.replace(tmp, path)
    except BaseException:
        Path(tmp).unlink(missing_ok=True)
        raise


def label_status(sid: str) -> str | None:
    try:
        return json.loads((HAND_LABELS / f"{sid}.json").read_text(encoding="utf-8")).get("status")
    except (OSError, ValueError):
        return None


def label_queue(songs: list[dict], by_id: dict) -> dict:
    """The fixed random order (computed once, then read from the file) and how far along it the labels are."""
    eligible = lambda s: s["genre"] == "electronic" and s["has_audio"] and s["has_beats"]
    with queue_lock:
        if not QUEUE.exists():
            order = sorted(s["id"] for s in songs if eligible(s))
            random.Random(QUEUE_SEED).shuffle(order)
            write_atomic(QUEUE, f"# labelling queue, seed {QUEUE_SEED}: electronic songs with audio and beats, shuffled\n" + "\n".join(order) + "\n")
    order = [i for i in QUEUE.read_text().split("\n") if i and not i.startswith("#") and i in by_id and eligible(by_id[i])]
    status = {i: st for i in order if (st := label_status(i))}
    return {"order": order, "status": status, "total": len(order), "target": min(QUEUE_TARGET, len(order)), "seed": QUEUE_SEED,
            "done": sum(st == "done" for st in status.values()), "partial": sum(st == "partial" for st in status.values()),
            "next": next((i for i in order if status.get(i) != "done"), None)}


class Handler(BaseHTTPRequestHandler):
    songs: list[dict] = []
    by_id: dict = {}
    meta: dict = {}
    cache: dict = {}

    def log_message(self, fmt, *args):  # quiet: only errors
        if not str(args[1] if len(args) > 1 else "").startswith(("2", "3")):
            super().log_message(fmt, *args)

    def send_bytes(self, body: bytes, ctype: str, cache: bool = False, status: HTTPStatus = HTTPStatus.OK):
        self.send_response(status)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "max-age=3600" if cache else "no-store")
        self.end_headers()
        self.wfile.write(body)

    def similar(self, sid: str, n: int = 5) -> list:
        """The songs that sound most alike (cosine of the CLAP song embeddings, analyze/embed.py)."""
        if not hasattr(Handler, "_emb"):
            emb = {}  # published only when complete: requests run on threads
            for p in FEATURES.glob("embed/*.json"):
                raw = base64.b64decode(json.loads(p.read_text()).get("embedding") or "")
                v = struct.unpack(f"<{len(raw) // 2}e", raw)
                norm = math.sqrt(sum(x * x for x in v)) or 1.0
                emb[p.stem] = [x / norm for x in v]
            Handler._emb = emb
        me = Handler._emb.get(sid)
        if not me:
            return []
        sims = sorted(((sum(a * b for a, b in zip(me, v)), k) for k, v in Handler._emb.items() if k != sid), reverse=True)[:n]
        return [[k, self.by_id[k]["name"], round(s, 3)] for s, k in sims if k in self.by_id]

    def analysis(self, song: dict) -> bytes:
        if song["id"] not in self.cache:
            doc = json.loads((DATA / f"{song['id']}.json").read_text())
            m = self.meta.get(song["path"], {})
            rb_beats, phrases = m.get("beats") or [], m.get("phrases") or []
            # Rekordbox's grid, moved onto the drum hits of the audio we play (grid.py)
            beats, shift = grid.align(doc, rb_beats) if rb_beats else ([], None)
            if rb_beats:
                phrases = [{**p, **{k: grid.warp(p[k], rb_beats, beats) for k in ("start", "end", "fill")}} for p in phrases]
            doc["beats"] = {"bpm": m.get("bpm"), "beats": beats, "beat_in_bar": m.get("beat_in_bar") or [],
                            "aligned": shift} if beats else None
            doc["key"], doc["mood"], doc["phrases"] = m.get("key"), m.get("mood"), phrases
            doc["show"] = sections.analyse(doc, beats, m.get("beat_in_bar") or []) if beats else {"bars": [], "drops": [], "sections": []}
            # clean, beat-synced melody and bass lines, chord roots and hooks (melody_line.py)
            doc["melody"] = melody_line.analyse(doc, beats, m.get("beat_in_bar") or [], m.get("key")) if beats else None
            # the offline analyzers' features (audio-model/melody/analyze, synced from ae-charlie)
            doc["features"] = {p.parent.name: json.loads(p.read_text()) for p in sorted(FEATURES.glob(f"*/{song['id']}.json"))}
            (doc["features"].get("mt3") or {}).pop("notes", None)  # the viewer only draws when each instrument plays
            (doc["features"].get("fxhead") or {}).pop("tracks", None)  # and only the effect head's events
            doc["cues"] = show_compile.compile_show(doc)  # everything merged into one cue list
            doc["similar"] = self.similar(song["id"])
            self.cache[song["id"]] = json.dumps(doc, separators=(",", ":")).encode()
        return self.cache[song["id"]]

    def do_GET(self):
        path = self.path.split("?", 1)[0]
        if path in STATIC:
            name = STATIC[path]
            ctype = "text/html; charset=utf-8" if name.endswith(".html") else "text/javascript; charset=utf-8"
            return self.send_bytes((HERE / name).read_bytes(), ctype)
        if path == "/api/songs":
            return self.send_bytes(json.dumps(self.songs).encode(), "application/json")
        if path == "/api/labels/queue":
            return self.send_bytes(json.dumps(label_queue(self.songs, self.by_id)).encode(), "application/json")
        parts = path.strip("/").split("/")
        if len(parts) == 3 and parts[0] == "api" and parts[2] in self.by_id:
            song = self.by_id[parts[2]]
            if parts[1] == "analysis":
                return self.send_bytes(self.analysis(song), "application/json")
            if parts[1] == "audio" and song["has_audio"]:
                audio = Path(song["path"])
                ctype = mimetypes.guess_type(audio.name)[0] or "application/octet-stream"
                return self.send_bytes(audio.read_bytes(), ctype, cache=True)
            if parts[1] == "labels":  # null: not labelled yet, which is no error
                labels = HAND_LABELS / f"{song['id']}.json"
                return self.send_bytes(labels.read_bytes() if labels.exists() else b"null", "application/json")
        self.send_error(HTTPStatus.NOT_FOUND)

    def do_POST(self):
        parts = self.path.split("?", 1)[0].strip("/").split("/")
        if not (len(parts) == 3 and parts[:2] == ["api", "labels"] and parts[2] in self.by_id):
            return self.send_error(HTTPStatus.NOT_FOUND)
        # Only this page: other sites can't send JSON without a preflight (which fails), or reach us under their name.
        if self.headers.get_content_type() != "application/json" or self.headers.get("Host", "").split(":")[0] not in ("127.0.0.1", "localhost"):
            return self.send_error(HTTPStatus.FORBIDDEN)
        reply = lambda obj, status=HTTPStatus.OK: self.send_bytes(json.dumps(obj, ensure_ascii=False).encode(), "application/json", status=status)
        try:
            length = int(self.headers.get("Content-Length") or 0)
            if not 0 < length <= MAX_LABEL_BYTES:
                raise ValueError(f"the body must be 1 to {MAX_LABEL_BYTES} bytes")
            labels = validate_labels(json.loads(self.rfile.read(length)), self.by_id[parts[2]])
        except (ValueError, RecursionError) as e:  # ValueError includes malformed JSON
            return reply({"error": str(e)}, HTTPStatus.BAD_REQUEST)
        try:
            write_atomic(HAND_LABELS / f"{parts[2]}.json", json.dumps(labels, ensure_ascii=False, indent=1) + "\n")
        except OSError as e:
            return reply({"error": f"could not write the labels: {e}"}, HTTPStatus.INTERNAL_SERVER_ERROR)
        reply(labels)


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--port", type=int, default=8765)
    ap.add_argument("--open", action="store_true", help="open the viewer in the default browser")
    args = ap.parse_args()
    songs, meta = load_songs()
    Handler.songs, Handler.meta = songs, meta
    Handler.by_id = {s["id"]: s for s in songs}
    url = f"http://127.0.0.1:{args.port}/"
    print(f"{len(songs)} songs ({sum(s['genre'] == 'electronic' for s in songs)} electronic, {sum(s['has_audio'] for s in songs)} with audio, "
          f"{sum(s['has_beats'] for s in songs)} with beat grids, {sum(bool(s['key']) for s in songs)} with keys) at {url}",
          flush=True)
    server = ThreadingHTTPServer(("127.0.0.1", args.port), Handler)
    if args.open:
        webbrowser.open(url)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass


if __name__ == "__main__":
    main()
