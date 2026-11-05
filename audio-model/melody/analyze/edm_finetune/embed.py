"""The four embedding sets EDMFormer trains on: MuQ and MusicFM, 30 s and 420 s windows. Runs on ae-charlie.

    CUDA_DEVICE_ORDER=PCI_BUS_ID CUDA_VISIBLE_DEVICES=0 ../.venv-sf/bin/python -m analyze.edm_finetune.embed --labels weak

Same as EDMFormer's src/data_pipeline scripts: hidden layer 10, one file per 420 s of song, the 30 s
windows concatenated to the same length. Differences: ffmpeg decoding like the stems (the viewer's
timeline), weights from the SongFormer repo (the ones the `songformer` analyzer uses, no download), and
all four trimmed to the same length here, because upstream's dataset silently skips a song if they
differ by more than 4 frames. Writes out/edm_embeddings/<set>/<id>_<start second>.npy, (1, T, 1024) float32.
"""

import argparse
import json
import time
from pathlib import Path

import numpy as np

from analyze import common, songformer

OUT = common.ROOT / "out/edm_embeddings"
SETS = ("musicfm_30s", "muq_30s", "musicfm_420s", "muq_420s")  # the order the checkpoint was trained on
WINDOW, SHORT, SR = 420, 30, songformer.SR
LAYER = 10


def data_dir(labels: str) -> Path:
    return Path(__file__).resolve().parent / "data" / labels


def embed_window(model, x):
    """x: (n) audio tensor of at most 420 s. The four (1, T, 1024) arrays, trimmed to one length."""
    muq = lambda a: model.muq(a.unsqueeze(0), output_hidden_states=True)["hidden_states"][LAYER].float().cpu().numpy()
    fm = lambda a: model.musicfm.get_predictions(a.unsqueeze(0))[1][LAYER].float().cpu().numpy()
    pieces = [x[a: a + SHORT * SR] for a in range(0, x.shape[0], SHORT * SR)]
    pieces = [p for p in pieces if p.shape[0] > 1024]
    out = [np.concatenate([fm(p) for p in pieces], 1), np.concatenate([muq(p) for p in pieces], 1), fm(x), muq(x)]
    n = min(o.shape[1] for o in out)
    if max(o.shape[1] for o in out) - n > 12:
        raise ValueError(f"lengths differ too much: {[o.shape[1] for o in out]}")
    return [o[:, :n] for o in out]


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--labels", required=True, help="dataset under data/, or a folder holding reference.json")
    ap.add_argument("--force", action="store_true")
    args = ap.parse_args()
    folder = Path(args.labels) if "/" in args.labels else data_dir(args.labels)
    want = set(json.loads((folder / "reference.json").read_text()))
    songs = [s for s in common.songs() if s.id in want]
    missing = want - {s.id for s in songs}
    if missing:
        print(f"no stems for {len(missing)} ids, skipped: {sorted(missing)[:5]}", flush=True)

    def done(song):
        return all((OUT / name / f"{song.id}_0.npy").exists() for name in SETS)

    todo = [s for s in songs if args.force or not done(s)]
    print(f"{len(todo)} of {len(songs)} songs to embed", flush=True)
    if not todo:
        return
    import torch

    model = songformer.setup()
    for name in SETS:
        (OUT / name).mkdir(parents=True, exist_ok=True)
    errors, t0 = 0, time.time()
    with torch.no_grad():
        for i, song in enumerate(todo):
            t1 = time.time()
            try:
                x = torch.from_numpy(common.decode(song.audio, SR, 1)[0]).cuda()
                pieces = {}
                for start in range(0, x.shape[0] // SR + 1, WINDOW):
                    seg = x[start * SR: (start + WINDOW) * SR]
                    if seg.shape[0] <= 1024:
                        continue
                    pieces[start] = embed_window(model, seg)
                # all or nothing per song: start 0 is written last and marks it done
                for start in sorted(pieces, reverse=True):
                    for name, arr in zip(SETS, pieces[start]):
                        tmp = OUT / name / f".{song.id}_{start}.tmp.npy"
                        np.save(tmp, arr)
                        tmp.replace(OUT / name / f"{song.id}_{start}.npy")
                print(f"[{i + 1}/{len(todo)}] {song.name}: {time.time() - t1:.1f} s", flush=True)
            except Exception as e:  # one bad song shouldn't stop the rest
                errors += 1
                print(f"[{i + 1}/{len(todo)}] {song.name}: error {e!r}", flush=True)
    print(f"done in {time.time() - t0:.0f} s, {errors} errors", flush=True)


if __name__ == "__main__":
    main()
