# /// script
# requires-python = ">=3.11"
# dependencies = ["numpy"]
# ///
"""Turn labelled tracks into per-track feature/target files for training.

    uv run audio-model/export_features.py \
        --labels audio-model/data/rekordbox_labels.json \
        --out audio-model/data/features

Writes one `<id>.npz` per track (mel float16, beat, downbeat) and an
`index.json` with the front-end config and track metadata. Tracks already
exported with the current front-end version are skipped.
"""

import argparse
import hashlib
import json
import sys
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

import numpy as np

import features


def track_id(path: str) -> str:
    return hashlib.sha1(path.encode()).hexdigest()[:12]


def export(record: dict, out_dir: Path, frame_rate: int) -> dict:
    out = out_dir / f"{track_id(record['path'])}.npz"
    if not out.exists():
        mel = features.extract(features.decode(record["path"]))
        beat, downbeat = features.targets(
            record["beats_s"], record["beat_in_bar"], len(mel), frame_rate
        )
        tmp = out.with_suffix(".tmp.npz")
        np.savez_compressed(tmp, mel=mel.astype(np.float16), beat=beat, downbeat=downbeat)
        tmp.rename(out)
    with np.load(out) as data:
        n_frames = len(data["beat"])
    return {
        "id": out.stem,
        "file": out.name,
        "frames": n_frames,
        **{k: record[k] for k in ("path", "title", "artist", "genre", "bpm", "variable_tempo")},
    }


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--labels", type=Path, required=True)
    ap.add_argument("--out", type=Path, required=True)
    ap.add_argument("--jobs", type=int, default=8)
    args = ap.parse_args()

    config = features.info()
    args.out.mkdir(parents=True, exist_ok=True)
    index_path = args.out / "index.json"
    if index_path.exists():
        old = json.loads(index_path.read_text())["frontend"]
        if old["version"] != config["version"]:
            sys.exit(
                f"{args.out} holds front-end v{old['version']} features, current is "
                f"v{config['version']}; export into a new directory"
            )

    records = json.loads(args.labels.read_text())
    with ThreadPoolExecutor(args.jobs) as pool:
        tracks = list(pool.map(lambda r: export(r, args.out, config["frame_rate"]), records))

    index_path.write_text(json.dumps({"frontend": config, "tracks": tracks}, indent=1))
    hours = sum(t["frames"] for t in tracks) / config["frame_rate"] / 3600
    print(f"{len(tracks)} tracks, {hours:.1f} h in {args.out}", file=sys.stderr)


if __name__ == "__main__":
    main()
