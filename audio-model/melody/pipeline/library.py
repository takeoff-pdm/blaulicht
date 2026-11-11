"""Where the song data lives, and the song list the compiler builds from it. Standard library only."""

import fcntl
import json
import os
import tempfile
from contextlib import contextmanager
from pathlib import Path

HERE = Path(__file__).resolve().parent
MODEL_DIR = HERE.parent.parent  # audio-model/
DATA = MODEL_DIR / "data/melody/viz"  # <id>.json from build_viz_data.py, songs.json, rekordbox.json, genres.json, lightshows/
REKORDBOX = DATA / "rekordbox.json"  # rekordbox_show.py
OLD_GRIDS = MODEL_DIR / "data/rekordbox_labels.json"  # older grid export, fallback
# electronic or pop per song id: labelled by hand, since only 13 of the first 249 songs carry a
# genre tag; edit the file to move a song. Songs not in it fall back to their genre tag, then to
# Rekordbox's phrase mood ("high" is its club/EDM phrase set).
GENRES = DATA / "genres.json"
FEATURES = MODEL_DIR / "data/melody/features"  # <analyzer>/<id>.json, from ae-charlie
LABELS = MODEL_DIR / "data/melody/labels"  # hand labels, written by the viewer's labelling mode
CORRECTIONS = MODEL_DIR / "data/melody/corrections"  # <id>.json, from `song.py fix`
REVIEWS = MODEL_DIR / "data/melody/reviews"  # <id>.json, from review_llm.py: flags only, nothing applies them
COMPILED = MODEL_DIR / "data/melody/compiled"  # <id>.json and index.json: all the viewer reads
JOBS = MODEL_DIR / "data/melody/jobs"  # <id>.json: songs on their way through ae-charlie
ELECTRONIC_TAGS = ("techno", "house", "trance", "dance", "electro", "edm", "hardstyle", "drum", "dubstep", "bass", "rave", "hard")
POP_TAGS = ("pop", "rock", "schlager", "soul", "funk", "disco", "r&b", "hip hop", "hip-hop", "rap", "country", "folk", "jazz")
# tags like "Music" or "People & Blogs" (YouTube categories) say nothing and fall through to the mood


def write_atomic(path: Path, text: str) -> None:
    """Temp file in the same folder, then a rename: a crash leaves the old file or the new one."""
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, tmp = tempfile.mkstemp(dir=path.parent, prefix=f".{path.stem}-", suffix=".tmp")
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as f:
            f.write(text)
        os.chmod(tmp, 0o644)
        os.replace(tmp, path)
    except BaseException:
        Path(tmp).unlink(missing_ok=True)
        raise


@contextmanager
def shared_lists():
    """Held while changing songs.json, rekordbox.json or the compiled song list, since the page can run several commands at once."""
    COMPILED.mkdir(parents=True, exist_ok=True)
    with (COMPILED / ".lock").open("w") as f:
        fcntl.flock(f, fcntl.LOCK_EX)
        yield


def load_songs() -> tuple[list[dict], dict]:
    """Every analysed song with what Rekordbox knows about it, and per path what the compiler needs from Rekordbox."""
    songs = json.loads((DATA / "songs.json").read_text())
    meta = {}
    if OLD_GRIDS.exists():
        for t in json.loads(OLD_GRIDS.read_text()):
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
