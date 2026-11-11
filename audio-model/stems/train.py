"""Train the streaming stem student.

    python train.py --out runs/la50 --lookahead 50 --musdb 1 --edm 1 --fma 2

Group weights pick how often each data source is sampled. Validation every
--eval-every steps: utterance SDR (uSDR, MDX definition) per stem on 30 s
excerpts of the MUSDB18 validation tracks (ground truth) and the held-out
valid dance tracks (teacher stems).
"""

import argparse
import json
import math
import time
from dataclasses import fields
from pathlib import Path

import numpy as np
import torch

from data import Mixes, clips
from student import SOURCES, Student, StudentConfig

ROOT = Path("/localhome/sokrates/stems/data/teacher")


def usdr(ref: torch.Tensor, est: torch.Tensor) -> torch.Tensor:
    """(..., n) -> (...) in dB."""
    num = (ref**2).sum(-1) + 1e-8
    den = ((ref - est) ** 2).sum(-1) + 1e-8
    return 10 * torch.log10(num / den)


def load_valid(paths: list[Path], seconds: float = 30.0) -> list[tuple[torch.Tensor, torch.Tensor]]:
    out = []
    for p in paths:
        arr = np.load(p, mmap_mode="r")
        n = int(seconds * 44_100)
        start = max(0, (arr.shape[1] - n) // 2)
        x = torch.from_numpy(np.asarray(arr[:, start : start + n], np.float32))
        mix, d, b, v = x
        out.append((mix, torch.stack([d, b, mix - d - b - v, v])))
    return out


@torch.no_grad()
def evaluate(model, sets: dict, device) -> dict:
    """Aggregate SDR per stem: 10 log10(sum |s|^2 / sum |s - s_hat|^2) over
    all excerpts. Robust to excerpts where a stem is silent, which make the
    per-excerpt mean meaningless."""
    model.eval()
    res = {}
    for name, items in sets.items():
        num = torch.zeros(4)
        den = torch.zeros(4)
        for mix, stems in items:
            with torch.autocast("cuda", torch.bfloat16):
                est, _ = model(mix[None, None].to(device))
            ref = stems.to(device)
            num += (ref**2).sum(-1).cpu()
            den += ((ref - est[0, :, 0].float()) ** 2).sum(-1).cpu()
        s = 10 * torch.log10(num / den)
        res[name] = {src: round(float(v), 3) for src, v in zip(SOURCES, s)} | {"avg": round(float(s.mean()), 3)}
    model.train()
    return res


def loss_fn(model, mix, stems):
    est_wav, est = model(mix[:, None])  # est: (B, S, 1, bins, T)
    tgt = model.stft(stems)  # (B, S, bins, T)
    est = est[:, :, 0]
    spec = (torch.view_as_real(est) - torch.view_as_real(tgt)).abs().mean()
    mag = (est.abs() - tgt.abs()).abs().mean()
    wav = (est_wav[:, :, 0] - stems).abs().mean()
    return spec + mag + 10 * wav


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", type=Path, required=True)
    ap.add_argument("--steps", type=int, default=150_000)
    ap.add_argument("--batch", type=int, default=16)
    ap.add_argument("--seconds", type=float, default=6.0)
    ap.add_argument("--lr", type=float, default=1e-3)
    ap.add_argument("--musdb", type=float, default=1.0)
    ap.add_argument("--edm", type=float, default=1.0)
    ap.add_argument("--fma", type=float, default=2.0)
    ap.add_argument("--remix", type=float, default=0.5)
    ap.add_argument("--workers", type=int, default=10)
    ap.add_argument("--eval-every", type=int, default=5000)
    for f in fields(StudentConfig):
        if f.type in (int, float):
            ap.add_argument(f"--{f.name.replace('_', '-')}", type=f.type, default=f.default)
    args = ap.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)

    cfg = StudentConfig(**{f.name: getattr(args, f.name) for f in fields(StudentConfig) if f.type in (int, float)})
    device = torch.device("cuda")
    model = Student(cfg).to(device)
    model.blocks.forward = torch.compile(model.blocks.forward, dynamic=False)
    n_params = sum(p.numel() for p in model.parameters())
    print(f"params {n_params/1e6:.2f}M, {model.macs_per_frame()/1e6:.1f}M MAC/frame, "
          f"right pads {cfg.right_pads()}", flush=True)

    groups = [
        (clips(ROOT / "musdb_train", "train", "musdb"), args.musdb),
        (clips(ROOT / "edm", "train", "edm"), args.edm),
        (clips(ROOT / "fma", "train"), args.fma),
    ]
    print("train clips:", [len(g[0]) for g in groups], flush=True)
    valid = {
        "musdb_valid": load_valid(clips(ROOT / "musdb_train", "valid", "musdb")),
        "edm_valid": load_valid(clips(ROOT / "edm", "valid", "edm")),
    }
    valid = {k: v for k, v in valid.items() if v}
    ds = Mixes([g for g in groups if g[1] > 0], args.seconds, args.remix)
    dl = torch.utils.data.DataLoader(ds, batch_size=args.batch, num_workers=args.workers,
                                     pin_memory=True, persistent_workers=True, prefetch_factor=4)
    it = iter(dl)

    # Feature normalisation from real mixes.
    mixes = torch.cat([next(it)[0] for _ in range(8)]).to(device)
    model.fit_feature_stats(mixes)

    opt = torch.optim.AdamW(model.parameters(), lr=args.lr, weight_decay=1e-4)
    warm = 2000
    sched = torch.optim.lr_scheduler.LambdaLR(
        opt, lambda s: min(1, (s + 1) / warm) * 0.5 * (1 + math.cos(math.pi * min(1.0, s / args.steps))))

    log = open(args.out / "log.jsonl", "a")
    best, t0, running = -1e9, time.time(), 0.0
    for step in range(1, args.steps + 1):
        mix, stems = (t.to(device, non_blocking=True) for t in next(it))
        with torch.autocast("cuda", torch.bfloat16):
            loss = loss_fn(model, mix, stems)
        opt.zero_grad(set_to_none=True)
        loss.backward()
        gn = torch.nn.utils.clip_grad_norm_(model.parameters(), 5.0)
        opt.step()
        sched.step()
        running += loss.item()
        if step % 200 == 0:
            print(f"step {step} loss {running/200:.4f} gn {gn:.2f} lr {sched.get_last_lr()[0]:.2e} "
                  f"{(time.time()-t0)/step:.3f}s/step", flush=True)
            running = 0.0
        if step % args.eval_every == 0 or step == args.steps:
            res = evaluate(model, valid, device)
            score = np.mean([r["avg"] for r in res.values()])
            rec = {"step": step, "time": round(time.time() - t0), **res}
            print(json.dumps(rec), flush=True)
            log.write(json.dumps(rec) + "\n")
            log.flush()
            ckpt = {"config": cfg.to_dict(), "model": model.state_dict(), "step": step, "valid": res}
            torch.save(ckpt, args.out / "last.pt")
            if score > best:
                best = score
                torch.save(ckpt, args.out / "best.pt")


if __name__ == "__main__":
    main()
