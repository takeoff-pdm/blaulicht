# /// script
# requires-python = ">=3.11"
# dependencies = ["pyrekordbox>=0.5", "sqlcipher3-wheels", "numpy"]
# ///
"""Export Rekordbox beat grids as beat/downbeat training labels.

Reads a copy of the local Rekordbox master.db (Rekordbox may stay open) and
the per-track ANLZ files, and writes one JSON record per track with an
analyzed grid and an audio file that still exists.

    uv run audio-model/rekordbox_labels.py --out audio-model/data/rekordbox_labels.json
"""

import argparse
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


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--rekordbox-dir", type=Path, default=RB_DIR)
    ap.add_argument("--out", type=Path, required=True)
    ap.add_argument(
        "--min-seconds",
        type=float,
        default=30.0,
        help="skip shorter files (loop samples)",
    )
    args = ap.parse_args()

    with tempfile.TemporaryDirectory() as tmp:
        tmp = Path(tmp)
        shutil.copy2(args.rekordbox_dir / "master.db", tmp / "master.db")
        # ANLZ paths resolve relative to the database directory.
        (tmp / "share").symlink_to(args.rekordbox_dir / "share")
        db = MasterDatabase(path=tmp / "master.db", unlock=True)
        records, skipped = export(db, args.min_seconds)

    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(records))

    seconds = sum(r["beats_s"][-1] - r["beats_s"][0] for r in records)
    variable = sum(r["variable_tempo"] for r in records)
    print(f"wrote {len(records)} tracks to {args.out}", file=sys.stderr)
    print(
        f"  {seconds / 3600:.1f} h, {sum(len(r['beats_s']) for r in records)} beats, "
        f"{variable} with variable tempo",
        file=sys.stderr,
    )
    print(f"  skipped: {skipped}", file=sys.stderr)


def export(db: MasterDatabase, min_seconds: float) -> tuple[list[dict], dict[str, int]]:
    records = []
    skipped = {"no_grid": 0, "missing_audio": 0, "too_short": 0}
    for track in db.get_content():
        dat = next(
            (f for p, f in db.read_anlz_files(track).items() if p.suffix == ".DAT"),
            None,
        )
        if dat is None or "PQTZ" not in dat.tag_types:
            skipped["no_grid"] += 1
            continue
        beat_in_bar, bpm, beats_s = (np.asarray(x) for x in dat.get("beat_grid"))
        if len(beats_s) < 2:
            skipped["no_grid"] += 1
            continue
        if not Path(track.FolderPath or "").exists():
            skipped["missing_audio"] += 1
            continue
        if beats_s[-1] - beats_s[0] < min_seconds:
            skipped["too_short"] += 1
            continue

        records.append(
            {
                "path": track.FolderPath,
                "title": track.Title,
                "artist": track.ArtistName,
                "genre": track.GenreName,
                "bpm": (track.BPM or 0) / 100,
                # Beat times in seconds from the start of the decoded audio.
                "beats_s": beats_s.astype(float).round(4).tolist(),
                # 1..4; downbeats are where this is 1.
                "beat_in_bar": beat_in_bar.astype(int).tolist(),
                "variable_tempo": bool(len(np.unique(np.round(bpm, 2))) > 1),
            }
        )
    return records, skipped


if __name__ == "__main__":
    main()
