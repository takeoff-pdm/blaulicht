"""Train one head on cached mixtures, the backbone frozen.

    python -m analyze.fx_train.train --name both_beats --sources gen,yt --features beats

Sources decide which training mixtures it may see: gen, yt, or both. Thresholds come from the
synthetic validation set of the same sources, never from real songs.
"""

import argparse
import json
import time

import numpy as np
import torch
from torch import nn

from analyze import common
from analyze.fx_train.effects import CLASSES
from analyze.fx_train.head import Head, inputs
from analyze.fx_train.mixtures import OUT

MODELS = common.ROOT / "models/fx_head"


class Set:
    def __init__(self, names):
        self.parts = []
        for name in names:
            self.parts.append(tuple(np.load(OUT / f"{name}.{k}.npy", mmap_mode="r") for k in ("emb", "mel", "y", "m")))
        self.index = [(p, i) for p, part in enumerate(self.parts) for i in range(len(part[0]))]
        # the frame of each fake drop with no impact pasted near it, or -1
        self.empty = []
        for name in names:
            for m in json.loads((OUT / f"{name}.json").read_text()):
                t = m["fake_drop"]
                ok = t is not None and not any(e["kind"] == "impact" and abs(e["start"] - t) < 1.0 for e in m["events"])
                self.empty.append(int(t * 25) if ok else -1)

    def __len__(self):
        return len(self.index)

    def get(self, k):
        p, i = self.index[k]
        return tuple(a[i] for a in self.parts[p])


def f1(tp, fp, fn):
    return 2 * tp / max(2 * tp + fp + fn, 1)


@torch.no_grad()
def val_probs(head, val, features, mean, std):
    head.eval()
    P, Y, M = [], [], []
    for k in range(0, len(val), 16):
        items = [val.get(j) for j in range(k, min(k + 16, len(val)))]
        x = np.stack([inputs(e, m, features, mean, std) for e, m, _, _ in items])
        P.append(torch.sigmoid(head(torch.from_numpy(x).cuda())).cpu().numpy())
        Y.append(np.stack([i[2] for i in items]))
        M.append(np.stack([i[3] for i in items]))
    return np.concatenate(P), np.concatenate(Y), np.concatenate(M)


