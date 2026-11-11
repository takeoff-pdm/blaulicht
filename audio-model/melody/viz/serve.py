"""Melody Lab: play a compiled song in the browser with its analysis and a club light show.

Serves `viewer.html` and `club.js`, the compiled songs (`audio-model/data/melody/compiled/<id>.json`, built by
`audio-model/melody/pipeline/song.py`), and the song audio straight from its Rekordbox path. It works nothing out
itself: compile a song and the next page load picks it up, with no restart. Standard library only, bound to
localhost; it only serves songs in the compiled list. The viewer's labelling mode (`label.js`) reads and writes the
hand labels in `audio-model/data/melody/labels/<id>.json` through `/api/labels/<id>`, in the order of `/api/labels/queue`.

    python3 audio-model/melody/viz/serve.py            # http://127.0.0.1:8765
    python3 audio-model/melody/viz/serve.py --open --port 8800
"""

import argparse
import json
import math
import mimetypes
import os
import random
import tempfile
import threading
import webbrowser
from datetime import datetime, timezone
from http import HTTPStatus
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

HERE = Path(__file__).resolve().parent
MODEL_DIR = HERE.parent.parent  # audio-model/
DATA = MODEL_DIR / "data/melody/viz"  # lightshows/index.json, for the Human Lighter's song menu
COMPILED = MODEL_DIR / "data/melody/compiled"  # <id>.json and index.json, from pipeline/song.py
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
    stamp = -1

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

    @classmethod
    def refresh(cls) -> None:
        """The song list, read again whenever the compiler has changed it."""
        index = COMPILED / "index.json"
        stamp = index.stat().st_mtime_ns if index.exists() else 0
        if stamp != cls.stamp:
            songs = json.loads(index.read_text()) if stamp else []
            for s in songs:
                s["has_audio"] = Path(s["path"]).exists()
            cls.songs, cls.by_id, cls.stamp = songs, {s["id"]: s for s in songs}, stamp

    def do_GET(self):
        self.refresh()
        path = self.path.split("?", 1)[0]
        if path in STATIC:
            name = STATIC[path]
            ctype = "text/html; charset=utf-8" if name.endswith(".html") else "text/javascript; charset=utf-8"
            return self.send_bytes((HERE / name).read_bytes(), ctype)
        if path == "/api/lightshows":
            index = DATA / "lightshows/index.json"
            return self.send_bytes(index.read_bytes() if index.exists() else b"[]", "application/json")
        if path == "/api/songs":
            return self.send_bytes(json.dumps(self.songs).encode(), "application/json")
        if path == "/api/labels/queue":
            return self.send_bytes(json.dumps(label_queue(self.songs, self.by_id)).encode(), "application/json")
        parts = path.strip("/").split("/")
        if len(parts) == 3 and parts[0] == "api" and parts[2] in self.by_id:
            song = self.by_id[parts[2]]
            if parts[1] == "analysis":
                return self.send_bytes((COMPILED / f"{song['id']}.json").read_bytes(), "application/json")
            if parts[1] == "audio" and song["has_audio"]:
                audio = Path(song["path"])
                ctype = mimetypes.guess_type(audio.name)[0] or "application/octet-stream"
                return self.send_bytes(audio.read_bytes(), ctype, cache=True)
            if parts[1] == "labels":  # null: not labelled yet, which is no error
                labels = HAND_LABELS / f"{song['id']}.json"
                return self.send_bytes(labels.read_bytes() if labels.exists() else b"null", "application/json")
        self.send_error(HTTPStatus.NOT_FOUND)

    def do_POST(self):
        self.refresh()
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
    Handler.refresh()
    songs = Handler.songs
    url = f"http://127.0.0.1:{args.port}/"
    print(f"{len(songs)} compiled songs ({sum(s['genre'] == 'electronic' for s in songs)} electronic, {sum(s['has_audio'] for s in songs)} with audio, "
          f"{sum(s['has_beats'] for s in songs)} with beat grids, {sum(bool(s['key']) for s in songs)} with keys) at {url}", flush=True)
    if not songs:
        print("Nothing compiled yet: python3 audio-model/melody/pipeline/song.py compile --all", flush=True)
    server = ThreadingHTTPServer(("127.0.0.1", args.port), Handler)
    if args.open:
        webbrowser.open(url)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass


if __name__ == "__main__":
    main()
