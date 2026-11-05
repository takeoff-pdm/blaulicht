#!/usr/bin/env python3
"""Labels and the fixed split for the EDM fine-tune. Runs on the Mac, standard library only.

    python3 audio-model/melody/analyze/edm_finetune/build_dataset.py --labels weak   # needs the viewer running
    python3 audio-model/melody/analyze/edm_finetune/build_dataset.py --labels hand
    sh audio-model/melody/analyze/sync.sh push                                       # carries data/ along

Writes data/<labels>/ next to this file:
    labels.jsonl    {"id": ..., "labels": [[start, "label"], ..., [end, "end"]]}, EDMFormer's format
    train.txt, val.txt, holdout.txt     song ids, one per line
    reference.json  what the evaluator compares with: sections, drops, beat grid
    manifest.json   counts, label shares, what was left out and why

weak: the viewer's section rule (viz/sections.py, doc.show). Intro, Build, Drop, Breakdown, Outro map
to their EDM labels. Groove has no EDM label, so it becomes NO_LABEL, which `edm_dataset.py` leaves out
of the label loss: the rule doesn't say whether a Groove is a drop or a breakdown, so the model shouldn't
be taught either. Songs where the rule finds no drop are left out of training (their labels mostly say
the rule failed); the evaluator looks at them separately.

hand: labels/<id>.json with status "done", in the labelling view's format. Gaps between sections
become NO_LABEL. Files with an unknown label or overlapping sections stop the build.

The split depends only on the song's title, so it never changes when more songs get labels and hand and
weak share it. Versions of one song ("X" and "X (Extended Mix)") stay on the same side.
"""

import argparse
import hashlib
import json
import re
import sys
import urllib.request
from collections import Counter
from multiprocessing.pool import ThreadPool
from pathlib import Path

HERE = Path(__file__).resolve().parent
LABELS_DIR = HERE.parents[2] / "data/melody/labels"
EDM_LABELS = ("intro", "buildup", "drop", "breakdown", "outro", "silence")
RULE_TO_EDM = {"Intro": "intro", "Build": "buildup", "Drop": "drop", "Breakdown": "breakdown", "Outro": "outro", "Groove": "NO_LABEL"}
NO_LABEL = "NO_LABEL"


def title_key(name: str) -> str:
    """The name without track number and anything in brackets: versions of a song share it."""
    name = re.sub(r"\(.*?\)|\[.*?\]", " ", name)
    name = re.sub(r"^\s*\d+[.\-\s]+", "", name)
    return re.sub(r"\s+", " ", name).strip().lower()


