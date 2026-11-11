# /// script
# requires-python = ">=3.11"
# dependencies = ["pyrekordbox>=0.5", "sqlcipher3-wheels", "mutagen"]
# ///
"""Add a folder of audio files to the local Rekordbox library and a playlist.

Rekordbox has no import command line, so this writes master.db directly with
pyrekordbox. Rekordbox must be closed; master.db is backed up first. Tracks
already in the library are skipped, so re-running after more downloads is safe.
Afterwards open Rekordbox, select the playlist and run Analyze Track, then
export labels with rekordbox_labels.py.

    uv run audio-model/rekordbox_import.py ~/Music/dj-wishlist --dry-run
    uv run audio-model/rekordbox_import.py ~/Music/dj-wishlist
"""

import argparse
import datetime
import logging
import shutil
import subprocess
import sys
import warnings
from pathlib import Path

import mutagen

warnings.filterwarnings("ignore")
logging.disable(logging.WARNING)
from pyrekordbox.masterdb import MasterDatabase  # noqa: E402

RB_DIR = Path.home() / "Library/Pioneer/rekordbox"
AUDIO_EXT = {".mp3", ".m4a", ".wav", ".flac", ".aiff", ".aif"}


def tags(path: Path) -> tuple[str, str, int]:
    """(artist, title, length in s); falls back to "Artist - Title" file names."""
    info = mutagen.File(path, easy=True)
    t = dict(info.tags or {}) if info else {}
    stem_artist, _, stem_title = path.stem.partition(" - ")
    artist = (t.get("artist") or [stem_artist])[0].strip()
    title = (t.get("title") or [stem_title or path.stem])[0].strip()
    return artist, title, int(info.info.length) if info else 0


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("folder", type=Path)
    ap.add_argument("--playlist", default="Blaulicht training")
    ap.add_argument("--rekordbox-dir", type=Path, default=RB_DIR)
    ap.add_argument("--dry-run", action="store_true")
    args = ap.parse_args()

    if subprocess.run(["pgrep", "-x", "rekordbox"], capture_output=True).returncode == 0:
        sys.exit("Rekordbox is running; quit it first (it would overwrite these changes).")

    files = sorted(p for p in args.folder.expanduser().iterdir() if p.suffix.lower() in AUDIO_EXT)
    db_path = args.rekordbox_dir / "master.db"
    if not args.dry_run:
        stamp = datetime.datetime.now().strftime("%Y%m%d-%H%M%S")
        backup = db_path.with_name(f"master.db.before-blaulicht-import-{stamp}")
        shutil.copy2(db_path, backup)
        print(f"backed up master.db to {backup}")

    db = MasterDatabase(path=db_path, unlock=True)
    playlist = db.get_playlist(Name=args.playlist).first()
    if playlist is None and not args.dry_run:
        playlist = db.create_playlist(args.playlist)

    added, skipped = 0, 0
    for path in files:
        if db.get_content(FolderPath=str(path)).count():
            skipped += 1
            continue
        artist, title, length = tags(path)
        print(f"  + {artist} – {title} ({length // 60}:{length % 60:02d})")
        added += 1
        if args.dry_run:
            continue
        artist_row = db.get_artist(Name=artist).first() or db.add_artist(artist)
        content = db.add_content(str(path), Title=title, ArtistID=artist_row.ID, Length=length)
        db.add_to_playlist(playlist, content)

    if not args.dry_run and added:
        db.commit()
    print(
        f"{'would add' if args.dry_run else 'added'} {added} tracks to the collection "
        f"and playlist '{args.playlist}', {skipped} already in the library"
    )


if __name__ == "__main__":
    main()