def frame_f1(P, Y, M, thr):
    out = []
    for j in range(P.shape[2]):
        keep = M[..., j] > 0
        p, y = P[..., j][keep] >= thr[j], Y[..., j][keep] > 0
        out.append(f1((p & y).sum(), (p & ~y).sum(), (~p & y).sum()))
    return np.array(out)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--name", required=True)
    ap.add_argument("--sources", required=True, help="gen, yt or gen,yt")
    ap.add_argument("--features", default="beats", choices=["beats", "mel", "both"])
    ap.add_argument("--epochs", type=int, default=24)
    ap.add_argument("--bs", type=int, default=32)
    ap.add_argument("--lr", type=float, default=1e-3)
    ap.add_argument("--hidden", type=int, default=192)
    ap.add_argument("--steps", type=int, default=180, help="steps per epoch: the same for every source mix")
    ap.add_argument("--train-sets", default="", help="override the cached sets to train on, for a quick check")
    ap.add_argument("--hard-negative", type=float, default=1.0, help="loss weight on impact frames around fake drops with nothing pasted")
    args = ap.parse_args()
    src = args.sources.split(",")
    tr, val = Set(args.train_sets.split(",") if args.train_sets else [f"train_{s}" for s in src]), Set([f"val_{s}" for s in src])
    rng = np.random.default_rng(1)
    sample = [tr.get(int(i)) for i in rng.choice(len(tr), min(256, len(tr)), replace=False)]
    stacked = np.stack([inputs(e, m, args.features, 0.0, 1.0) for e, m, _, _ in sample])
    mean, std = stacked.mean((0, 1), keepdims=False).astype(np.float32), stacked.std((0, 1)).astype(np.float32) + 1e-5
    Yall = np.stack([s[2] for s in sample]).astype(float)
    Mall = np.stack([s[3] for s in sample]).astype(float)
    pos = (Yall * Mall).sum((0, 1))
    neg = ((1 - Yall) * Mall).sum((0, 1))
    pw = torch.tensor(np.clip(np.sqrt(neg / np.maximum(pos, 1)), 1, 12), dtype=torch.float32).cuda()
    print("positive frames", dict(zip(CLASSES, np.round(pos / (pos + neg), 4))), "pos weight", np.round(pw.cpu().numpy(), 1), flush=True)
    head = Head(stacked.shape[2], args.hidden).cuda()
    opt = torch.optim.AdamW(head.parameters(), lr=args.lr, weight_decay=0.01)
    sched = torch.optim.lr_scheduler.OneCycleLR(opt, args.lr, total_steps=args.epochs * args.steps, pct_start=0.1)
    lossf = nn.BCEWithLogitsLoss(reduction="none", pos_weight=pw)
    best, best_state, t0 = -1, None, time.time()
    thr_half = np.full(len(CLASSES), 0.5)
    for epoch in range(args.epochs):
        head.train()
        total = 0.0
        for _ in range(args.steps):
            idx = rng.integers(0, len(tr), args.bs)
            items = [tr.get(int(i)) for i in idx]
            # a random stretch of each mixture, so it can't count on the window's edges
            ln = int(rng.integers(600, 1001))
            starts = rng.integers(0, 1000 - ln + 1, len(items))
            x = np.stack([inputs(e[s: s + ln], m[s: s + ln], args.features, mean, std) for (e, m, _, _), s in zip(items, starts)])
            y = np.stack([i[2][s: s + ln] for i, s in zip(items, starts)]).astype(np.float32)
            w = np.stack([i[3][s: s + ln] for i, s in zip(items, starts)]).astype(np.float32)
            if args.hard_negative != 1.0:
                ji = CLASSES.index("impact")
                for b, (k, st) in enumerate(zip(idx, starts)):
                    f = tr.empty[int(k)]
                    if f >= 0:
                        lo, hi = max(f - 12 - st, 0), min(f + 13 - st, ln)
                        if hi > lo:
                            w[b, lo: hi, ji] *= args.hard_negative
            xt = torch.from_numpy(x).cuda()
            drop = torch.rand(xt.shape[:2], device=xt.device) < 0.05  # whole frames, so it can't lean on single ones
            xt[drop] = 0
            yt, wt = torch.from_numpy(y).cuda(), torch.from_numpy(w).cuda()
            logit = head(xt)
            loss = (lossf(logit, yt) * wt).sum() / wt.sum().clamp(min=1)
            opt.zero_grad(set_to_none=True)
            loss.backward()
            nn.utils.clip_grad_norm_(head.parameters(), 5.0)
            opt.step()
            sched.step()
            total += float(loss)
        P, Y, M = val_probs(head, val, args.features, mean, std)
        f = frame_f1(P, Y, M, thr_half)
        print(f"{args.name} epoch {epoch + 1}/{args.epochs} loss {total / args.steps:.4f} val frame F1 {f.mean():.3f} "
              + " ".join(f"{c}:{v:.2f}" for c, v in zip(CLASSES, f)) + f" {time.time() - t0:.0f} s", flush=True)
        if f.mean() > best:
            best, best_state = float(f.mean()), {k: v.detach().cpu().clone() for k, v in head.state_dict().items()}
    head.load_state_dict(best_state)
    P, Y, M = val_probs(head, val, args.features, mean, std)
    grid = np.arange(0.1, 0.96, 0.05)
    thr = []
    for j in range(len(CLASSES)):
        scores = [frame_f1(P[..., [j]], Y[..., [j]], M[..., [j]], [t])[0] for t in grid]
        thr.append(float(grid[int(np.argmax(scores))]))
    f = frame_f1(P, Y, M, thr)
    print(f"{args.name} best val frame F1 at tuned thresholds {f.mean():.3f}", dict(zip(CLASSES, np.round(f, 2))), "thresholds", dict(zip(CLASSES, np.round(thr, 2))), flush=True)
    MODELS.mkdir(parents=True, exist_ok=True)
    torch.save({"state": best_state, "dim": stacked.shape[2], "hidden": args.hidden, "features": args.features, "sources": src, "mean": mean, "std": std,
                "thresholds": thr, "classes": CLASSES, "val_frame_f1": f.tolist(), "seconds": time.time() - t0}, MODELS / f"{args.name}.pt")


if __name__ == "__main__":
    main()
