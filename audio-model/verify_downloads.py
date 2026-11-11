#!/usr/bin/env python3
"""Check downloaded files against the durations in a wishlist CSV.

    uv run audio-model/verify_downloads.py --csv audio-model/mika_heggemann.csv \
        --out ~/Music/mika-heggemann

Prints missing files and files whose real duration is off by more than
--tolerance seconds (default 8), which usually means the wrong video was
downloaded (radio edit instead of extended, a DJ set, a cover, ...).
"""

from __future__ import annotations

import argparse
import csv
import re
import subprocess
import sys
from pathlib import Path


def base_name(artist: str, title: str, version: str) -> str:
    suffix = f" ({version})" if version else ""
    text = f"{artist} - {title}{suffix}"
    text = text.replace("/", "_").replace("\\", "_").replace(":", " -")
    text = re.sub(r"[<>:\"|?*]", "", text)
    return re.sub(r"\s+", " ", text).strip()[:120]


def duration(path: Path) -> float | None:
    out = subprocess.run(
        ["ffprobe", "-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0", str(path)],
        capture_output=True, text=True,
    )
    try:
        return float(out.stdout.strip())
    except ValueError:
        return None


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--csv", type=Path, required=True)
    ap.add_argument("--out", type=Path, required=True)
    ap.add_argument("--tolerance", type=float, default=8.0)
    args = ap.parse_args()

    rows = [r for r in csv.DictReader(args.csv.open(encoding="utf-8"))
            if not (r.get("artist") or "").strip().startswith("(")]
    missing, off, ok = [], [], 0

    for r in rows:
        artist = r["artist"].strip()
        title = r["title"].strip()
        version = (r.get("version") or "").strip()
        expected = (r.get("duration") or "").strip()

        path = args.out / f"{base_name(artist, title, version)}.mp3"
        if not path.exists():
            missing.append(f"{artist} - {title}{f' ({version})' if version else ''}")
            continue
        got = duration(path)
        if expected.isdigit() and got is not None:
            delta = abs(got - int(expected))
            if delta > args.tolerance:
                off.append((f"{artist} - {title}{f' ({version})' if version else ''}", int(expected), round(got, 1)))
                continue
        ok += 1

    print(f"{ok}/{len(rows)} files match the expected duration")
    if off:
        print(f"\n{len(off)} wrong file(s):")
        for name, exp, got in off:
            print(f"  {name}: expected {exp}s, got {got}s")
    if missing:
        print(f"\n{len(missing)} missing:")
        for m in missing:
            print(f"  {m}")
    return 1 if off or missing else 0


if __name__ == "__main__":
    sys.exit(main())
