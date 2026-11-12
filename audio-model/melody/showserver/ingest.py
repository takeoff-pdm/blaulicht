"""A track from a player's media into the pipeline: the audio file, plus what rekordbox knows about it.

The audio lands in data/melody/prolink/<sha1 of the bytes>/<file name>, and that path is the song's
`listed` path, so the same audio from two sticks is one song. The beat grid, BPM, key and genre go
into viz/rekordbox.json in the shape rekordbox_show.py writes, which is where the compiler reads them.
"""

import hashlib
import json
import os
import tempfile
from pathlib import Path

import httpx

import workers  # noqa: F401  (puts pipeline/ on sys.path)
import library as lib  # noqa: E402
import song  # noqa: E402

AUDIO = lib.MODEL_DIR / "data/melody/prolink"


def track_meta(bridge: str, device: int, slot: int, track_id: int, grid: bool = False) -> dict:
    r = httpx.get(f"{bridge}/track", params={"device": device, "slot": slot, "id": track_id, "grid": int(grid)}, timeout=60)
    r.raise_for_status()
    return r.json()


def download(bridge: str, device: int, slot: int, track_id: int, name: str) -> Path:
    """Streams the file to disk, then moves it to its content-addressed folder."""
    AUDIO.mkdir(parents=True, exist_ok=True)
    h = hashlib.sha1()
    fd, tmp = tempfile.mkstemp(dir=AUDIO, prefix=".dl-")
    try:
        with os.fdopen(fd, "wb") as f, httpx.stream("GET", f"{bridge}/audio", params={"device": device, "slot": slot, "id": track_id},
                                                     timeout=httpx.Timeout(30, read=600)) as r:
            if r.status_code != 200:
                r.read()
                raise RuntimeError(f"download failed: {r.text[:200]}")
            for chunk in r.iter_bytes(1 << 20):
                h.update(chunk)
                f.write(chunk)
        dest = AUDIO / h.hexdigest()[:12] / Path(name).name
        dest.parent.mkdir(parents=True, exist_ok=True)
        os.replace(tmp, dest)
        return dest
    except BaseException:
        Path(tmp).unlink(missing_ok=True)
        raise


def rekordbox_row(listed: str, meta: dict) -> dict:
    row = {"path": listed, "bpm": meta.get("bpm") or 0, "key": meta.get("key"), "genre": meta.get("genre")}
    if meta.get("beats"):
        row["beats"], row["beat_in_bar"] = meta["beats"], meta["beat_in_bar"]
    return row  # phrases (PSSI) aren't read over the network; the compiler works without them


def save_rekordbox_row(row: dict) -> None:
    with lib.shared_lists():
        rows = json.loads(lib.REKORDBOX.read_text()) if lib.REKORDBOX.exists() else []
        rows = [r for r in rows if r["path"] != row["path"]] + [row]
        lib.write_atomic(lib.REKORDBOX, json.dumps(rows))


def ingest(bridge: str, store, job: dict) -> dict:
    """Downloads if needed. Returns the fields to set on the job: song_id, listed, file info."""
    meta = track_meta(bridge, job["device"], job["slot"], job["track_id"], grid=True)
    known = store.known(meta["file_path"], meta.get("file_size"))
    if known and Path(known["listed"]).exists():
        listed = known["listed"]
    else:
        listed = str(download(bridge, job["device"], job["slot"], job["track_id"], meta.get("file_name") or meta["file_path"]))
    sid = song.song_id(listed)
    save_rekordbox_row(rekordbox_row(listed, meta))
    store.remember(meta["file_path"], meta.get("file_size"), sid, listed)
    return {"song_id": sid, "listed": listed, "file_path": meta["file_path"], "file_size": meta.get("file_size"),
            "title": meta.get("title") or job.get("title"), "artist": meta.get("artist") or job.get("artist")}
