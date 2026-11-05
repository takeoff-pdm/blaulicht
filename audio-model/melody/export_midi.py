"""Collect the per-song MIDI files under readable names: export/<method>/<song file stem>.mid.

Melodia outputs are keyed by sha1 of the root-relative path (melodia_batch.py), the stem-based
ones (game, lead, bass, student) by sha1 of the absolute path (songs_remote.txt); both map back
to the Rekordbox file name here.

    python export_midi.py export
"""

import hashlib
import json
import shutil
import sys
from pathlib import Path

ROOT = Path("/localhome/sokrates/melody")


def main():
    out = Path(sys.argv[1])
    index = json.loads((ROOT / "out/melodia/index.json").read_text())
    rows = []
    for sid, v in index.items():
        abs_path = str(ROOT / v["path"])
        gid = hashlib.sha1(abs_path.encode()).hexdigest()[:12]
        name = Path(v["path"]).stem
        row = {"song": name, "rekordbox_path": v["listed"]}
        for method, key in (("melodia", sid), ("game", gid), ("lead", gid), ("bass", gid), ("student", gid)):
            src = ROOT / "out" / method / f"{key}.mid"
            if src.exists():
                (out / method).mkdir(parents=True, exist_ok=True)
                shutil.copy(src, out / method / f"{name}.mid")
                row[method] = f"{method}/{name}.mid"
        rows.append(row)
    (out / "index.json").write_text(json.dumps(sorted(rows, key=lambda r: r["song"]), indent=1, ensure_ascii=False))
    print({m: sum(m in r for r in rows) for m in ("melodia", "game", "lead", "bass", "student")}, "of", len(rows))


if __name__ == "__main__":
    main()
