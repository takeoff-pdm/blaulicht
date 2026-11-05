"""Distil Melodia into the streaming melody student.

    python train_melody.py --out runs/la30 --lookahead 30

Training crops come from four groups (weights pick how often each is sampled): FMA clips, the
dance tracks and MUSDB18 train mixes of the stem work (their teacher files, row 0 = mono mix),
and the Rekordbox songs. Every crop's target is Melodia's offline f0 on the same audio.

Validation every --eval-every steps on whole tracks: held-out Rekordbox songs (every song whose
id % 6 == 0, also removed from the dance set) and the MUSDB18 test mixes, against Melodia.
Metrics follow MIREX melody extraction on the 10 ms grid: overall accuracy (OA), raw pitch and
raw chroma accuracy (RPA, RCA, within 50 cents), voicing recall (VR) and false alarm (VFA).
"""

import argparse
import json
import math
import random
import time
from pathlib import Path

import numpy as np
import torch

from melody_common import HOP, ROOT, TEACHER, is_test_song, metrics, summarize
from student_melody import MelodyConfig, MelodyStudent, decode, targets

def song_names() -> dict:
    import hashlib

    out = {}
    for line in (ROOT / "songs_remote.txt").read_text().splitlines():
        if line.strip():
            out[hashlib.sha1(line.strip().encode()).hexdigest()[:12]] = Path(line.strip()).name
    return out


def collect():
    """{group: [(audio path, row or None, label path)]} for training, {set: [...]} for validation."""
    names = song_names()
    test_names = {n for s, n in names.items() if is_test_song(s)}
    groups = {"fma": [], "edm": [], "musdb": [], "songs": []}
    valid = {"songs": [], "musdb_test": []}
    for set_name, group in (("fma", "fma"), ("edm", "edm"), ("musdb_train", "musdb")):
        index = {}
        for tsv in (TEACHER / set_name).glob("index.*.tsv"):
            for line in tsv.read_text().splitlines():
                sid, path = line.split("\t", 1)
                index[sid] = Path(path).name
        for lab in sorted((ROOT / "data/labels" / set_name).glob("*.npy")):
            if group == "edm" and index.get(lab.stem) in test_names:
                continue
            groups[group].append((TEACHER / set_name / lab.name, 0, lab))
    for lab in sorted((ROOT / "data/labels/songs").glob("*.npy")):
        item = (ROOT / "data/audio/songs" / lab.name, None, lab)
        (valid["songs"] if is_test_song(lab.stem) else groups["songs"]).append(item)
    for lab in sorted((ROOT / "data/labels/musdb_test").glob("*.npy")):
        valid["musdb_test"].append((ROOT / "data/audio/musdb_test" / lab.name, None, lab))
    return groups, valid


def load_audio(path: Path, row, start=0, n=None):
    a = np.load(path, mmap_mode="r")
    a = a[row] if row is not None else a
    stop = len(a) if n is None else start + n
    return np.asarray(a[start:stop], np.float32)


class Crops(torch.utils.data.IterableDataset):
    def __init__(self, groups: dict, weights: dict, frames: int, seed: int):
        self.groups = {g: items for g, items in groups.items() if items and weights.get(g, 0) > 0}
        self.weights = weights
        self.frames, self.seed = frames, seed

    def __iter__(self):
        info = torch.utils.data.get_worker_info()
        rng = random.Random(self.seed + (info.id if info else 0) + int(time.time()))
        names = list(self.groups)
        w = [self.weights[g] for g in names]
        n = (self.frames - 1) * HOP
        last_scan = time.time()
        while True:
            if time.time() - last_scan > 600:  # pick up clips labelled since the start
                groups, _ = collect()
                self.groups.update({g: v for g, v in groups.items() if g in self.groups and v})
                last_scan = time.time()
            items = self.groups[rng.choices(names, w)[0]]
            path, row, lab_path = rng.choice(items)
            lab = np.load(lab_path, mmap_mode="r")
            if len(lab) <= self.frames:
                continue
            g0 = rng.randrange(0, len(lab) - self.frames)
            x = load_audio(path, row, g0 * HOP, n)
            if len(x) < n or not np.isfinite(x).all():
                continue
            gain = 10 ** (rng.uniform(-9, 3) / 20)
            yield torch.from_numpy(x * gain), torch.from_numpy(np.asarray(lab[g0: g0 + self.frames], np.float32))


@torch.no_grad()
def predict(model, wav: np.ndarray, device, chunk_s=120) -> np.ndarray:
    """Whole-track f0 at 100 fps. Long tracks run in overlapping chunks so memory stays bounded;
    each chunk keeps 20 s of context on both sides, far more than the receptive field."""
    n_frames = len(wav) // HOP + 1
    out = np.zeros(n_frames, np.float32)
    ctx, step = 2000, chunk_s * 100
    for g in range(0, n_frames, step):
        a, b = max(0, g - ctx), min(n_frames, g + step + ctx)
        x = torch.from_numpy(wav[a * HOP: (b - 1) * HOP + 1]).to(device)[None]
        with torch.autocast("cuda", torch.bfloat16):
            f0 = decode(model(x))[0].float().cpu().numpy()
        out[g: min(n_frames, g + step)] = f0[g - a: g - a + min(step, n_frames - g)]
    return out


