#!/usr/bin/env python3
"""Download the tracks from track_wishlist.csv with yt-dlp.

    uv run audio-model/fetch_wishlist.py --dry-run          # show what would be fetched
    uv run audio-model/fetch_wishlist.py --priority 1       # only priority 1
    uv run audio-model/fetch_wishlist.py                    # everything

Files land in ~/Music/dj-wishlist as "Artist - Title.mp3" (or whatever extension
yt-dlp produced). Re-running skips tracks that already have a file, so it is safe
to interrupt and resume. Every successful download is appended to manifest.csv
with the chosen YouTube id/URL so a bad match can be spotted later.
"""

from __future__ import annotations

import argparse
import csv
import json
import re
import subprocess
import sys
import threading
import time
import unicodedata
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

HERE = Path(__file__).resolve().parent
WISHLIST = HERE / "track_wishlist.csv"
OUT_DIR = Path.home() / "Music" / "dj-wishlist"
MANIFEST = OUT_DIR / "manifest.csv"

AUDIO_EXT = {".mp3", ".m4a", ".opus", ".wav", ".flac"}

# Titles that usually mean "not the actual track" unless the wishlist asks for it.
BAD_WORDS = {
    "cover", "covered", "karaoke", "instrumental", "reaction", "tutorial",
    "lesson", "how", "to", "play", "piano", "guitar", "violin", "remake",
    "nightcore", "8d", "slowed", "reverb", "sped", "speed", "bass", "boosted",
    "mashup", "medley", "loop", "hour", "10h", "1h", "shorts", "teaser",
    "preview", "snippet", "ai", "generated", "ncs", "tiktok", "status",
}
GOOD_WORDS = {"official", "audio", "video", "extended", "original", "mix", "hq", "hd"}
# Words that may appear in a correct match without counting as "extra".
NEUTRAL_WORDS = {
    "official", "video", "audio", "music", "mv", "hd", "hq", "4k", "lyrics",
    "lyric", "visualizer", "remaster", "remastered", "feat", "ft", "featuring",
    "from", "the", "album", "single", "clean", "explicit", "version", "full",
}
# Alternate versions that are only wanted when the wishlist version says so.
ALTERNATE_WORDS = {
    "remix", "edit", "mashup", "bootleg", "flip", "vip", "rework", "refix",
    "touch", "reconstruction", "blend", "vs", "cover", "nightcore", "ai",
    "slowed", "sped", "8d", "acoustic", "orchestral", "piano",
}


def slug(text: str) -> str:
    text = unicodedata.normalize("NFKD", text)
    text = "".join(c for c in text if not unicodedata.combining(c))
    text = text.lower().replace("&", " and ")
    return re.sub(r"[^a-z0-9]+", " ", text).strip()


def tokens(text: str) -> set[str]:
    return {t for t in slug(text).split() if len(t) > 1}


def safe_name(text: str) -> str:
    text = text.replace("/", "_").replace("\\", "_").replace(":", " -")
    text = re.sub(r"[<>:\"|?*]", "", text)
    return re.sub(r"\s+", " ", text).strip()[:120]


