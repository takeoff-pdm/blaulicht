"""Run one analyzer over every song, skipping songs already analysed at its current version.

    python -m analyze.run stereo --jobs 16                    # CPU analyzers: a process pool
    CUDA_VISIBLE_DEVICES=2 python -m analyze.run drumparts --shard 0 --num-shards 2
    python -m analyze.run risers --only 58e556af6855 --force  # one song, recompute

Run from the code directory (`/localhome/sokrates/melody/code`), in the analyzer's venv.
"""

import argparse
import importlib
import sys
import time
import traceback
from multiprocessing import Pool

from analyze import common

_mod = None
_state = None


def _init(name: str) -> None:
    global _mod, _state
    _mod = importlib.import_module(f"analyze.{name}")
    _state = _mod.setup() if hasattr(_mod, "setup") else None


def _one(song: common.Song) -> tuple[str, str]:
    t0 = time.time()
    try:
        doc = _mod.analyse(song, _state)
        common.save(_mod.NAME, _mod.VERSION, song, doc)
        return song.name, f"ok {time.time() - t0:.1f} s"
    except Exception:
        return song.name, "error: " + traceback.format_exc(limit=3).strip().replace("\n", " | ")


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("analyzer")
    ap.add_argument("--jobs", type=int, default=1)
    ap.add_argument("--shard", type=int, default=0)
    ap.add_argument("--num-shards", type=int, default=1)
    ap.add_argument("--only", default="", help="comma-separated song ids")
    ap.add_argument("--limit", type=int, default=0)
    ap.add_argument("--force", action="store_true")
    args = ap.parse_args()
    mod = importlib.import_module(f"analyze.{args.analyzer}")
    songs = common.songs()
    if args.only:
        keep = set(args.only.split(","))
        songs = [s for s in songs if s.id in keep]
    songs = [s for s in songs if int(s.id, 16) % args.num_shards == args.shard]
    todo = [s for s in songs if args.force or not common.done(mod.NAME, mod.VERSION, s)]
    if args.limit:
        todo = todo[: args.limit]
    print(f"{mod.NAME} v{mod.VERSION}: {len(todo)} of {len(songs)} songs to do", flush=True)
    t0, errors = time.time(), 0
    if getattr(mod, "GPU", False) or args.jobs <= 1:
        _init(args.analyzer)
        results = map(_one, todo)
    else:
        pool = Pool(args.jobs, initializer=_init, initargs=(args.analyzer,))
        results = pool.imap_unordered(_one, todo)
    for i, (name, msg) in enumerate(results):
        errors += msg.startswith("error")
        print(f"[{i + 1}/{len(todo)}] {name}: {msg}", flush=True)
    print(f"done in {time.time() - t0:.0f} s, {errors} errors", flush=True)
    sys.exit(1 if errors else 0)


if __name__ == "__main__":
    main()