def evaluate(model, valid: dict, device, eager=None) -> dict:
    """`eager`: the uncompiled blocks forward; whole tracks have varying lengths, which would
    make the compiled (static-shape) graph recompile for every track."""
    model.eval()
    compiled = model.blocks.forward
    if eager is not None:
        model.blocks.forward = eager
    res = {}
    for name, items in valid.items():
        ms = []
        for path, row, lab_path in items:
            wav = load_audio(path, row)
            ms.append(metrics(np.load(lab_path), predict(model, wav, device)))
        if ms:
            res[name] = summarize(ms)
    model.blocks.forward = compiled
    model.train()
    return res


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", type=Path, required=True)
    ap.add_argument("--lookahead", type=int, default=0)
    ap.add_argument("--channels", type=int, default=64)
    ap.add_argument("--steps", type=int, default=30000)
    ap.add_argument("--batch", type=int, default=8)
    ap.add_argument("--seconds", type=float, default=6.0)
    ap.add_argument("--lr", type=float, default=1e-3)
    ap.add_argument("--sigma", type=float, default=1.0, help="target width in 20-cent bins")
    ap.add_argument("--w-fma", type=float, default=1.0)
    ap.add_argument("--w-edm", type=float, default=1.0)
    ap.add_argument("--w-musdb", type=float, default=0.5)
    ap.add_argument("--w-songs", type=float, default=1.0)
    ap.add_argument("--workers", type=int, default=6)
    ap.add_argument("--eval-every", type=int, default=5000)
    ap.add_argument("--no-compile", action="store_true")
    args = ap.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    torch.manual_seed(0)
    device = "cuda"

    groups, valid = collect()
    print({g: len(v) for g, v in groups.items()}, {g: len(v) for g, v in valid.items()}, flush=True)
    cfg = MelodyConfig(lookahead=args.lookahead, channels=args.channels)
    model = MelodyStudent(cfg).to(device)
    print(f"params {sum(p.numel() for p in model.parameters()) / 1e6:.3f}M, "
          f"{model.macs_per_frame() / 1e6:.1f}M MAC/frame, right pads {cfg.right_pads()}", flush=True)

    frames = int(args.seconds * 100) + 1
    weights = {"fma": args.w_fma, "edm": args.w_edm, "musdb": args.w_musdb, "songs": args.w_songs}
    loader = torch.utils.data.DataLoader(Crops(groups, weights, frames, 0), batch_size=args.batch,
                                         num_workers=args.workers, persistent_workers=True)
    it = iter(loader)
    x, _ = next(it)
    model.fit_feature_stats(torch.cat([next(it)[0] for _ in range(8)]).to(device))

    eager = model.blocks.forward
    if not args.no_compile:
        model.blocks.forward = torch.compile(model.blocks.forward, dynamic=False)
    opt = torch.optim.AdamW(model.parameters(), lr=args.lr, weight_decay=1e-2)
    warm = 1000
    sched = torch.optim.lr_scheduler.LambdaLR(
        opt, lambda s: min(1, (s + 1) / warm) * 0.5 * (1 + math.cos(math.pi * min(1, s / args.steps))))
    log = open(args.out / "metrics.jsonl", "a")
    best, t0, run_loss = -1.0, time.time(), 0.0
    for step in range(1, args.steps + 1):
        x, f0 = next(it)
        x, f0 = x.to(device, non_blocking=True), f0.to(device, non_blocking=True)
        with torch.autocast("cuda", torch.bfloat16):
            logits = model(x)
        t = targets(f0[:, : logits.shape[1]], args.sigma)
        loss = -(t * logits.float().log_softmax(-1)).sum(-1).mean()
        opt.zero_grad(set_to_none=True)
        loss.backward()
        torch.nn.utils.clip_grad_norm_(model.parameters(), 1.0)
        opt.step()
        sched.step()
        run_loss += loss.item()
        if step % 200 == 0:
            print(f"step {step} loss {run_loss / 200:.4f} lr {sched.get_last_lr()[0]:.2e} "
                  f"{(time.time() - t0) / step:.3f} s/step", flush=True)
            run_loss = 0.0
        if step % args.eval_every == 0 or step == args.steps:
            res = evaluate(model, valid, device, eager)
            rec = {"step": step, "time": time.time() - t0, **{f"{k}/{m}": round(v, 4) for k, r in res.items() for m, v in r.items()}}
            print(json.dumps(rec), flush=True)
            log.write(json.dumps(rec) + "\n")
            log.flush()
            ckpt = {"config": cfg.to_dict(), "model": model.state_dict(), "step": step, "metrics": rec}
            torch.save(ckpt, args.out / "last.pt")
            score = res.get("songs", {}).get("OA", 0.0)
            if score > best:
                best = score
                torch.save(ckpt, args.out / "best.pt")


if __name__ == "__main__":
    main()