def score(entry: dict, artist: str, title: str, version: str, expected: float | None = None) -> float:
    """Heuristic: how likely is this YouTube hit the track we want?"""
    cand_title = entry.get("title") or ""
    cand_desc = entry.get("description") or ""
    uploader = entry.get("uploader") or entry.get("channel") or ""
    hay = slug(f"{cand_title} {cand_desc[:200]} {uploader}")
    want = tokens(f"{artist} {title}")
    have = set(hay.split())

    s = 0.0
    s += 6.0 * (len(want & have) / max(len(want), 1))
    # main title should appear verbatim as a phrase
    if slug(title) and slug(title) in slug(cand_title):
        s += 4.0
    if slug(artist).split() and slug(artist).split()[0] in slug(cand_title):
        s += 1.5

    duration = entry.get("duration") or 0
    if duration:
        if 90 <= duration <= 900:
            s += 1.5
        else:
            s -= 4.0
    else:
        s -= 1.0

    # If we know the real track length (from the release metadata), use it:
    # it is by far the best signal for extended vs radio edit vs DJ set.
    if expected and duration:
        delta = abs(duration - expected)
        if delta <= 6:
            s += 8.0
        elif delta <= 15:
            s += 3.0
        elif delta <= 40:
            s -= 2.0
        else:
            s -= 6.0

    words = set(slug(cand_title).split())
    s -= 1.2 * len(words & BAD_WORDS)
    s += 0.35 * len(words & GOOD_WORDS)

    # Penalise extra words (alternate versions, guest artists, mashup names).
    want_words = set(slug(f"{artist} {title} {version}").split())
    extra = words - want_words - NEUTRAL_WORDS - GOOD_WORDS
    s -= 0.9 * len(extra)

    alt = words & ALTERNATE_WORDS
    version_words = set(slug(version).split())
    s -= 2.5 * len(alt - version_words)

    # When the wishlist asks for a specific version (e.g. "Extended Mix"),
    # that word must appear in the candidate title; don't reward a radio edit.
    meaningful = version_words - {"original", "mix", "version", "the"}
    if meaningful:
        if meaningful <= words:
            s += 4.0
        else:
            s -= 3.0
    elif slug(cand_title) in {slug(f"{artist} {title}"), slug(f"{artist} - {title}"), slug(title)}:
        s += 5.0

    if version:
        v = set(slug(version).split()) - {"original", "mix"}
        if v and v <= words | set(slug(cand_desc[:200]).split()):
            s += 2.0
        elif "extended" in v and "extended" not in words:
            s += 0.75  # extended is rare on YT; don't punish a clean original too hard

    uploader_slug = slug(uploader)
    first_artist = slug(artist).split("&")[0].split(",")[0].strip().split()
    if first_artist and first_artist[0] in uploader_slug:
        s += 1.0
    if any(x in uploader_slug for x in ("topic", "vevo", "records", "official")):
        s += 0.75

    return s


def search(artist: str, title: str, version: str, n: int = 12) -> list[dict]:
    query = f"ytsearch{n}:{artist} {title} {version}".strip()
    cmd = ["yt-dlp", "--no-warnings", "--ignore-errors", "--flat-playlist",
           "--dump-single-json", query]
    for attempt in range(3):
        try:
            proc = subprocess.run(cmd, capture_output=True, text=True, timeout=90)
            data = json.loads(proc.stdout)
            entries = [e for e in data.get("entries") or [] if e.get("id")]
            if entries:
                return entries
        except (subprocess.TimeoutExpired, json.JSONDecodeError):
            pass
        time.sleep(1.5 * (attempt + 1))
    return []


def base_name(artist: str, title: str, version: str = "") -> str:
    suffix = f" ({version})" if version else ""
    return safe_name(f"{artist} - {title}{suffix}")


def download(entry: dict, artist: str, title: str, version: str, out_dir: Path) -> Path | None:
    base = base_name(artist, title, version)
    pattern = str(out_dir / f"{base}.%(ext)s")
    url = entry.get("webpage_url") or f"https://www.youtube.com/watch?v={entry['id']}"
    cmd = [
        "yt-dlp",
        "--no-warnings",
        "--no-playlist",
        "-f", "bestaudio/best",
        "--extract-audio",
        "--audio-format", "mp3",
        "--audio-quality", "0",
        "--embed-metadata",
        "-o", pattern,
        url,
    ]
    proc = subprocess.run(cmd, capture_output=True, text=True)
    if proc.returncode != 0:
        print(f"    download failed: {proc.stderr.strip().splitlines()[-1] if proc.stderr.strip() else '?'}")
        return None
    for ext in (".mp3", ".m4a", ".opus", ".wav", ".flac"):
        p = out_dir / f"{base}{ext}"
        if p.exists():
            return p
    return None


def tag(path: Path, artist: str, title: str, version: str, url: str) -> None:
    tmp = path.with_suffix(".tag" + path.suffix)
    cmd = [
        "ffmpeg", "-y", "-loglevel", "error", "-i", str(path), "-c", "copy",
        "-metadata", f"artist={artist}",
        "-metadata", f"title={title}",
        "-metadata", f"comment={version + ' | ' if version else ''}{url}",
        str(tmp),
    ]
    if subprocess.run(cmd).returncode == 0:
        tmp.replace(path)