def split_of(name: str) -> str:
    """holdout about 20%, val about 8%, train about 72%."""
    h = int(hashlib.sha1(("edm-holdout-v1|" + title_key(name)).encode()).hexdigest(), 16)
    if h % 5 == 0:
        return "holdout"
    return "val" if (h // 5) % 10 == 0 else "train"


def fetch(api: str, path: str, timeout: int = 600):
    return json.load(urllib.request.urlopen(api + path, timeout=timeout))


def grid(doc: dict) -> dict:
    """Seconds per beat and downbeat times from a viewer document."""
    b = doc.get("beats") or {}
    beats, inbar = b.get("beats") or [], b.get("beat_in_bar") or []
    if len(beats) < 2:
        return {"beat": None, "downbeats": []}
    return {"beat": round((beats[-1] - beats[0]) / (len(beats) - 1), 4),
            "downbeats": [round(t, 3) for t, k in zip(beats, inbar) if k == 1]}


def to_jsonl_labels(sections: list, end: float) -> list:
    """Sorted [start, stop, label] sections to [[start, label], ..., [end, "end"]]: gaps become NO_LABEL,
    neighbours with the same label merge."""
    out = []
    pos = 0.0
    for a, b, label in sections:
        if a > pos + 1e-3:
            out.append([round(pos, 3), NO_LABEL])
        out.append([round(a, 3), label])
        pos = b
    if pos < end - 1e-3:
        out.append([round(pos, 3), NO_LABEL])
    merged = [x for i, x in enumerate(out) if i == 0 or x[1] != out[i - 1][1]]
    return merged + [[round(end, 3), "end"]]


def weak_songs(api: str) -> tuple[list, dict]:
    songs = [s for s in fetch(api, "/api/songs") if s["genre"] == "electronic" and s["has_beats"]]
    docs = ThreadPool(3).map(lambda s: fetch(api, f"/api/analysis/{s['id']}"), songs)
    rows, excluded = [], {}
    for doc in docs:
        sections, drops = doc["show"]["sections"], doc["show"]["drops"]
        secs = [[0.0 if i == 0 else s["start"], s["end"], RULE_TO_EDM[s["kind"]]] for i, s in enumerate(sections)]
        if secs:
            secs[-1][1] = doc["duration"]  # the rule stops at the last downbeat
        row = {"id": doc["id"], "name": doc["name"], "duration": doc["duration"], "sections": secs,
               "drops": [d["time"] for d in drops], **grid(doc)}
        if not drops:
            row["left_out"] = "the rule finds no drop"
            excluded[doc["id"]] = doc["name"]
        rows.append(row)
    return rows, excluded


def hand_songs(folder: Path, api: str) -> tuple[list, dict]:
    rows, excluded = [], {}
    for path in sorted(folder.glob("*.json")):
        d = json.loads(path.read_text())
        if d.get("status") != "done":
            excluded[d.get("id", path.stem)] = f"status {d.get('status')}"
            continue
        secs = sorted([float(s["start"]), float(s["end"]), s["label"]] for s in d["sections"])
        for a, b, label in secs:
            if label not in EDM_LABELS or b <= a:
                sys.exit(f"{path}: bad section {a} {b} {label}")
        for (_, b, _), (a, _, _) in zip(secs, secs[1:]):
            if a < b - 1e-3:
                sys.exit(f"{path}: sections overlap at {a}")
        if not secs:
            excluded[d["id"]] = "no sections"
            continue
        row = {"id": d["id"], "name": d.get("name") or d["id"], "duration": secs[-1][1], "sections": secs,
               "drops": [a for i, (a, _, label) in enumerate(secs) if label == "drop" and (i == 0 or secs[i - 1][2] != "drop")],
               "beat": None, "downbeats": [], "fx": len(d.get("fx") or [])}
        if api:
            try:
                row.update(grid(fetch(api, f"/api/analysis/{d['id']}")))
            except OSError as e:
                print(f"no beat grid for {d['id']}: {e}", file=sys.stderr)
        rows.append(row)
    return rows, excluded


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--labels", choices=("hand", "weak"), required=True)
    ap.add_argument("--labels-dir", type=Path, default=LABELS_DIR, help="hand: folder of <id>.json")
    ap.add_argument("--api", default="http://127.0.0.1:8766", help="the viewer; hand can do without (empty): no beat grids then")
    ap.add_argument("--out", type=Path, help="default data/<labels> next to this file")
    args = ap.parse_args()

    if args.labels == "weak":
        rows, excluded = weak_songs(args.api)
    else:
        rows, excluded = hand_songs(args.labels_dir, args.api)
    out = args.out or HERE / "data" / args.labels
    out.mkdir(parents=True, exist_ok=True)

    splits = {"train": [], "val": [], "holdout": []}
    shares, lines = Counter(), []
    for row in rows:
        row["split"] = split_of(row["name"])
        if "left_out" in row:
            continue
        splits[row["split"]].append(row["id"])
        labels = to_jsonl_labels(row["sections"], row["duration"])
        lines.append(json.dumps({"id": row["id"], "labels": labels}, ensure_ascii=False))
        for a, b, label in row["sections"]:
            shares[label] += b - a
    if not splits["holdout"]:
        sys.exit("no labelled song falls into the fixed hold-out: nothing to evaluate on yet")
    if not splits["train"]:
        sys.exit("no labelled song falls into the training split")

    (out / "labels.jsonl").write_text("\n".join(lines) + "\n")
    for name, ids in splits.items():
        (out / f"{name}.txt").write_text("\n".join(sorted(ids)) + "\n")
    (out / "reference.json").write_text(json.dumps({r["id"]: r for r in rows}, ensure_ascii=False, separators=(",", ":")))
    total = sum(shares.values()) or 1
    manifest = {"labels": args.labels, "songs": {k: len(v) for k, v in splits.items()}, "left_out": excluded,
                "time_share": {k: round(v / total, 3) for k, v in shares.most_common()}}
    (out / "manifest.json").write_text(json.dumps(manifest, ensure_ascii=False, indent=1))
    print(json.dumps(manifest, ensure_ascii=False, indent=1))
    if len(splits["train"]) < 50:
        print(f"only {len(splits['train'])} training songs: fine to test the pipeline, too few to learn much", file=sys.stderr)


if __name__ == "__main__":
    main()
