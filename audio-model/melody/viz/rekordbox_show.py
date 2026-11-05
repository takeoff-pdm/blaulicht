# /// script
# requires-python = ">=3.11"
# dependencies = ["pyrekordbox>=0.5", "sqlcipher3-wheels", "numpy"]
# ///
"""Export what Rekordbox knows about each song for the viewer's light show.

Per song: BPM, key, genre tag, the beat grid (beat times, beat in bar) and the phrase analysis (mood,
phrases with start beat, kind and label, fill-ins). Reads a copy of master.db, so Rekordbox may
stay open. Labels follow Rekordbox's phrase moods (high: Intro/Up/Down/Chorus/Outro; mid and
low: Intro/Verse/Bridge/Chorus/Outro).

    uv run audio-model/melody/viz/rekordbox_show.py audio-model/data/melody/viz/rekordbox.json
"""

import json
import logging
import shutil
import sys
import tempfile
import warnings
from pathlib import Path

import numpy as np

warnings.filterwarnings("ignore")
logging.disable(logging.WARNING)
from pyrekordbox.masterdb import MasterDatabase  # noqa: E402

RB_DIR = Path.home() / "Library/Pioneer/rekordbox"
MOODS = {1: "high", 2: "mid", 3: "low"}
LABELS = {
    1: {1: "Intro", 2: "Up", 3: "Down", 5: "Chorus", 6: "Outro"},
    2: {1: "Intro", 2: "Verse", 3: "Verse", 4: "Verse", 5: "Verse", 6: "Verse", 7: "Verse", 8: "Bridge",
        9: "Chorus", 10: "Outro"},
    3: {1: "Intro", 2: "Verse", 3: "Verse", 4: "Verse", 5: "Verse", 6: "Verse", 7: "Verse", 8: "Bridge",
        9: "Chorus", 10: "Outro"},
}


def main():
    out = Path(sys.argv[1])
    with tempfile.TemporaryDirectory() as tmp:
        tmp = Path(tmp)
        shutil.copy2(RB_DIR / "master.db", tmp / "master.db")
        (tmp / "share").symlink_to(RB_DIR / "share")  # ANLZ paths resolve relative to the db
        db = MasterDatabase(path=tmp / "master.db", unlock=True)
        rows = []
        for c in db.get_content():
            path = c.FolderPath or ""
            if "/Sampler/" in path:
                continue
            row = {"path": path, "bpm": (c.BPM or 0) / 100, "key": c.Key.ScaleName if c.Key else None,
                   "genre": c.Genre.Name if c.Genre else None}
            files = db.read_anlz_files(c)
            dat = next((f for p, f in files.items() if p.suffix == ".DAT"), None)
            if dat is not None and "PQTZ" in dat.tag_types:
                beat_in_bar, _, beats = (np.asarray(x) for x in dat.get("beat_grid"))
                row["beats"] = beats.astype(float).round(4).tolist()
                row["beat_in_bar"] = beat_in_bar.astype(int).tolist()
            ext = next((f for p, f in files.items() if p.suffix == ".EXT"), None)
            if ext is not None and "PSSI" in ext.tag_types and "beats" in row:
                pssi = ext.get_tag("PSSI").content
                mood = int(pssi.mood)
                beats = row["beats"]
                at = lambda b: beats[min(max(int(b) - 1, 0), len(beats) - 1)]  # 1-based beat numbers
                entries = list(pssi.entries)
                phrases = []
                for i, e in enumerate(entries):
                    end_beat = entries[i + 1].beat if i + 1 < len(entries) else pssi.end_beat
                    kind = int(e.kind)
                    phrases.append({
                        "start": at(e.beat), "end": at(end_beat), "kind": kind,
                        "label": LABELS.get(mood, {}).get(kind, f"kind {kind}"),
                        "fill": at(e.beat_fill) if int(e.fill) and int(e.beat_fill) else None,
                    })
                row["mood"] = MOODS.get(mood, str(mood))
                row["phrases"] = phrases
            rows.append(row)
    out.write_text(json.dumps(rows))
    print(f"{len(rows)} songs, {sum('beats' in r for r in rows)} grids, {sum('phrases' in r for r in rows)} "
          f"phrase analyses, {sum(bool(r['key']) for r in rows)} keys -> {out}", file=sys.stderr)


if __name__ == "__main__":
    main()