def existing(out_dir: Path, artist: str, title: str, version: str = "") -> Path | None:
    base = base_name(artist, title, version)
    for ext in AUDIO_EXT:
        p = out_dir / f"{base}{ext}"
        if p.exists() and p.stat().st_size > 100_000:
            return p
    return None


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--csv", type=Path, default=WISHLIST, help="wishlist CSV to read")
    ap.add_argument("--dry-run", action="store_true")
    ap.add_argument("--priority", type=int, action="append", help="only these priorities (1,2,3)")
    ap.add_argument("--out", type=Path, default=OUT_DIR)
    ap.add_argument("--min-score", type=float, default=5.0)
    ap.add_argument("--jobs", type=int, default=4)
    ap.add_argument("--log", type=Path, default=HERE / "data" / "fetch_wishlist.log")
    ap.add_argument("--status", action="store_true", help="just report how many are already downloaded")
    args = ap.parse_args()

    args.log.parent.mkdir(parents=True, exist_ok=True)
    logger = args.log.open("a", encoding="utf-8")

    rows = list(csv.DictReader(args.csv.open(encoding="utf-8")))
    if args.priority:
        rows = [r for r in rows if int(r["priority"] or 0) in args.priority]

    if args.status:
        real = [r for r in rows if (r.get("artist") or "").strip() and not r["artist"].strip().startswith("(")]
        have = [r for r in real if existing(args.out, r["artist"].strip(), r["title"].strip(), (r.get("version") or "").strip())]
        bar_len = 40
        filled = int(bar_len * len(have) / max(len(real), 1))
        print(f"{'█' * filled}{'·' * (bar_len - filled)}  {len(have)}/{len(real)}")
        missing = [f"{r['artist']} - {r['title']}" for r in real if r not in have]
        for m in missing:
            print(f"  missing: {m}")
        logger.close()
        return 0

    args.out.mkdir(parents=True, exist_ok=True)
    manifest_path = args.out / "manifest.csv"
    manifest_exists = manifest_path.exists()
    manifest = manifest_path.open("a", newline="", encoding="utf-8")
    writer = csv.writer(manifest)
    if not manifest_exists:
        writer.writerow(["priority", "artist", "title", "version", "file", "score", "video_id", "url"])

    lock = threading.Lock()
    stats = {"ok": 0, "skipped": 0, "failed": 0}
    failed: list[str] = []
    total = len(rows)
    done = 0

    def emit(lines: list[str]) -> None:
        """Print one track's log as a single block so parallel workers don't interleave."""
        nonlocal done
        with lock:
            done += 1
            prefix = f"[{done:3d}/{total}] "
            print(prefix + ("\n" + " " * len(prefix)).join(lines), flush=True)
            for line in lines:
                logger.write(line + "\n")
            logger.flush()

    def fail(artist: str, title: str) -> None:
        with lock:
            stats["failed"] += 1
            failed.append(f"{artist} - {title}")

    def handle(row: dict) -> None:
        artist = (row.get("artist") or "").strip()
        title = (row.get("title") or "").strip()
        version = (row.get("version") or "").strip()
        priority = row.get("priority") or ""
        expected = float(row["duration"]) if (row.get("duration") or "").strip().isdigit() else None

        if not artist or not title:
            return
        if artist.startswith("("):
            emit([f"skip  {artist} — {title} (needs manual recording)"])
            with lock:
                stats["skipped"] += 1
            return

        hit = existing(args.out, artist, title, version)
        if hit:
            emit([f"have  {artist} - {title}"])
            with lock:
                stats["skipped"] += 1
            return

        entries = search(artist, title, version)
        if not entries:
            emit([f"find  {artist} - {title} ({version})", "    no results"])
            fail(artist, title)
            return

        ranked = sorted(((score(e, artist, title, version, expected), e) for e in entries), key=lambda x: -x[0])
        best_score, best = ranked[0]
        lines = [
            f"find  {artist} - {title} ({version})",
            f"    -> {best_score:4.1f}  {best.get('title')}  [{best.get('id')}, {best.get('duration')}s]",
        ]
        if best_score < args.min_score:
            emit(lines + ["    below --min-score, skipping (add manually)"])
            fail(artist, title)
            return

        if args.dry_run:
            emit(lines)
            with lock:
                stats["ok"] += 1
            return

        path = download(best, artist, title, version, args.out)
        if not path:
            emit(lines + ["    download failed"])
            fail(artist, title)
            return
        tag(path, artist, f"{title}{f' ({version})' if version else ''}", "", best.get("webpage_url") or "")
        with lock:
            writer.writerow([priority, artist, title, version, path.name, f"{best_score:.1f}", best.get("id"), best.get("webpage_url") or ""])
            manifest.flush()
            stats["ok"] += 1
        emit(lines + [f"    saved {path.name}"])

    with ThreadPoolExecutor(max_workers=max(1, args.jobs)) as pool:
        list(pool.map(handle, rows))

    manifest.close()
    logger.close()
    print(f"\n{stats['ok']} ok, {stats['skipped']} skipped, {stats['failed']} failed (of {len(rows)} rows)")
    if failed:
        print("failed:")
        for f in failed:
            print(f"  - {f}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
